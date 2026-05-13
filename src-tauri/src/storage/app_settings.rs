use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::settings::AppSetting;

pub fn get(conn: &Connection, key: &str) -> Result<Option<AppSetting>, AppError> {
    let result = conn.query_row(
        "SELECT key, value, updated_at FROM app_settings WHERE key = ?1",
        [key],
        |row| {
            Ok(AppSetting {
                key: row.get(0)?,
                value: row.get(1)?,
                updated_at: row.get(2)?,
            })
        },
    );

    match result {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::database(e)),
    }
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<AppSetting, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
        params![key, value, &now],
    )?;
    get(conn, key).map(|o| o.unwrap())
}
