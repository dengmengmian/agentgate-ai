//! Client-local session log synchronisation.
//!
//! For users who sometimes go through AgentGate and sometimes use the
//! official CLI directly, the gateway can only see half their traffic.
//! This module bridges the gap by scanning the local session log files
//! that Claude Code / Codex / Gemini CLI write to disk, extracting per-message
//! token usage, and writing the rows into `request_logs` with
//! `source = '<client>_session'`.
//!
//! Every parsed row carries:
//!   - `external_id`: a stable identifier from the file (message id / event
//!     index). The sync layer SELECTs `external_id` already in the DB and
//!     skips them, so re-running the sync is idempotent.
//!   - `session_id`: the file's session uuid. Aggregates by-session view.
//!   - `timestamp`: the actual event time from the file, not `now()`.
//!
//! Parsers are best-effort: malformed lines are skipped silently, file IO
//! errors return an error in the SyncResult but don't crash; missing client
//! directories yield an empty result (the user might not have installed it).

pub mod claude;
pub mod codex;
pub mod gemini;
pub mod periodic;

use serde::{Deserialize, Serialize};

/// One sync run's outcome — same shape across all clients. Aggregated for
/// the GUI's "synced X conversations, Y new messages" status.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncResult {
    pub files_scanned: u32,
    pub imported: u32,
    pub skipped: u32,
    pub errors: Vec<String>,
}

impl SyncResult {
    pub fn merge(&mut self, other: SyncResult) {
        self.files_scanned += other.files_scanned;
        self.imported += other.imported;
        self.skipped += other.skipped;
        self.errors.extend(other.errors);
    }
}
