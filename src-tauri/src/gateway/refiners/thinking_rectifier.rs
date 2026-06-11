//! Thinking Rectifier — clamp / normalise thinking & reasoning parameters
//! to the provider's accepted shape and range.
//!
//! Two parameter families are handled:
//!   - **Anthropic-style** `thinking.budget_tokens` (integer; per-model
//!     min/max). Clamped into `quirks.thinking_budget` range.
//!   - **OpenAI-style** `reasoning.effort` (string enum). Normalised against
//!     `quirks.reasoning_effort_values`; unknown values rewritten to
//!     `"medium"` when available, else the first listed value.
//!
//! Returns a vector of `ThinkingRectifierAction` entries so the refiner_log
//! can show every individual edit. An empty vec means no-op.
//!
//! Design constraints (same as body_filter):
//!   - Pure, in-place body mutation.
//!   - Top-level field lookup only.
//!   - Idempotent.

use serde_json::Value;

use crate::gateway::refiner_log::ThinkingRectifierAction;
use crate::models::provider::Provider;

pub fn apply(provider: &Provider, body: &mut Value) -> Vec<ThinkingRectifierAction> {
    let quirks = super::resolve_quirks(provider);
    let mut actions = Vec::new();

    if let Some(range) = quirks.thinking_budget {
        if let Some(obj) = body.as_object_mut() {
            if let Some(thinking) = obj.get_mut("thinking").and_then(|v| v.as_object_mut()) {
                if let Some(budget) = thinking.get("budget_tokens").and_then(|v| v.as_i64()) {
                    let clamped = budget.clamp(range.min, range.max);
                    if clamped != budget {
                        thinking.insert("budget_tokens".into(), Value::from(clamped));
                        actions.push(ThinkingRectifierAction {
                            field: "thinking.budget_tokens".into(),
                            from: Some(budget.to_string()),
                            to: Some(clamped.to_string()),
                            reason: format!("clamped to [{}, {}]", range.min, range.max),
                        });
                    }
                }
            }
        }
    }

    if !quirks.reasoning_effort_values.is_empty() {
        if let Some(obj) = body.as_object_mut() {
            if let Some(reasoning) = obj.get_mut("reasoning").and_then(|v| v.as_object_mut()) {
                if let Some(effort) = reasoning
                    .get("effort")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                {
                    if !quirks.reasoning_effort_values.contains(&effort) {
                        let target = if quirks.reasoning_effort_values.iter().any(|v| v == "medium")
                        {
                            "medium".to_string()
                        } else {
                            quirks.reasoning_effort_values[0].clone()
                        };
                        reasoning.insert("effort".into(), Value::String(target.clone()));
                        actions.push(ThinkingRectifierAction {
                            field: "reasoning.effort".into(),
                            from: Some(effort),
                            to: Some(target),
                            reason: "normalised to provider's accepted value".into(),
                        });
                    }
                }
            }
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(provider_type: &str, quirks_json: Option<&str>) -> Provider {
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
            protocol: "anthropic_messages".into(),
            timeout_seconds: 60,
            status: "ok".into(),
            supports_vision: None,
            auto_cache_control: None,
            supports_cache: None,
            model_capabilities: None,
            provider_quirks: quirks_json.map(|s| s.to_string()),
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
    fn budget_below_min_is_clamped_up() {
        // MiMo defaults: min 1024, max 32768.
        let p = provider("mimo", None);
        let mut body = json!({
            "thinking": {"type": "enabled", "budget_tokens": 100},
        });
        let actions = apply(&p, &mut body);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].field, "thinking.budget_tokens");
        assert_eq!(body["thinking"]["budget_tokens"].as_i64(), Some(1024));
    }

    #[test]
    fn budget_above_max_is_clamped_down() {
        let p = provider("mimo", None);
        let mut body = json!({
            "thinking": {"budget_tokens": 999_999},
        });
        let actions = apply(&p, &mut body);
        assert_eq!(actions.len(), 1);
        assert_eq!(body["thinking"]["budget_tokens"].as_i64(), Some(32_768));
    }

    #[test]
    fn budget_in_range_is_untouched() {
        let p = provider("mimo", None);
        let mut body = json!({"thinking": {"budget_tokens": 4096}});
        assert!(apply(&p, &mut body).is_empty());
        assert_eq!(body["thinking"]["budget_tokens"].as_i64(), Some(4096));
    }

    #[test]
    fn reasoning_effort_unknown_value_normalised_to_medium() {
        let p = provider("openai", None);
        let mut body = json!({"reasoning": {"effort": "extreme"}});
        let actions = apply(&p, &mut body);
        assert_eq!(actions.len(), 1);
        assert_eq!(body["reasoning"]["effort"].as_str(), Some("medium"));
    }

    #[test]
    fn reasoning_effort_known_value_untouched() {
        let p = provider("openai", None);
        let mut body = json!({"reasoning": {"effort": "high"}});
        assert!(apply(&p, &mut body).is_empty());
    }

    #[test]
    fn no_thinking_no_reasoning_is_noop() {
        let p = provider("mimo", None);
        let mut body = json!({"model": "mimo-v2.5-pro"});
        assert!(apply(&p, &mut body).is_empty());
    }

    #[test]
    fn provider_with_no_quirks_does_nothing() {
        // Custom provider with no defaults and no user quirks.
        let p = provider("brand-new", None);
        let mut body = json!({
            "thinking": {"budget_tokens": 100},
            "reasoning": {"effort": "extreme"},
        });
        assert!(apply(&p, &mut body).is_empty());
        assert_eq!(body["thinking"]["budget_tokens"].as_i64(), Some(100));
        assert_eq!(body["reasoning"]["effort"].as_str(), Some("extreme"));
    }

    #[test]
    fn user_thinking_budget_override_wins_over_default() {
        let p = provider(
            "mimo",
            Some(r#"{"thinking_budget":{"min":2048,"max":4096}}"#),
        );
        let mut body = json!({"thinking": {"budget_tokens": 100}});
        apply(&p, &mut body);
        assert_eq!(body["thinking"]["budget_tokens"].as_i64(), Some(2048));
    }

    #[test]
    fn idempotent_second_call_is_noop() {
        let p = provider("mimo", None);
        let mut body = json!({"thinking": {"budget_tokens": 100}});
        assert_eq!(apply(&p, &mut body).len(), 1);
        assert!(apply(&p, &mut body).is_empty());
    }

    #[test]
    fn effort_falls_back_to_first_value_when_medium_absent() {
        // provider 接受列表里没有 medium 时，未知值改写成列表第一个。
        let p = provider(
            "brand-new",
            Some(r#"{"reasoning_effort_values":["low","high"]}"#),
        );
        let mut body = json!({"reasoning": {"effort": "extreme"}});
        let actions = apply(&p, &mut body);
        assert_eq!(actions.len(), 1);
        assert_eq!(body["reasoning"]["effort"].as_str(), Some("low"));
    }

    #[test]
    fn budget_and_effort_rectified_in_one_pass() {
        // thinking 与 reasoning 同时越界：一次 apply 产出两条 action。
        let p = provider(
            "brand-new",
            Some(
                r#"{"thinking_budget":{"min":1024,"max":2048},"reasoning_effort_values":["low","medium","high"]}"#,
            ),
        );
        let mut body = json!({
            "thinking": {"budget_tokens": 10},
            "reasoning": {"effort": "max"},
        });
        let actions = apply(&p, &mut body);
        assert_eq!(actions.len(), 2);
        assert_eq!(body["thinking"]["budget_tokens"].as_i64(), Some(1024));
        assert_eq!(body["reasoning"]["effort"].as_str(), Some("medium"));
    }

    #[test]
    fn non_object_thinking_field_is_noop() {
        // thinking 不是对象（如 true）时拿不到 budget_tokens，原样透传。
        let p = provider("mimo", None);
        let mut body = json!({"thinking": true});
        assert!(apply(&p, &mut body).is_empty());
        assert_eq!(body["thinking"], json!(true));
    }

    #[test]
    fn non_object_body_is_noop() {
        let p = provider("mimo", None);
        let mut body = json!("not an object");
        assert!(apply(&p, &mut body).is_empty());
    }
}
