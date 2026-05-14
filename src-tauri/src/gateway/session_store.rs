//! Dual-layer session store: L1 in-memory + L2 SQLite.
//!
//! L1: HashMap with LRU eviction (1000 entries, access-counter based).
//! L2: SQLite file at ~/.agentgate/sessions.db (survives restarts).
//! L1 miss → L2 lookup → L1 warm-up.

use std::collections::HashMap;
use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};

use crate::protocol::chat_completions::ChatMessage;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StoredTurn {
    pub messages: Vec<ChatMessage>,
    pub assistant_messages: Vec<ChatMessage>,
    pub response_id: String,
    pub reasoning_content: Option<String>,
    access: u64,
}

static L1: Mutex<Option<HashMap<String, StoredTurn>>> = Mutex::new(None);
static COUNTER: AtomicU64 = AtomicU64::new(0);
const MAX_L1: usize = 1000;

fn next() -> u64 { COUNTER.fetch_add(1, Ordering::Relaxed) }

fn with_l1<F, R>(f: F) -> R where F: FnOnce(&mut HashMap<String, StoredTurn>) -> R {
    let mut g = L1.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() { *g = Some(HashMap::new()); }
    f(g.as_mut().unwrap())
}

/// Get or create the L2 SQLite connection path.
fn l2_path() -> std::path::PathBuf {
    crate::security::local_token::token_dir().join("sessions.db")
}

fn ensure_l2() -> Option<rusqlite::Connection> {
    let path = l2_path();
    let _ = std::fs::create_dir_all(path.parent()?);
    let conn = rusqlite::Connection::open(&path).ok()?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;").ok()?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
            response_id TEXT PRIMARY KEY,
            data TEXT NOT NULL,
            created_at TEXT NOT NULL
        )"
    ).ok()?;
    Some(conn)
}

/// Store a completed turn in L1 + L2.
pub fn store_turn(
    response_id: &str,
    messages: Vec<ChatMessage>,
    assistant_messages: Vec<ChatMessage>,
    reasoning_content: Option<String>,
) {
    let turn = StoredTurn {
        messages: messages.clone(),
        assistant_messages: assistant_messages.clone(),
        response_id: response_id.to_string(),
        reasoning_content: reasoning_content.clone(),
        access: next(),
    };

    // L1 store with LRU eviction
    with_l1(|map| {
        if map.len() >= MAX_L1 {
            let mut entries: Vec<(String, u64)> = map.iter().map(|(k, v)| (k.clone(), v.access)).collect();
            entries.sort_by_key(|(_, a)| *a);
            for (k, _) in entries.into_iter().take(MAX_L1 / 4) {
                map.remove(&k);
            }
        }
        map.insert(response_id.to_string(), turn);
    });

    // L2 persist (best effort)
    if let Some(conn) = ensure_l2() {
        let data = serde_json::json!({
            "messages": messages,
            "assistant_messages": assistant_messages,
            "reasoning_content": reasoning_content,
        });
        let _ = conn.execute(
            "INSERT OR REPLACE INTO sessions (response_id, data, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![response_id, data.to_string(), chrono::Utc::now().to_rfc3339()],
        );

        // Clean old sessions (>24h)
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let _ = conn.execute("DELETE FROM sessions WHERE created_at < ?1", [&cutoff]);
    }
}

/// Look up a previous turn. L1 first, then L2 fallback.
pub fn get_history(previous_response_id: &str) -> Option<Vec<ChatMessage>> {
    // L1 lookup
    let l1_hit = with_l1(|map| {
        if let Some(turn) = map.get_mut(previous_response_id) {
            turn.access = next();
            let mut h = turn.messages.clone();
            h.extend(turn.assistant_messages.clone());
            Some(h)
        } else {
            None
        }
    });

    if l1_hit.is_some() {
        return l1_hit;
    }

    // L2 lookup
    let conn = ensure_l2()?;
    let data_str: String = conn.query_row(
        "SELECT data FROM sessions WHERE response_id = ?1",
        [previous_response_id],
        |row| row.get(0),
    ).ok()?;

    let data: serde_json::Value = serde_json::from_str(&data_str).ok()?;
    let messages: Vec<ChatMessage> = serde_json::from_value(data.get("messages")?.clone()).ok()?;
    let asst: Vec<ChatMessage> = serde_json::from_value(data.get("assistant_messages")?.clone()).ok()?;
    let rc = data.get("reasoning_content").and_then(|v| v.as_str()).map(String::from);

    // Warm L1
    with_l1(|map| {
        map.insert(previous_response_id.to_string(), StoredTurn {
            messages: messages.clone(),
            assistant_messages: asst.clone(),
            response_id: previous_response_id.to_string(),
            reasoning_content: rc,
            access: next(),
        });
    });

    let mut history = messages;
    history.extend(asst);
    Some(history)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::ChatMessage;
    use crate::test_utils::{FS_LOCK, setup_temp_home, cleanup};
    use serde_json::json;

    fn clear_l1() {
        with_l1(|map| map.clear());
    }

    fn dummy_messages() -> (Vec<ChatMessage>, Vec<ChatMessage>) {
        let msgs = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(json!("hello")),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        let asst = vec![ChatMessage {
            role: "assistant".to_string(),
            content: Some(json!("hi")),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        (msgs, asst)
    }

    #[test]
    fn test_store_and_get_history_l1() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_l1();
        let temp = setup_temp_home();
        let (msgs, asst) = dummy_messages();
        store_turn("resp_1", msgs.clone(), asst.clone(), Some("rc".to_string()));
        let history = get_history("resp_1").unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "user");
        assert_eq!(history[1].role, "assistant");
        cleanup(&temp);
    }

    #[test]
    fn test_get_history_not_found() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_l1();
        let temp = setup_temp_home();
        // Ensure L2 file does not exist so get_history truly returns None
        let l2 = l2_path();
        let _ = std::fs::remove_file(&l2);
        assert!(get_history("resp_nonexistent").is_none());
        cleanup(&temp);
    }

    #[test]
    fn test_l2_persistence() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_l1();
        let temp = setup_temp_home();
        let (msgs, asst) = dummy_messages();
        store_turn("resp_persistent", msgs.clone(), asst.clone(), Some("rc2".to_string()));

        // Clear L1 to force L2 lookup
        clear_l1();
        // L2 should return the stored turn and warm L1
        let history = get_history("resp_persistent").unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, Some(json!("hello")));
        assert_eq!(history[1].content, Some(json!("hi")));

        // Now L1 should be warmed
        let history2 = get_history("resp_persistent").unwrap();
        assert_eq!(history2.len(), 2);
        cleanup(&temp);
    }

    #[test]
    fn test_store_turn_overwrites() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_l1();
        let temp = setup_temp_home();
        let (msgs, asst) = dummy_messages();
        store_turn("resp_same", msgs.clone(), asst.clone(), Some("first".to_string()));
        store_turn("resp_same", msgs.clone(), asst.clone(), Some("second".to_string()));
        let history = get_history("resp_same").unwrap();
        assert_eq!(history[1].content, Some(json!("hi")));
        assert_eq!(history.len(), 2);
        cleanup(&temp);
    }
}
