use tauri::State;

use crate::app::state::AppState;
use crate::storage;

pub mod clients;
pub mod config;
pub mod diagnostics;
pub mod gateway;
pub mod instructions_skills;
pub mod logs;
pub mod pet;
pub mod pricing;
pub mod providers;
pub mod route_profiles;

pub use clients::*;
pub use config::*;
pub use diagnostics::*;
pub use gateway::*;
pub use instructions_skills::*;
pub use logs::*;
pub use pet::*;
pub use pricing::*;
pub use providers::*;
pub use route_profiles::*;

// ── Client apply history helper ────────────────────────────────

/// Snapshot the client's on-disk config files **before** the apply/disable/
/// toggle path rewrites them, and append one row to `client_apply_history`.
/// Failures are swallowed: losing one rollback point shouldn't break the
/// actual apply.
pub(super) fn record_pre_apply(
    state: &State<'_, AppState>,
    client_id: &str,
    action: &str,
    paths: Vec<(&'static str, std::path::PathBuf)>,
    summary: &str,
) {
    // Read off disk before acquiring the DB lock — file I/O may be slow.
    let snap = storage::apply_history::snapshot_files_at(&paths);
    let Ok(conn) = state.db.get() else { return };
    let _ = storage::apply_history::record(&conn, client_id, action, &snap, summary);
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    use super::*;
    use crate::app::state::AppState;
    use crate::test_utils::{cleanup, setup_temp_home, FS_LOCK};

    fn test_state() -> AppState {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        {
            let conn = pool.get().unwrap();
            crate::storage::migrations::run_migrations(&conn).unwrap();
        }
        AppState {
            db: pool,
            gateway_runtime: Arc::new(Mutex::new(
                crate::models::gateway::GatewayRuntimeState::default(),
            )),
            pet_click_through: Arc::new(Mutex::new(false)),
        }
    }

    unsafe fn as_state<'r>(state: &'r AppState) -> tauri::State<'r, AppState> {
        std::mem::transmute(state)
    }

    #[test]
    fn record_pre_apply_creates_history_entry() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        let cfg_path = temp.join("test-config.toml");
        std::fs::write(&cfg_path, "model_provider = \"openai\"\n").unwrap();

        record_pre_apply(
            unsafe { &as_state(&state) },
            "codex",
            "apply",
            vec![("config.toml", cfg_path)],
            "apply test config",
        );

        let conn = state.db.get().unwrap();
        let history = storage::apply_history::list(&conn, "codex").unwrap();
        assert!(!history.is_empty());
        assert_eq!(history[0].action, "apply");
        assert!(history[0].is_initial);
        cleanup(&temp);
    }
}
