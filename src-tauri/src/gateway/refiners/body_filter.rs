//! Body Filter — strip top-level request fields a provider is known to reject.
//!
//! Operates on a parsed JSON request body in-place. Only consults the
//! resolved `ProviderQuirks::unsupported_fields` list; anything not on the
//! list is left untouched. Returns a `BodyFilterAction` describing what
//! was actually stripped (the list is potentially smaller than the
//! capability list because the request might not contain every banned
//! field).
//!
//! Design constraints:
//!   - **Pure** — no DB access, no network. Caller decides whether to invoke.
//!   - **Top-level only** — we don't dig into messages/tools; the
//!     scenarios we've actually seen 400 on are top-level shape
//!     mismatches (e.g. `web_search` on DeepSeek's OpenAI endpoint).
//!     Walking nested structures invites silent surprises.
//!   - **Idempotent** — calling twice on the same body is a no-op.

use serde_json::Value;

use crate::gateway::refiner_log::BodyFilterAction;
use crate::models::provider::Provider;

/// Strip every banned top-level field that's present. Returns `None` when no
/// field was actually removed (so the caller can skip log-writing).
pub fn apply(provider: &Provider, body: &mut Value) -> Option<BodyFilterAction> {
    let quirks = super::resolve_quirks(provider);
    if quirks.unsupported_fields.is_empty() {
        return None;
    }
    let obj = body.as_object_mut()?;
    let mut stripped = Vec::new();
    for f in &quirks.unsupported_fields {
        if obj.remove(f).is_some() {
            stripped.push(f.clone());
        }
    }
    if stripped.is_empty() {
        None
    } else {
        Some(BodyFilterAction {
            stripped_fields: stripped,
            reason: "provider_quirks.unsupported_fields".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider_with_quirks(provider_type: &str, json: &str) -> Provider {
        Provider {
            id: "p".into(),
            name: "P".into(),
            provider_type: provider_type.into(),
            base_url: "https://x".into(),
            api_key: Some("sk".into()),
            default_model: "m".into(),
            reasoning_model: None,
            supported_models: None,
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            responses_base_url: None,
            protocol: "openai_chat_completions".into(),
            timeout_seconds: 60,
            status: "ok".into(),
            supports_vision: None,
            auto_cache_control: None,
            supports_cache: None,
            model_capabilities: None,
            provider_quirks: Some(json.to_string()),
            body_filter_enabled: None,
            thinking_rectifier_enabled: None,
            error_mapper_enabled: None,
            model_degradation_chain: None,
            model_context_windows: None,
            enabled: true,
            is_active: true,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    #[test]
    fn deepseek_default_strips_web_search() {
        // No user-quirks JSON, default DeepSeek quirks ship `web_search` in
        // unsupported_fields.
        let p = provider_with_quirks("deepseek", r#"{}"#);
        let mut body = json!({
            "model": "deepseek-v4-pro",
            "messages": [],
            "web_search": true,
        });
        let action = apply(&p, &mut body).expect("should strip");
        assert_eq!(action.stripped_fields, vec!["web_search".to_string()]);
        assert!(body.get("web_search").is_none());
        assert!(body.get("model").is_some(), "non-banned fields preserved");
    }

    #[test]
    fn provider_with_no_default_quirks_is_noop() {
        let p = provider_with_quirks("brand-new", r#"{}"#);
        let mut body = json!({"web_search": true});
        assert!(apply(&p, &mut body).is_none());
        assert!(body.get("web_search").is_some());
    }

    #[test]
    fn missing_field_returns_none() {
        // DeepSeek bans web_search, but the request didn't include it.
        let p = provider_with_quirks("deepseek", r#"{}"#);
        let mut body = json!({"model": "deepseek-v4-pro"});
        assert!(apply(&p, &mut body).is_none());
    }

    #[test]
    fn user_added_field_strips_alongside_defaults() {
        let p = provider_with_quirks("deepseek", r#"{"unsupported_fields":["my_extra"]}"#);
        let mut body = json!({"web_search": true, "my_extra": "x"});
        let action = apply(&p, &mut body).unwrap();
        assert!(action.stripped_fields.contains(&"web_search".to_string()));
        assert!(action.stripped_fields.contains(&"my_extra".to_string()));
    }

    #[test]
    fn idempotent_second_call_returns_none() {
        let p = provider_with_quirks("deepseek", r#"{}"#);
        let mut body = json!({"web_search": true});
        assert!(apply(&p, &mut body).is_some());
        assert!(apply(&p, &mut body).is_none());
    }

    #[test]
    fn non_object_body_is_noop() {
        let p = provider_with_quirks("deepseek", r#"{}"#);
        let mut body = json!([1, 2, 3]);
        assert!(apply(&p, &mut body).is_none());
    }
}
