use rusqlite::Connection;
use std::path::PathBuf;
use std::fs;

use crate::errors::AppError;
use crate::storage::migrations;

pub fn init_database(app_data_dir: &PathBuf) -> Result<Connection, AppError> {
    fs::create_dir_all(app_data_dir).map_err(|e| {
        AppError::internal(format!("Failed to create app data directory: {e}"))
    })?;

    let db_path = app_data_dir.join("agentgate.db");
    let conn = Connection::open(&db_path)?;

    // Enable WAL mode for better concurrent performance
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    migrations::run_migrations(&conn)?;

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_database_in_memory() {
        let temp = std::env::temp_dir().join("agentgate_test_db");
        let conn = init_database(&temp).unwrap();
        // Verify WAL mode is enabled
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
        // Verify foreign keys are enabled
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
        // Verify key tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(tables.contains(&"providers".to_string()));
        assert!(tables.contains(&"gateway_settings".to_string()));
        assert!(tables.contains(&"route_profiles".to_string()));
        assert!(tables.contains(&"request_logs".to_string()));
        assert!(tables.contains(&"model_pricing".to_string()));
        assert!(tables.contains(&"pet_settings".to_string()));
        // Cleanup
        let _ = std::fs::remove_dir_all(&temp);
    }
}
