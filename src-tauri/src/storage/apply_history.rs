//! 客户端配置版本史。
//!
//! 每次 5 个客户端（codex / claude_code / opencode / gemini / atomcode）的
//! apply / disable / toggle 入口在写盘前打一次盘上配置文件的快照，写进
//! `client_apply_history`。用户在 UI 上选某条历史即可一键回滚到那个时点
//! 的磁盘状态。
//!
//! 设计要点：
//! - **snapshot 是盘上文件原文**（base64-encoded by the caller — caller 决定
//!   编码方式以容纳二进制；当前所有客户端都是文本配置，直接 raw string）。
//!   AgentGate 内部 state（active_provider_id 之类）不归这里管。
//! - **保留策略**：每个 client 第一条 `initial` 永久保留 + 最近 10 条滚动。
//!   apply / disable / toggle 都按"新历史"插入，写满后挤掉最老的非-initial。
//! - **回滚不写历史**：rollback 本身不产生新条目，否则用户连续多次回滚会
//!   把历史撑爆。但 rollback 之后的下一次 apply 仍会正常 record。

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

const MAX_NON_INITIAL_PER_CLIENT: usize = 10;

/// 一条历史条目的可序列化形态。`snapshot_json` 是 `ClientSnapshot` 序列化
/// 后的字符串，反序列化交给 caller —— 5 个客户端各自知道怎么 restore。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct HistoryEntry {
    pub id: String,
    pub client_id: String,
    /// `apply` / `disable` / `toggle_to_agentgate` / `toggle_to_official`
    pub action: String,
    /// 序列化后的 `ClientSnapshot`。
    pub snapshot_json: String,
    /// 一句话摘要：changed_keys 拼起来 / "switch to official" / 等。
    pub summary: String,
    /// 同客户端第一条永远是 initial=true，从不被清理。
    pub is_initial: bool,
    pub agentgate_version: String,
    /// RFC3339。
    pub created_at: String,
}

/// 5 个客户端 snapshot 的统一结构。每个 file_name 是相对名（如 "config.toml"
/// / "auth.json" / "settings.json"），content 是 UTF-8 raw（当前所有客户端
/// 的配置都是文本）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientSnapshot {
    pub files: Vec<SnapshotFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFile {
    /// 相对文件名，仅用于 UI 展示和 restore 时区分。
    pub name: String,
    /// 写入这一条 snapshot 时的绝对路径。restore 按这里写回去。
    pub absolute_path: String,
    /// 文件存不存在；不存在的话 content 为空字符串，restore 时要把对应文件
    /// 删掉而不是写入空内容（"配置不存在"和"配置为空"语义不同）。
    pub existed: bool,
    /// UTF-8 文件内容。existed=false 时为空字符串。
    pub content: String,
}

/// Build a snapshot by reading each `(display_name, absolute_path)` off
/// disk. Missing files become `existed: false, content: ""` rows so the
/// restore path knows to delete instead of write-empty.
pub fn snapshot_files_at(paths: &[(&str, std::path::PathBuf)]) -> ClientSnapshot {
    let files = paths
        .iter()
        .map(|(name, path)| {
            let (existed, content) = match std::fs::read_to_string(path) {
                Ok(s) => (true, s),
                Err(_) => (false, String::new()),
            };
            SnapshotFile {
                name: (*name).to_string(),
                absolute_path: path.to_string_lossy().to_string(),
                existed,
                content,
            }
        })
        .collect();
    ClientSnapshot { files }
}

/// Write each file back to its captured absolute path. For files that didn't
/// exist at snapshot time, the current on-disk file is removed (if any).
/// Parent dirs are created on demand. Errors short-circuit and report which
/// file failed.
pub fn restore_files(snapshot: &ClientSnapshot) -> Result<(), AppError> {
    use std::fs;
    for file in &snapshot.files {
        let path = std::path::PathBuf::from(&file.absolute_path);
        if file.existed {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    AppError::new(
                        "CLIENT_RESTORE_FAILED",
                        format!("Cannot create parent of {}: {e}", file.name),
                    )
                })?;
            }
            fs::write(&path, &file.content).map_err(|e| {
                AppError::new(
                    "CLIENT_RESTORE_FAILED",
                    format!("Cannot write {}: {e}", file.name),
                )
            })?;
        } else if path.exists() {
            fs::remove_file(&path).map_err(|e| {
                AppError::new(
                    "CLIENT_RESTORE_FAILED",
                    format!("Cannot remove {}: {e}", file.name),
                )
            })?;
        }
    }
    Ok(())
}

/// Insert one new history row. Trims older non-initial rows so each client
/// keeps at most `MAX_NON_INITIAL_PER_CLIENT` non-initial entries plus its
/// initial row.
pub fn record(
    conn: &Connection,
    client_id: &str,
    action: &str,
    snapshot: &ClientSnapshot,
    summary: &str,
) -> Result<String, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let version = env!("CARGO_PKG_VERSION").to_string();

    // First row for this client gets is_initial=1.
    let existing_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM client_apply_history WHERE client_id = ?1",
        params![client_id],
        |r| r.get(0),
    )?;
    let is_initial = if existing_count == 0 { 1 } else { 0 };

    let snapshot_json = serde_json::to_string(snapshot)
        .map_err(|e| AppError::internal(format!("snapshot serialise failed: {e}")))?;

    conn.execute(
        "INSERT INTO client_apply_history
         (id, client_id, action, snapshot_json, summary, is_initial, agentgate_version, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            &id,
            client_id,
            action,
            &snapshot_json,
            summary,
            is_initial,
            &version,
            &now
        ],
    )?;

    trim_old(conn, client_id)?;
    Ok(id)
}

/// Drop non-initial rows beyond `MAX_NON_INITIAL_PER_CLIENT` keeping the
/// newest. Initial rows are never deleted.
fn trim_old(conn: &Connection, client_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM client_apply_history
         WHERE id IN (
             SELECT id FROM client_apply_history
             WHERE client_id = ?1 AND is_initial = 0
             ORDER BY created_at DESC
             LIMIT -1 OFFSET ?2
         )",
        params![client_id, MAX_NON_INITIAL_PER_CLIENT as i64],
    )?;
    Ok(())
}

pub fn list(conn: &Connection, client_id: &str) -> Result<Vec<HistoryEntry>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, client_id, action, snapshot_json, summary, is_initial,
                agentgate_version, created_at
         FROM client_apply_history
         WHERE client_id = ?1
         ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map(params![client_id], row_to_entry)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 曾经 apply 过配置的客户端 id 列表——用于「配置漂移」判断：detected 但接入过
/// 的客户端说明配置被改回去了，提示重新应用。
pub fn distinct_clients(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut stmt = conn.prepare("SELECT DISTINCT client_id FROM client_apply_history")?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &Connection, id: &str) -> Result<HistoryEntry, AppError> {
    conn.query_row(
        "SELECT id, client_id, action, snapshot_json, summary, is_initial,
                agentgate_version, created_at
         FROM client_apply_history
         WHERE id = ?1",
        params![id],
        row_to_entry,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found("client_apply_history", id),
        other => AppError::from(other),
    })
}

fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
    Ok(HistoryEntry {
        id: row.get(0)?,
        client_id: row.get(1)?,
        action: row.get(2)?,
        snapshot_json: row.get(3)?,
        summary: row.get(4)?,
        is_initial: row.get::<_, i64>(5)? != 0,
        agentgate_version: row.get(6)?,
        created_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&conn).unwrap();
        conn
    }

    fn dummy_snapshot(content: &str) -> ClientSnapshot {
        ClientSnapshot {
            files: vec![SnapshotFile {
                name: "config.toml".to_string(),
                absolute_path: "/tmp/codex/config.toml".to_string(),
                existed: true,
                content: content.to_string(),
            }],
        }
    }

    #[test]
    fn first_record_per_client_is_marked_initial() {
        let conn = setup();
        let id = record(&conn, "codex", "apply", &dummy_snapshot("a"), "first").unwrap();
        let entry = get(&conn, &id).unwrap();
        assert!(entry.is_initial);
    }

    #[test]
    fn subsequent_records_are_not_initial() {
        let conn = setup();
        record(&conn, "codex", "apply", &dummy_snapshot("a"), "1").unwrap();
        let id2 = record(&conn, "codex", "apply", &dummy_snapshot("b"), "2").unwrap();
        let entry = get(&conn, &id2).unwrap();
        assert!(!entry.is_initial);
    }

    #[test]
    fn list_returns_newest_first() {
        let conn = setup();
        record(&conn, "codex", "apply", &dummy_snapshot("a"), "1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        record(&conn, "codex", "apply", &dummy_snapshot("b"), "2").unwrap();
        let rows = list(&conn, "codex").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].summary, "2");
        assert_eq!(rows[1].summary, "1");
    }

    #[test]
    fn retention_keeps_initial_plus_10_most_recent() {
        let conn = setup();
        for i in 0..15 {
            record(
                &conn,
                "codex",
                "apply",
                &dummy_snapshot(&i.to_string()),
                &i.to_string(),
            )
            .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let rows = list(&conn, "codex").unwrap();
        // 1 initial (i=0) + 10 most-recent non-initial = 11
        assert_eq!(rows.len(), 11);
        // Initial row is the original (summary="0").
        assert!(rows.iter().any(|r| r.is_initial && r.summary == "0"));
        // Newest non-initial is i=14.
        assert_eq!(rows[0].summary, "14");
    }

    #[test]
    fn retention_per_client_isolation() {
        let conn = setup();
        record(&conn, "codex", "apply", &dummy_snapshot("c1"), "c1").unwrap();
        record(&conn, "claude_code", "apply", &dummy_snapshot("cl1"), "cl1").unwrap();
        let codex_rows = list(&conn, "codex").unwrap();
        let claude_rows = list(&conn, "claude_code").unwrap();
        assert_eq!(codex_rows.len(), 1);
        assert_eq!(claude_rows.len(), 1);
        assert!(codex_rows[0].is_initial);
        assert!(claude_rows[0].is_initial);
    }

    #[test]
    fn snapshot_json_roundtrips_through_storage() {
        let conn = setup();
        let snap = ClientSnapshot {
            files: vec![
                SnapshotFile {
                    name: "config.toml".into(),
                    absolute_path: "/x".into(),
                    existed: true,
                    content: "[k]\nv=1".into(),
                },
                SnapshotFile {
                    name: "auth.json".into(),
                    absolute_path: "/y".into(),
                    existed: false,
                    content: "".into(),
                },
            ],
        };
        let id = record(&conn, "codex", "apply", &snap, "test").unwrap();
        let entry = get(&conn, &id).unwrap();
        let restored: ClientSnapshot = serde_json::from_str(&entry.snapshot_json).unwrap();
        assert_eq!(restored.files.len(), 2);
        assert!(restored.files[1].existed == false);
        assert_eq!(restored.files[0].content, "[k]\nv=1");
    }
}
