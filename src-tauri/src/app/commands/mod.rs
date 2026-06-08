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
