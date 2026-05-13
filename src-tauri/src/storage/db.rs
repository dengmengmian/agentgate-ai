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
