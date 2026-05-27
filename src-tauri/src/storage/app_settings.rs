use rusqlite::{params, Connection};

use crate::errors::AppError;

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    )
    .optional()
    .map_err(AppError::database)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
        params![key, value, &now],
    )?;
    Ok(())
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_get_missing_key() {
        let conn = setup_db();
        let val = get(&conn, "nonexistent").unwrap();
        assert!(val.is_none());
    }

    #[test]
    fn test_set_and_get() {
        let conn = setup_db();
        set(&conn, "theme", "dark").unwrap();
        let val = get(&conn, "theme").unwrap();
        assert_eq!(val, Some("dark".into()));
    }

    #[test]
    fn test_set_overwrites() {
        let conn = setup_db();
        set(&conn, "lang", "en").unwrap();
        set(&conn, "lang", "zh").unwrap();
        let val = get(&conn, "lang").unwrap();
        assert_eq!(val, Some("zh".into()));
    }
}
