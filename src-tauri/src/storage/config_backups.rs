use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigBackup {
    pub id: String,
    pub tool_type: String,
    pub source_path: String,
    pub backup_path: String,
    pub backup_kind: String,
    pub created_at: String,
    pub metadata_json: Option<String>,
}

pub fn insert(
    conn: &Connection,
    tool_type: &str,
    source_path: &str,
    backup_path: &str,
    backup_kind: &str,
    metadata_json: Option<&str>,
) -> Result<ConfigBackup, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO config_backups (id, tool_type, source_path, backup_path, backup_kind, created_at, metadata_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![&id, tool_type, source_path, backup_path, backup_kind, &now, metadata_json],
    )?;

    Ok(ConfigBackup {
        id, tool_type: tool_type.to_string(), source_path: source_path.to_string(),
        backup_path: backup_path.to_string(), backup_kind: backup_kind.to_string(),
        created_at: now, metadata_json: metadata_json.map(String::from),
    })
}

pub fn list_by_tool(conn: &Connection, tool_type: &str) -> Result<Vec<ConfigBackup>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, tool_type, source_path, backup_path, backup_kind, created_at, metadata_json
         FROM config_backups WHERE tool_type = ?1 ORDER BY created_at DESC LIMIT 20",
    )?;
    let rows = stmt.query_map([tool_type], |row| {
        Ok(ConfigBackup {
            id: row.get(0)?, tool_type: row.get(1)?, source_path: row.get(2)?,
            backup_path: row.get(3)?, backup_kind: row.get(4)?, created_at: row.get(5)?,
            metadata_json: row.get(6)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<ConfigBackup, AppError> {
    conn.query_row(
        "SELECT id, tool_type, source_path, backup_path, backup_kind, created_at, metadata_json
         FROM config_backups WHERE id = ?1",
        [id],
        |row| Ok(ConfigBackup {
            id: row.get(0)?, tool_type: row.get(1)?, source_path: row.get(2)?,
            backup_path: row.get(3)?, backup_kind: row.get(4)?, created_at: row.get(5)?,
            metadata_json: row.get(6)?,
        }),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found("ConfigBackup", id),
        other => AppError::database(other),
    })
}
