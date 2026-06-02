//! Soft session affinity to preserve upstream prompt-cache hit rate.
//!
//! When the upstream reports `cached_tokens > 0` for a request, the gateway
//! records a `(session_id → provider_id)` binding with a 1 h TTL. On the next
//! turn of the same conversation, the failover candidate list is reordered to
//! put the affinity provider first — as long as it's still in the candidate
//! set and not in cooldown. If the affinity provider is unavailable we fall
//! back to the natural failover order; affinity is a hint, never a hard pin.
//!
//! Session-ID derivation is request-agnostic: we hash the FIRST user message
//! plus the sorted tool-name signature. Codex / Claude Code / OpenCode all
//! replay the original prompt at the head of every turn, so this fingerprint
//! stays stable across the whole conversation without requiring the client
//! to send a `previous_response_id` or `prompt_cache_key`.
//!
//! Borrowed from codex-switcher's session-affinity design.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::protocol::openai_responses::ResponsesRequest;

const TTL_MS: u64 = 60 * 60 * 1000; // 1 hour
const MAX_ENTRIES: usize = 512;
const MIN_PROMPT_LEN: usize = 64;

#[derive(Debug, Clone)]
pub struct AffinityEntry {
    pub provider_id: String,
    pub created_at_ms: u64,
    pub last_hit_at_ms: u64,
    pub hit_count: u64,
}

static STORE: OnceLock<Mutex<HashMap<String, AffinityEntry>>> = OnceLock::new();

fn store() -> &'static Mutex<HashMap<String, AffinityEntry>> {
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Record a `(session_id → provider_id)` binding. Bumps `last_hit_at_ms`
/// and `hit_count` for an existing entry; resets created_at_ms when the
/// provider changes (treat as a new binding).
pub fn record(session_id: &str, provider_id: &str) {
    let now = now_ms();
    let mut g = store().lock().unwrap_or_else(|e| e.into_inner());

    g.entry(session_id.to_string())
        .and_modify(|e| {
            if e.provider_id != provider_id {
                e.provider_id = provider_id.to_string();
                e.created_at_ms = now;
                e.hit_count = 0;
            }
            e.last_hit_at_ms = now;
            e.hit_count += 1;
        })
        .or_insert_with(|| AffinityEntry {
            provider_id: provider_id.to_string(),
            created_at_ms: now,
            last_hit_at_ms: now,
            hit_count: 1,
        });

    if g.len() > MAX_ENTRIES {
        evict_oldest(&mut g);
    }
}

/// Record only when the upstream usage indicates a real cache hit. Returns
/// true when an affinity entry was written. Caller can ignore the return —
/// it's surfaced for tests and telemetry.
pub fn record_if_cache_hit(session_id: &str, provider_id: &str, usage: &Value) -> bool {
    if extract_cached_tokens(usage) > 0 {
        record(session_id, provider_id);
        true
    } else {
        false
    }
}

/// Look up the affinity binding for a session. Returns None when the entry
/// doesn't exist OR has expired (expired entries are purged on lookup).
pub fn lookup(session_id: &str) -> Option<AffinityEntry> {
    let now = now_ms();
    let mut g = store().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = g.get(session_id).cloned() {
        if now.saturating_sub(entry.last_hit_at_ms) <= TTL_MS {
            return Some(entry);
        }
        g.remove(session_id);
    }
    None
}

/// Clear all entries. Test 用，也用于 CLI SIGHUP 热重载 —— 让用户改完
/// provider 配置后能立刻摆脱之前的 cache-hit affinity 偏好（不需要等 TTL）。
pub fn clear() {
    let mut g = store().lock().unwrap_or_else(|e| e.into_inner());
    g.clear();
}

fn evict_oldest(g: &mut HashMap<String, AffinityEntry>) {
    let oldest = g
        .iter()
        .min_by_key(|(_, e)| e.last_hit_at_ms)
        .map(|(k, _)| k.clone());
    if let Some(k) = oldest {
        g.remove(&k);
    }
}

/// Extract the cached-token count from a normalized usage object. Recognizes:
/// - OpenAI Responses API:    `input_tokens_details.cached_tokens`
/// - OpenAI Chat Completions: `prompt_tokens_details.cached_tokens`
/// - Anthropic Messages API:  `cache_read_input_tokens`
/// - Bare field (some Chinese providers): `cached_tokens`
fn extract_cached_tokens(usage: &Value) -> i64 {
    if let Some(v) = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(|v| v.as_i64())
    {
        return v;
    }
    if let Some(v) = usage
        .pointer("/input_tokens_details/cached_tokens")
        .and_then(|v| v.as_i64())
    {
        return v;
    }
    if let Some(v) = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_i64())
    {
        return v;
    }
    if let Some(v) = usage.get("cached_tokens").and_then(|v| v.as_i64()) {
        return v;
    }
    0
}

/// Derive a stable session-affinity key from a Responses API request. Uses
/// `first_user_message + sorted_tool_names` as the fingerprint so the key
/// stays the same across turns of the same conversation (Codex replays the
/// original prompt every turn). Returns None for prompts too short to be a
/// real conversation — pinning a one-word query gives no caching benefit.
pub fn derive_from_responses(req: &ResponsesRequest) -> Option<String> {
    let first_user = extract_first_user_text(&req.input)?;
    if first_user.len() < MIN_PROMPT_LEN {
        return None;
    }
    let tools_sig = req
        .tools
        .as_ref()
        .map(|t| tools_signature(t))
        .unwrap_or_default();
    Some(make_session_id(&first_user, &tools_sig))
}

/// Same idea for an Anthropic Messages request body (raw JSON, since our
/// Anthropic path doesn't have a typed struct yet). Looks at the first user
/// message and the tools array.
pub fn derive_from_anthropic_body(body: &Value) -> Option<String> {
    let messages = body.get("messages")?.as_array()?;
    let first_user = messages.iter().find_map(|m| {
        if m.get("role").and_then(|r| r.as_str()) != Some("user") {
            return None;
        }
        extract_text_from_anthropic_content(m.get("content")?)
    })?;
    if first_user.len() < MIN_PROMPT_LEN {
        return None;
    }
    let tools_sig = body
        .get("tools")
        .and_then(|t| t.as_array())
        .map(|t| tools_signature(t))
        .unwrap_or_default();
    Some(make_session_id(&first_user, &tools_sig))
}

fn make_session_id(first_user_text: &str, tools_sig: &str) -> String {
    let mut h = DefaultHasher::new();
    first_user_text.hash(&mut h);
    tools_sig.hash(&mut h);
    format!("sa_{:016x}", h.finish())
}

fn extract_first_user_text(input: &Value) -> Option<String> {
    let arr = input.as_array()?;
    for item in arr {
        let t = item
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("message");
        let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if t != "message" || role != "user" {
            continue;
        }
        let c = item.get("content")?;
        if let Some(s) = c.as_str() {
            return Some(s.to_string());
        }
        if let Some(parts) = c.as_array() {
            for p in parts {
                if let Some(s) = p.get("text").and_then(|x| x.as_str()) {
                    return Some(s.to_string());
                }
                if let Some(s) = p.get("input_text").and_then(|x| x.as_str()) {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

fn extract_text_from_anthropic_content(c: &Value) -> Option<String> {
    if let Some(s) = c.as_str() {
        return Some(s.to_string());
    }
    if let Some(parts) = c.as_array() {
        for p in parts {
            if p.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(s) = p.get("text").and_then(|x| x.as_str()) {
                    return Some(s.to_string());
                }
            }
        }
    }
    None
}

fn tools_signature(tools: &[Value]) -> String {
    let mut names: Vec<String> = tools
        .iter()
        .filter_map(|t| {
            t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .map(str::to_string)
                .or_else(|| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .or_else(|| t.get("type").and_then(|x| x.as_str()).map(str::to_string))
        })
        .collect();
    names.sort();
    names.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // STORE 是 process-local 全局状态（OnceLock<Mutex<HashMap>>），cargo test
    // 默认并行跑 `tests` 模块里的多个 #[test]，导致 A 刚 record 完 B 的 clear()
    // 把 A 写入的条目清掉，A 的 lookup 就拿到 None。给所有触碰 STORE 的测试
    // 串行化解决——这是 lib.rs::test_utils::FS_LOCK 同款模式。derive_* 一类
    // 纯函数测试不动 STORE，跑并行无所谓。
    static SESSION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn req_with_input(input: Value) -> ResponsesRequest {
        ResponsesRequest {
            model: Some("gpt-5".into()),
            input,
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            parallel_tool_calls: None,
            reasoning: None,
            text: None,
            metadata: None,
            seed: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            extra: Default::default(),
        }
    }

    fn long_text(prefix: &str) -> String {
        format!("{prefix} {}", "x".repeat(80))
    }

    #[test]
    fn record_and_lookup_returns_provider() {
        let _g = SESSION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        record("sa_abc", "prov_mimo");
        let entry = lookup("sa_abc").expect("entry should exist");
        assert_eq!(entry.provider_id, "prov_mimo");
        assert_eq!(entry.hit_count, 1);
    }

    #[test]
    fn record_twice_increments_hit_count() {
        let _g = SESSION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        record("sa_xyz", "prov_a");
        record("sa_xyz", "prov_a");
        let entry = lookup("sa_xyz").unwrap();
        assert_eq!(entry.hit_count, 2);
    }

    #[test]
    fn record_with_different_provider_resets_binding() {
        let _g = SESSION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        record("sa_q", "prov_a");
        record("sa_q", "prov_a");
        record("sa_q", "prov_b");
        let entry = lookup("sa_q").unwrap();
        assert_eq!(entry.provider_id, "prov_b", "switched provider");
        assert_eq!(entry.hit_count, 1, "hit_count resets on provider change");
    }

    #[test]
    fn lookup_returns_none_for_unknown_session() {
        let _g = SESSION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        assert!(lookup("sa_nope").is_none());
    }

    #[test]
    fn record_if_cache_hit_no_cached_tokens_is_noop() {
        let _g = SESSION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        let usage = json!({"input_tokens": 100, "output_tokens": 50});
        let recorded = record_if_cache_hit("sa_n", "prov_a", &usage);
        assert!(!recorded);
        assert!(lookup("sa_n").is_none());
    }

    #[test]
    fn record_if_cache_hit_writes_on_responses_format() {
        let _g = SESSION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        let usage = json!({
            "input_tokens": 200,
            "input_tokens_details": {"cached_tokens": 150},
            "output_tokens": 80,
        });
        assert!(record_if_cache_hit("sa_r", "prov_r", &usage));
        assert_eq!(lookup("sa_r").unwrap().provider_id, "prov_r");
    }

    #[test]
    fn record_if_cache_hit_writes_on_chat_completions_format() {
        let _g = SESSION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        let usage = json!({
            "prompt_tokens": 200,
            "prompt_tokens_details": {"cached_tokens": 150},
            "completion_tokens": 80,
        });
        assert!(record_if_cache_hit("sa_c", "prov_c", &usage));
    }

    #[test]
    fn record_if_cache_hit_writes_on_anthropic_format() {
        let _g = SESSION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        clear();
        let usage = json!({
            "input_tokens": 200,
            "cache_read_input_tokens": 175,
            "output_tokens": 80,
        });
        assert!(record_if_cache_hit("sa_a", "prov_a", &usage));
    }

    #[test]
    fn derive_skips_short_prompts() {
        clear();
        let req = req_with_input(json!([
            {"type": "message", "role": "user", "content": "hi"}
        ]));
        assert!(derive_from_responses(&req).is_none());
    }

    #[test]
    fn derive_stable_across_history_growth() {
        let p1 = long_text("You are a coding agent. Help me build something.");
        // Turn 1 — just the original prompt.
        let req1 = req_with_input(json!([
            {"type": "message", "role": "user", "content": p1.clone()}
        ]));
        // Turn 2 — original prompt + assistant reply + new user message.
        let req2 = req_with_input(json!([
            {"type": "message", "role": "user", "content": p1.clone()},
            {"type": "message", "role": "assistant", "content": "I'll help."},
            {"type": "message", "role": "user", "content": "Now do this part."}
        ]));
        let id1 = derive_from_responses(&req1).expect("first turn");
        let id2 = derive_from_responses(&req2).expect("second turn");
        assert_eq!(id1, id2, "session id should be stable across turns");
    }

    #[test]
    fn derive_picks_up_array_content_text() {
        let req = req_with_input(json!([
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": long_text("array content")},
            ]}
        ]));
        assert!(derive_from_responses(&req).is_some());
    }

    #[test]
    fn derive_differentiates_by_tools_signature() {
        let p = long_text("Help me with code");
        let mut req1 = req_with_input(json!([{"type": "message", "role": "user", "content": p}]));
        req1.tools = Some(vec![json!({"function": {"name": "search"}})]);

        let p = long_text("Help me with code");
        let mut req2 = req_with_input(json!([{"type": "message", "role": "user", "content": p}]));
        req2.tools = Some(vec![json!({"function": {"name": "execute"}})]);

        let id1 = derive_from_responses(&req1).unwrap();
        let id2 = derive_from_responses(&req2).unwrap();
        assert_ne!(
            id1, id2,
            "different tools must produce different session ids"
        );
    }

    #[test]
    fn derive_tools_signature_order_independent() {
        let p = long_text("Help me with code");
        let mut req1 = req_with_input(json!([{"type": "message", "role": "user", "content": p}]));
        req1.tools = Some(vec![
            json!({"function": {"name": "a"}}),
            json!({"function": {"name": "b"}}),
        ]);
        let p = long_text("Help me with code");
        let mut req2 = req_with_input(json!([{"type": "message", "role": "user", "content": p}]));
        req2.tools = Some(vec![
            json!({"function": {"name": "b"}}),
            json!({"function": {"name": "a"}}),
        ]);
        let id1 = derive_from_responses(&req1).unwrap();
        let id2 = derive_from_responses(&req2).unwrap();
        assert_eq!(id1, id2, "tools order should not affect session id");
    }

    #[test]
    fn derive_from_anthropic_body_finds_first_user_text() {
        let body = json!({
            "model": "claude-3",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": long_text("anthropic-style content")}]},
                {"role": "assistant", "content": "hi"},
            ]
        });
        assert!(derive_from_anthropic_body(&body).is_some());
    }

    #[test]
    fn extract_cached_tokens_handles_all_formats() {
        assert_eq!(extract_cached_tokens(&json!({})), 0);
        assert_eq!(
            extract_cached_tokens(&json!({"prompt_tokens_details": {"cached_tokens": 42}})),
            42
        );
        assert_eq!(
            extract_cached_tokens(&json!({"input_tokens_details": {"cached_tokens": 17}})),
            17
        );
        assert_eq!(
            extract_cached_tokens(&json!({"cache_read_input_tokens": 9})),
            9
        );
        assert_eq!(extract_cached_tokens(&json!({"cached_tokens": 3})), 3);
    }
}
