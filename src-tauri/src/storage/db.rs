use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;

use crate::errors::AppError;
use crate::storage::migrations;

/// 全局数据库连接池类型别名。AppState 持有 Clone 的 handle,
/// 各处用 `pool.get()` 拿 `PooledConnection`(deref 到 `Connection`)。
pub type DbPool = Pool<SqliteConnectionManager>;

/// 从池借出的连接(deref 到 `Connection`)。一次性 CLI 子命令借一条用完即还。
pub type DbConn = r2d2::PooledConnection<SqliteConnectionManager>;

/// 池大小。SQLite WAL 模式允许多 reader 并发,写仍内部串行。
/// 桌面应用 QPS 不大,4 个 connection 足够吸收短期 burst。
const POOL_MAX_SIZE: u32 = 4;

/// 每个连接初始化时统一开 WAL + 外键约束,跟旧单连接实现保持行为一致。
fn init_connection(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    // WAL 下多连接并发写仍会 SQLITE_BUSY。给 5s 重试窗口,避免高并发瞬间直接失败。
    conn.execute_batch("PRAGMA busy_timeout=5000;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(())
}

pub fn init_database(app_data_dir: &PathBuf) -> Result<DbPool, AppError> {
    fs::create_dir_all(app_data_dir)
        .map_err(|e| AppError::internal(format!("Failed to create app data directory: {e}")))?;

    let db_path = app_data_dir.join("agentgate.db");

    let manager = SqliteConnectionManager::file(&db_path).with_init(init_connection);
    let pool = Pool::builder()
        .max_size(POOL_MAX_SIZE)
        .build(manager)
        .map_err(|e| AppError::internal(format!("Failed to build DB pool: {e}")))?;

    // migrations 在 pool ready 后跑一次:借一个连接、跑完归还。
    {
        let conn = pool
            .get()
            .map_err(|e| AppError::internal(format!("Failed to acquire connection: {e}")))?;
        migrations::run_migrations(&conn)?;
    }

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_database_in_memory() {
        let temp = std::env::temp_dir().join("agentgate_test_db");
        let pool = init_database(&temp).unwrap();
        let conn = pool.get().unwrap();
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

    #[test]
    fn test_pool_concurrent_reads() {
        let temp = std::env::temp_dir().join("agentgate_test_db_concurrent");
        let pool = init_database(&temp).unwrap();
        // 同时拿 2 个 connection,确认 Pool 没把它们 serialize
        let c1 = pool.get().unwrap();
        let c2 = pool.get().unwrap();
        let n1: i64 = c1.query_row("SELECT 1", [], |r| r.get(0)).unwrap();
        let n2: i64 = c2.query_row("SELECT 2", [], |r| r.get(0)).unwrap();
        assert_eq!(n1, 1);
        assert_eq!(n2, 2);
        drop(c1);
        drop(c2);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
