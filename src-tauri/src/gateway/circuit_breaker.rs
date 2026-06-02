//! Circuit breaker — three-state read-only view over `provider_runtime_status`.
//!
//! AgentGate already tracks `consecutive_failures` and `cooldown_until` per
//! provider via `storage::provider_runtime_status::mark_failure / mark_success`,
//! which gives us 90% of a breaker for free. This module just classifies the
//! current state so the failover loop in `provider_selector` and the GUI can
//! reason about it without re-implementing the math.
//!
//! Three states match the textbook breaker pattern:
//!   - `Closed`    : healthy, allow traffic.
//!   - `Open`      : recent failure, in cooldown, skip this provider.
//!   - `HalfOpen`  : cooldown elapsed, let the next request through as a probe;
//!                   `mark_success` on the probe closes the breaker, another
//!                   `mark_failure` reopens it with the same cooldown formula
//!                   the runtime_status layer already applies.
//!
//! Failure-threshold semantics: the existing `mark_failure` opens the breaker
//! on the *first* failure (consecutive_failures jumps from 0 → 1 and
//! `available` flips to 0). That's intentional — short cooldowns (60s default)
//! cost less than letting a broken provider eat the next request. If you want
//! N-strike behaviour, configure `cooldown_seconds` to 0 for the first N-1
//! failures via the route_profile_providers row.
//!
//! NOTE: this module deliberately does *not* write to runtime_status. Writes
//! still flow through `provider_runtime_status::mark_failure / mark_success`,
//! which is the single source of truth. We only classify.

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::errors::AppError;
use crate::storage::provider_runtime_status;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum CircuitState {
    /// Healthy. Let the request through.
    Closed,
    /// Tripped — `until` is the absolute wall-clock time when the breaker
    /// transitions to HalfOpen. Caller should skip this provider for now.
    Open { until: String },
    /// Cooldown elapsed; next request is a probe. Success closes the breaker,
    /// failure reopens it with the standard cooldown.
    HalfOpen,
}

/// Read the current breaker state for a provider. Creates the runtime_status
/// row on first query (the underlying `get` already does this). Cheap — one
/// indexed query.
pub fn check_state(conn: &Connection, provider_id: &str) -> Result<CircuitState, AppError> {
    let status = provider_runtime_status::get(conn, provider_id)?;
    if status.available {
        return Ok(CircuitState::Closed);
    }

    // `available = 0` means a recent failure exists. The breaker is either
    // Open (still in cooldown) or HalfOpen (cooldown elapsed but no success
    // recorded yet to close it).
    match status.cooldown_until.as_deref().and_then(parse_rfc3339) {
        Some(until) if until > Utc::now() => Ok(CircuitState::Open {
            until: until.to_rfc3339(),
        }),
        _ => Ok(CircuitState::HalfOpen),
    }
}

/// Should the failover loop attempt this provider on the current request?
/// `Closed` and `HalfOpen` say yes; `Open` says no.
pub fn should_attempt(conn: &Connection, provider_id: &str) -> Result<bool, AppError> {
    Ok(!matches!(
        check_state(conn, provider_id)?,
        CircuitState::Open { .. }
    ))
}

fn parse_rfc3339(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::migrations;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn fresh_provider_starts_closed() {
        let conn = setup();
        assert_eq!(check_state(&conn, "p1").unwrap(), CircuitState::Closed);
        assert!(should_attempt(&conn, "p1").unwrap());
    }

    #[test]
    fn failure_opens_breaker_with_future_cooldown() {
        let conn = setup();
        provider_runtime_status::mark_failure(&conn, "p1", "RATE", "rate limit", 60).unwrap();
        match check_state(&conn, "p1").unwrap() {
            CircuitState::Open { until } => {
                let until_dt = parse_rfc3339(&until).unwrap();
                assert!(
                    until_dt > Utc::now(),
                    "cooldown should still be in the future"
                );
            }
            other => panic!("expected Open, got {other:?}"),
        }
        assert!(!should_attempt(&conn, "p1").unwrap());
    }

    #[test]
    fn elapsed_cooldown_classifies_as_half_open() {
        let conn = setup();
        // Negative cooldown puts `cooldown_until` in the past immediately,
        // simulating the elapsed-cooldown case without sleeping.
        provider_runtime_status::mark_failure(&conn, "p1", "X", "boom", -10).unwrap();
        assert_eq!(check_state(&conn, "p1").unwrap(), CircuitState::HalfOpen);
        assert!(
            should_attempt(&conn, "p1").unwrap(),
            "HalfOpen must allow the probe request through"
        );
    }

    #[test]
    fn success_after_failure_closes_breaker() {
        let conn = setup();
        provider_runtime_status::mark_failure(&conn, "p1", "X", "boom", 60).unwrap();
        provider_runtime_status::mark_success(&conn, "p1").unwrap();
        assert_eq!(check_state(&conn, "p1").unwrap(), CircuitState::Closed);
    }

    #[test]
    fn missing_cooldown_until_after_failure_is_half_open() {
        // Edge case: a row with available=0 but cooldown_until=NULL (e.g. data
        // from an older migration). Classify as HalfOpen rather than Open
        // — better to retry than to block forever.
        let conn = setup();
        conn.execute(
            "INSERT INTO provider_runtime_status (provider_id, available, consecutive_failures, quota_exhausted, updated_at)
             VALUES ('weird', 0, 5, 0, ?1)",
            [&Utc::now().to_rfc3339()],
        )
        .unwrap();
        assert_eq!(check_state(&conn, "weird").unwrap(), CircuitState::HalfOpen);
    }
}
