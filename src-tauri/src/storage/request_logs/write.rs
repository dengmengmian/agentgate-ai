//! 写入与清理：插入（网关请求 / 会话日志）、同步去重辅助、删除与保留期清理。

use rusqlite::Connection;

use crate::errors::AppError;

/// 删除某个会话在 request_logs 里的全部行，返回删除行数。
pub fn delete_by_session(conn: &Connection, session_id: &str) -> Result<usize, AppError> {
    let n = conn.execute(
        "DELETE FROM request_logs WHERE session_id = ?1",
        [session_id],
    )?;
    Ok(n)
}

#[allow(clippy::too_many_arguments)]
pub fn insert(
    conn: &Connection,
    request_id: &str,
    client: &str,
    provider: &str,
    model: &str,
    route: &str,
    status_code: i64,
    latency_ms: i64,
    raw_request: Option<&str>,
    converted_request: Option<&str>,
    raw_response: Option<&str>,
    converted_response: Option<&str>,
    sse_events: Option<&str>,
    tool_calls: Option<&str>,
    error_message: Option<&str>,
    trace_json: Option<&str>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cost: Option<f64>,
    cache_write_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    source: Option<&str>,
    session_id: Option<&str>,
    external_id: Option<&str>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    // 缺省视为 'gateway' —— 旧调用方迁移期间还没传，保持以前的语义。
    let source = source.unwrap_or("gateway");

    conn.execute(
        "INSERT INTO request_logs (id, request_id, timestamp, client, provider, model, route,
                status_code, latency_ms, raw_request, converted_request, raw_response,
                converted_response, sse_events, tool_calls, error_message, trace_json,
                input_tokens, output_tokens, cost, cache_write_tokens, cache_read_tokens,
                source, session_id, external_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
        rusqlite::params![
            &id, request_id, &now, client, provider, model, route,
            status_code, latency_ms, raw_request, converted_request, raw_response,
            converted_response, sse_events, tool_calls, error_message, trace_json,
            input_tokens, output_tokens, cost, cache_write_tokens, cache_read_tokens,
            source, session_id, external_id,
        ],
    )?;
    Ok(())
}

/// 给客户端会话日志同步器用：插入一条来自客户端本地日志的请求记录。
/// 与 `insert` 的差别：
///   - timestamp 来自调用方（文件里的事件时间），不是 now()
///   - 没有 raw_request / converted_request / response / SSE / tool_calls / error_message
///     —— 客户端日志只能给到 usage，不可能反推完整请求
///   - source / session_id / external_id 必填
///
/// 调用方应当先用 `external_ids_for_source` 过滤去重，再批量调这个函数。
#[allow(clippy::too_many_arguments)]
pub fn insert_session_log(
    conn: &Connection,
    timestamp: &str,
    client: &str,
    provider: &str,
    model: &str,
    route: &str,
    source: &str,
    session_id: &str,
    external_id: &str,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    cost: Option<f64>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    // request_id 我们没有真实值——客户端日志的 message_id 作为 request_id，方便用户在
    // Logs 详情里通过 request_id 列追溯到原文件里的那条消息。external_id 同时填同样的值，
    // 用作幂等 key 防止重复同步。
    conn.execute(
        "INSERT INTO request_logs (id, request_id, timestamp, client, provider, model, route,
                status_code, latency_ms,
                input_tokens, output_tokens, cost, cache_write_tokens, cache_read_tokens,
                source, session_id, external_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 200, 0, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            &id,
            external_id,
            timestamp,
            client,
            provider,
            model,
            route,
            input_tokens,
            output_tokens,
            cost,
            cache_write_tokens,
            cache_read_tokens,
            source,
            session_id,
            external_id,
        ],
    )?;
    Ok(())
}

/// 给客户端日志同步器用：从 DB 里查询某个 source 下已存在的 external_id 集合。
/// 同步前先调一次，把扫到的条目和这个集合做差集，避免重复插入。
pub fn external_ids_for_source(
    conn: &Connection,
    source: &str,
    candidates: &[String],
) -> Result<std::collections::HashSet<String>, AppError> {
    if candidates.is_empty() {
        return Ok(std::collections::HashSet::new());
    }
    // SQLite 单语句最多约 32k 个 placeholder；这里取 800 为一批，留足余量。
    let mut found = std::collections::HashSet::new();
    for chunk in candidates.chunks(800) {
        let placeholders = (1..=chunk.len())
            .map(|i| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT external_id FROM request_logs
             WHERE source = ?1 AND external_id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::types::ToSql> =
            vec![&source as &dyn rusqlite::types::ToSql];
        for c in chunk {
            params.push(c as &dyn rusqlite::types::ToSql);
        }
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            r.get::<_, String>(0)
        })?;
        for r in rows {
            if let Ok(id) = r {
                found.insert(id);
            }
        }
    }
    Ok(found)
}

/// Pull `(cache_write_tokens, cache_read_tokens)` out of any supported upstream
/// usage shape. Returns `(None, None)` when neither is present so the row
/// keeps "unknown" semantics rather than misleading zeroes.
///
/// Recognised shapes:
///   - Anthropic Messages: `cache_creation_input_tokens` / `cache_read_input_tokens`
///   - OpenAI Responses: `input_tokens_details.cached_tokens` (Read only)
///   - OpenAI Chat Completions: `prompt_tokens_details.cached_tokens` (Read only)
///   - Bare field used by some Chinese providers: `cached_tokens` (Read only)
pub fn extract_cache_tokens(usage: &serde_json::Value) -> (Option<i64>, Option<i64>) {
    let write = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_i64());
    let read = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(|v| v.as_i64())
        })
        .or_else(|| {
            usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(|v| v.as_i64())
        })
        .or_else(|| usage.get("cached_tokens").and_then(|v| v.as_i64()));
    (write, read)
}

pub fn clear(conn: &Connection) -> Result<bool, AppError> {
    conn.execute("DELETE FROM request_logs", [])?;
    Ok(true)
}

/// Delete logs older than `retention_days`. Returns the number of deleted rows.
pub fn cleanup_older_than(conn: &Connection, retention_days: i64) -> Result<usize, AppError> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(retention_days)).to_rfc3339();
    let deleted = conn.execute("DELETE FROM request_logs WHERE timestamp < ?1", [&cutoff])?;
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::request_log::RequestLogFilter;
    use crate::storage::request_logs::query;
    use rusqlite::Connection;

    fn empty_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE request_logs (
                id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                client TEXT,
                provider TEXT,
                model TEXT,
                route TEXT,
                status_code INTEGER,
                latency_ms INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                raw_request TEXT,
                converted_request TEXT,
                raw_response TEXT,
                converted_response TEXT,
                sse_events TEXT,
                tool_calls TEXT,
                error_message TEXT,
                cost REAL,
                trace_json TEXT,
                cache_write_tokens INTEGER,
                cache_read_tokens INTEGER,
                source TEXT,
                session_id TEXT,
                external_id TEXT
            );",
        )
        .unwrap();
        conn
    }

    fn empty_filter() -> RequestLogFilter {
        RequestLogFilter {
            client: None,
            provider: None,
            model: None,
            route_profile_id: None,
            status: None,
            error_type: None,
            keyword: None,
            source: None,
            session_id: None,
            limit: Some(100),
            offset: Some(0),
        }
    }

    #[test]
    fn insert_creates_gateway_row_with_defaults() {
        let conn = empty_db();
        insert(
            &conn,
            "req-1",
            "Codex",
            "openai_official",
            "gpt-5",
            "/v1/responses",
            200,
            120,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(10),
            Some(20),
            Some(0.001),
            Some(1),
            Some(2),
            None,
            Some("sess-1"),
            None,
        )
        .unwrap();

        let rows = query::list(&conn, empty_filter()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].request_id, "req-1");
        assert_eq!(rows[0].source, Some("gateway".to_string())); // default source
        assert_eq!(rows[0].session_id, Some("sess-1".to_string()));
    }

    #[test]
    fn insert_session_log_creates_client_session_row() {
        let conn = empty_db();
        insert_session_log(
            &conn,
            "2026-06-01T12:00:00Z",
            "Codex",
            "openai_official",
            "gpt-5",
            "/v1/responses",
            "codex_session",
            "sess-a",
            "ext-1",
            Some(5),
            Some(15),
            Some(0),
            Some(1),
            Some(0.0005),
        )
        .unwrap();

        let rows = query::list(&conn, empty_filter()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].client, Some("Codex".to_string()));
        assert_eq!(rows[0].source, Some("codex_session".to_string()));
        assert_eq!(rows[0].status_code, Some(200));
        assert_eq!(rows[0].latency_ms, Some(0));
    }

    #[test]
    fn delete_by_session_removes_only_target_session() {
        let conn = empty_db();
        insert(
            &conn, "r1", "c", "p", "m", "/r", 200, 0, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, Some("s1"), None,
        )
        .unwrap();
        insert(
            &conn, "r2", "c", "p", "m", "/r", 200, 0, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, Some("s2"), None,
        )
        .unwrap();

        let deleted = delete_by_session(&conn, "s1").unwrap();
        assert_eq!(deleted, 1);

        let rows = query::list(&conn, empty_filter()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id, Some("s2".to_string()));
    }

    #[test]
    fn external_ids_for_source_returns_existing_ids() {
        let conn = empty_db();
        insert_session_log(&conn, "2026-06-01T12:00:00Z", "c", "p", "m", "/r", "codex_session", "s", "ext-a", None, None, None, None, None).unwrap();
        insert_session_log(&conn, "2026-06-01T12:00:01Z", "c", "p", "m", "/r", "codex_session", "s", "ext-b", None, None, None, None, None).unwrap();

        let found = external_ids_for_source(&conn, "codex_session", &["ext-a".to_string(), "ext-c".to_string()]).unwrap();
        assert!(found.contains("ext-a"));
        assert!(!found.contains("ext-b"));
        assert!(!found.contains("ext-c"));
    }

    #[test]
    fn external_ids_for_source_empty_candidates_returns_empty() {
        let conn = empty_db();
        let found = external_ids_for_source(&conn, "codex_session", &[]).unwrap();
        assert!(found.is_empty());
    }

    #[test]
    fn clear_removes_all_rows() {
        let conn = empty_db();
        insert(
            &conn, "r1", "c", "p", "m", "/r", 200, 0, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None, None,
        )
        .unwrap();
        assert!(clear(&conn).unwrap());
        let rows = query::list(&conn, empty_filter()).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn cleanup_older_than_deletes_only_old_rows() {
        let conn = empty_db();
        let old = (chrono::Utc::now() - chrono::Duration::days(10)).to_rfc3339();
        let recent = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();

        insert_session_log(&conn, &old, "c", "p", "m", "/r", "codex_session", "s1", "old-1", None, None, None, None, None).unwrap();
        insert_session_log(&conn, &recent, "c", "p", "m", "/r", "codex_session", "s2", "new-1", None, None, None, None, None).unwrap();

        let deleted = cleanup_older_than(&conn, 7).unwrap();
        assert_eq!(deleted, 1);

        let rows = query::list(&conn, empty_filter()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id, Some("s2".to_string()));
    }
}
