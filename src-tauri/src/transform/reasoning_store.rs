//! Server-side store for DeepSeek reasoning_content.
//!
//! Codex doesn't pass reasoning_content back in subsequent requests.
//! We store it keyed by content hash and tool_call_id, then re-inject
//! it when converting the next request.
//!
//! Uses LRU-like eviction: each entry has an access counter, oldest entries
//! are evicted first when the store exceeds MAX_ENTRIES.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// (reasoning_content, last_access_counter)
static STORE: Mutex<Option<HashMap<String, (String, u64)>>> = Mutex::new(None);
static ACCESS_COUNTER: AtomicU64 = AtomicU64::new(0);

const MAX_ENTRIES: usize = 500;

fn with_store<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, (String, u64)>) -> R,
{
    let mut guard = STORE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    f(guard.as_mut().unwrap())
}

fn next_counter() -> u64 {
    ACCESS_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn content_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Use two independent hash seeds to produce 128 bits, reducing collision probability
    let mut h1 = DefaultHasher::new();
    text.hash(&mut h1);
    let hash1 = h1.finish();
    let mut h2 = DefaultHasher::new();
    hash1.hash(&mut h2);
    text.hash(&mut h2);
    let hash2 = h2.finish();
    // Include text length as additional discriminator
    format!("{:016x}{:016x}_{}", hash1, hash2, text.len())
}

/// Store reasoning_content keyed by assistant text hash and optionally by tool_call_ids.
pub fn store(text: &str, reasoning: &str, tool_call_ids: &[String]) {
    if reasoning.is_empty() {
        return;
    }
    let counter = next_counter();
    with_store(|map| {
        // LRU eviction: remove entries with lowest access counter
        if map.len() > MAX_ENTRIES {
            let mut entries: Vec<(String, u64)> = map.iter().map(|(k, (_, c))| (k.clone(), *c)).collect();
            entries.sort_by_key(|(_, c)| *c);
            let to_remove = entries.len() / 4;
            for (k, _) in entries.into_iter().take(to_remove) {
                map.remove(&k);
            }
        }

        let h = content_hash(text);
        map.insert(h, (reasoning.to_string(), counter));

        for tc_id in tool_call_ids {
            map.insert(format!("tc_{tc_id}"), (reasoning.to_string(), counter));
        }
    });
}

/// Look up stored reasoning_content by assistant text.
pub fn lookup_by_content(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let h = content_hash(text);
    let counter = next_counter();
    with_store(|map| {
        map.get_mut(&h).map(|(rc, c)| {
            *c = counter; // Update access time
            rc.clone()
        })
    })
}

/// Look up stored reasoning_content by tool_call_id.
pub fn lookup_by_tool_call_id(tc_id: &str) -> Option<String> {
    let counter = next_counter();
    with_store(|map| {
        map.get_mut(&format!("tc_{tc_id}")).map(|(rc, c)| {
            *c = counter;
            rc.clone()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::FS_LOCK;
    use std::sync::Mutex;

    // Global lock to prevent concurrent access to the static store during tests
    #[allow(dead_code)]
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn clear_store() {
        with_store(|map| map.clear());
    }

    #[test]
    fn test_store_and_lookup_by_content() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        store("hello", "thinking...", &[]);
        assert_eq!(lookup_by_content("hello"), Some("thinking...".to_string()));
    }

    #[test]
    fn test_store_and_lookup_by_tool_call_id() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        store("hello", "thinking...", &["tc1".to_string()]);
        assert_eq!(lookup_by_tool_call_id("tc1"), Some("thinking...".to_string()));
    }

    #[test]
    fn test_lookup_empty_text() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        assert_eq!(lookup_by_content(""), None);
    }

    #[test]
    fn test_store_empty_reasoning_skipped() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        store("hello", "", &[]);
        assert_eq!(lookup_by_content("hello"), None);
    }

    #[test]
    fn test_recent_entry_survives_eviction() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        // Fill store to trigger eviction
        for i in 0..MAX_ENTRIES + 20 {
            store(&format!("filler_{i}"), &format!("rc_{i}"), &[]);
        }
        // The most recent entry should still exist
        assert!(lookup_by_content(&format!("filler_{}", MAX_ENTRIES + 19)).is_some());
    }

    #[test]
    fn test_lru_eviction() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        // Fill store well beyond capacity
        for i in 0..MAX_ENTRIES + 100 {
            store(&format!("key_{i:04}"), &format!("rc_{i}"), &[]);
        }
        // Some oldest entries should have been evicted
        let mut found_old = false;
        let mut found_new = false;
        for i in 0..50 {
            if lookup_by_content(&format!("key_{i:04}")).is_none() {
                found_old = true;
            }
        }
        for i in MAX_ENTRIES..MAX_ENTRIES + 50 {
            if lookup_by_content(&format!("key_{i:04}")).is_some() {
                found_new = true;
            }
        }
        assert!(found_old, "Expected some old entries to be evicted");
        assert!(found_new, "Expected newer entries to still exist");
    }

    #[test]
    fn test_overwrite_existing_key() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        store("same", "first", &[]);
        store("same", "second", &[]);
        assert_eq!(lookup_by_content("same"), Some("second".to_string()));
    }

    #[test]
    fn test_multiple_tool_call_ids() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        store("text", "rc", &["tc1".to_string(), "tc2".to_string()]);
        assert_eq!(lookup_by_tool_call_id("tc1"), Some("rc".to_string()));
        assert_eq!(lookup_by_tool_call_id("tc2"), Some("rc".to_string()));
        assert_eq!(lookup_by_content("text"), Some("rc".to_string()));
    }

    #[test]
    fn test_content_hash_includes_length() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Two strings that differ only in length should have different hashes
        let h1 = content_hash("hello");
        let h2 = content_hash("hello!");
        assert_ne!(h1, h2);
        // Hash includes length suffix
        assert!(h1.ends_with("_5"));
        assert!(h2.ends_with("_6"));
    }

    #[test]
    fn test_different_texts_produce_different_keys() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        store("text_a", "reasoning_a", &[]);
        store("text_b", "reasoning_b", &[]);
        assert_eq!(lookup_by_content("text_a"), Some("reasoning_a".to_string()));
        assert_eq!(lookup_by_content("text_b"), Some("reasoning_b".to_string()));
    }

    #[test]
    fn test_chinese_content_hash_no_panic() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_store();
        let chinese = "你好世界，这是一段中文测试内容";
        store(chinese, "中文推理", &[]);
        assert_eq!(lookup_by_content(chinese), Some("中文推理".to_string()));
    }
}
