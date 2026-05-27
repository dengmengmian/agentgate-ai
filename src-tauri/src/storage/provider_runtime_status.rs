use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::route_profile::ProviderRuntimeStatus;

pub fn get(conn: &Connection, provider_id: &str) -> Result<ProviderRuntimeStatus, AppError> {
    let result = conn.query_row(
        "SELECT provider_id, available, consecutive_failures, last_error, last_error_code,
                last_error_at, cooldown_until, quota_exhausted, updated_at
         FROM provider_runtime_status WHERE provider_id = ?1",
        [provider_id],
        |row| Ok(ProviderRuntimeStatus {
            provider_id: row.get(0)?, available: row.get(1)?, consecutive_failures: row.get(2)?,
            last_error: row.get(3)?, last_error_code: row.get(4)?, last_error_at: row.get(5)?,
            cooldown_until: row.get(6)?, quota_exhausted: row.get(7)?, updated_at: row.get(8)?,
        }),
    );
    match result {
        Ok(s) => Ok(s),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            // Create default
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT OR IGNORE INTO provider_runtime_status (provider_id, available, consecutive_failures, quota_exhausted, updated_at)
                 VALUES (?1, 1, 0, 0, ?2)",
                params![provider_id, &now],
            )?;
            Ok(ProviderRuntimeStatus {
                provider_id: provider_id.to_string(), available: true, consecutive_failures: 0,
                last_error: None, last_error_code: None, last_error_at: None,
                cooldown_until: None, quota_exhausted: false, updated_at: now,
            })
        }
        Err(e) => Err(AppError::database(e)),
    }
}

pub fn list_all(conn: &Connection) -> Result<Vec<ProviderRuntimeStatus>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT provider_id, available, consecutive_failures, last_error, last_error_code,
                last_error_at, cooldown_until, quota_exhausted, updated_at
         FROM provider_runtime_status ORDER BY provider_id",
    )?;
    let rows = stmt.query_map([], |row| Ok(ProviderRuntimeStatus {
        provider_id: row.get(0)?, available: row.get(1)?, consecutive_failures: row.get(2)?,
        last_error: row.get(3)?, last_error_code: row.get(4)?, last_error_at: row.get(5)?,
        cooldown_until: row.get(6)?, quota_exhausted: row.get(7)?, updated_at: row.get(8)?,
    }))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

pub fn mark_failure(conn: &Connection, provider_id: &str, error_code: &str, error_msg: &str, cooldown_seconds: i64) -> Result<(), AppError> {
    let now = chrono::Utc::now();
    let cooldown_until = (now + chrono::Duration::seconds(cooldown_seconds)).to_rfc3339();
    let now_str = now.to_rfc3339();
    let is_quota = error_msg.to_lowercase().contains("quota") || error_msg.to_lowercase().contains("insufficient balance");

    conn.execute(
        "INSERT INTO provider_runtime_status (provider_id, available, consecutive_failures, last_error, last_error_code, last_error_at, cooldown_until, quota_exhausted, updated_at)
         VALUES (?1, 0, 1, ?2, ?3, ?4, ?5, ?6, ?4)
         ON CONFLICT(provider_id) DO UPDATE SET
           available = 0,
           consecutive_failures = consecutive_failures + 1,
           last_error = ?2,
           last_error_code = ?3,
           last_error_at = ?4,
           cooldown_until = ?5,
           quota_exhausted = CASE WHEN ?6 THEN 1 ELSE quota_exhausted END,
           updated_at = ?4",
        params![provider_id, error_msg, error_code, &now_str, &cooldown_until, is_quota],
    )?;
    Ok(())
}

pub fn mark_success(conn: &Connection, provider_id: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO provider_runtime_status (provider_id, available, consecutive_failures, quota_exhausted, updated_at)
         VALUES (?1, 1, 0, 0, ?2)
         ON CONFLICT(provider_id) DO UPDATE SET
           available = 1,
           consecutive_failures = 0,
           cooldown_until = NULL,
           updated_at = ?2",
        params![provider_id, &now],
    )?;
    Ok(())
}

pub fn reset(conn: &Connection, provider_id: &str) -> Result<ProviderRuntimeStatus, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO provider_runtime_status (provider_id, available, consecutive_failures, last_error, last_error_code, last_error_at, cooldown_until, quota_exhausted, updated_at)
         VALUES (?1, 1, 0, NULL, NULL, NULL, NULL, 0, ?2)
         ON CONFLICT(provider_id) DO UPDATE SET
           available = 1, consecutive_failures = 0, last_error = NULL, last_error_code = NULL,
           last_error_at = NULL, cooldown_until = NULL, quota_exhausted = 0, updated_at = ?2",
        params![provider_id, &now],
    )?;
    get(conn, provider_id)
}

pub fn reset_all(conn: &Connection) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE provider_runtime_status SET available=1, consecutive_failures=0, last_error=NULL, last_error_code=NULL, last_error_at=NULL, cooldown_until=NULL, quota_exhausted=0, updated_at=?1",
        [&now],
    )?;
    Ok(())
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
    fn test_get_creates_default_when_missing() {
        let conn = setup_db();
        let status = get(&conn, "p1").unwrap();
        assert_eq!(status.provider_id, "p1");
        assert!(status.available);
        assert_eq!(status.consecutive_failures, 0);
        assert!(!status.quota_exhausted);
    }

    #[test]
    fn test_mark_failure_and_success() {
        let conn = setup_db();
        mark_failure(&conn, "p1", "RATE_LIMIT", "rate limit exceeded", 60).unwrap();
        let status = get(&conn, "p1").unwrap();
        assert!(!status.available);
        assert_eq!(status.consecutive_failures, 1);
        assert_eq!(status.last_error_code, Some("RATE_LIMIT".to_string()));
        assert!(status.cooldown_until.is_some());

        mark_success(&conn, "p1").unwrap();
        let status = get(&conn, "p1").unwrap();
        assert!(status.available);
        assert_eq!(status.consecutive_failures, 0);
        assert!(status.cooldown_until.is_none());
    }

    #[test]
    fn test_mark_failure_quota_detection() {
        let conn = setup_db();
        mark_failure(&conn, "p1", "INSUFFICIENT_QUOTA", "insufficient balance", 60).unwrap();
        let status = get(&conn, "p1").unwrap();
        assert!(status.quota_exhausted);
    }

    #[test]
    fn test_reset() {
        let conn = setup_db();
        mark_failure(&conn, "p1", "ERROR", "fail", 60).unwrap();
        let status = reset(&conn, "p1").unwrap();
        assert!(status.available);
        assert_eq!(status.consecutive_failures, 0);
        assert!(status.cooldown_until.is_none());
        assert!(!status.quota_exhausted);
    }

    #[test]
    fn test_reset_all() {
        let conn = setup_db();
        mark_failure(&conn, "p1", "ERROR", "fail", 60).unwrap();
        mark_failure(&conn, "p2", "ERROR", "fail", 60).unwrap();
        reset_all(&conn).unwrap();
        let s1 = get(&conn, "p1").unwrap();
        let s2 = get(&conn, "p2").unwrap();
        assert!(s1.available);
        assert!(s2.available);
    }

    #[test]
    fn test_list_all() {
        let conn = setup_db();
        get(&conn, "p1").unwrap();
        get(&conn, "p2").unwrap();
        let all = list_all(&conn).unwrap();
        let ids: Vec<_> = all.iter().map(|s| s.provider_id.as_str()).collect();
        assert!(ids.contains(&"p1"));
        assert!(ids.contains(&"p2"));
    }
}

