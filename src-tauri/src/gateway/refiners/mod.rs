//! Refiner pipeline — provider-specific request/response touch-ups gated
//! behind opt-in switches.
//!
//! Three refiners share one design contract:
//!   - **Body Filter** strips request fields the provider rejects.
//!   - **Thinking Rectifier** rewrites thinking / reasoning parameters to
//!     match the provider's accepted shape and range.
//!   - **Error Mapper** rewrites upstream error responses into the shape the
//!     client (Codex / Claude Code / Gemini CLI) expects.
//!
//! Activation: each refiner is off by default. The user opts in via
//! `gateway_settings.{body_filter,thinking_rectifier,error_mapper}_global`
//! (master switch); a per-provider override on `providers.*_enabled` can
//! force off (Some(0)) but cannot force on when the global is off — global
//! is the master kill. See `Provider::refiner_effective`.
//!
//! Source of truth for each provider's quirks is layered:
//!   1. `providers.provider_quirks` JSON (user override)
//!   2. `providers::capabilities::default_quirks_for_provider` (built-in defaults)
//!   3. No-op (zero-impact pass-through).
//!
//! Every refiner returns a structured action log (`RefinerLog` variants)
//! that the gateway appends to `request_logs.trace_json` for diagnosis.

pub mod body_filter;
pub mod error_mapper;
pub mod runtime;
pub mod thinking_rectifier;

use crate::models::gateway::GatewaySettings;
use crate::models::provider::{Provider, ProviderQuirks};
use crate::providers::capabilities::default_quirks_for_provider;

/// Whether each refiner should run for this request. Computed once and
/// passed through so all three refiners observe a consistent view.
#[derive(Debug, Clone, Copy)]
pub struct EffectiveSwitches {
    pub body_filter: bool,
    pub thinking_rectifier: bool,
    pub error_mapper: bool,
}

impl EffectiveSwitches {
    pub fn for_request(provider: &Provider, settings: &GatewaySettings) -> Self {
        Self {
            body_filter: Provider::refiner_effective(
                provider.body_filter_enabled,
                settings.body_filter_global,
            ),
            thinking_rectifier: Provider::refiner_effective(
                provider.thinking_rectifier_enabled,
                settings.thinking_rectifier_global,
            ),
            error_mapper: Provider::refiner_effective(
                provider.error_mapper_enabled,
                settings.error_mapper_global,
            ),
        }
    }

    pub fn all_off(&self) -> bool {
        !self.body_filter && !self.thinking_rectifier && !self.error_mapper
    }
}

/// Resolve effective quirks: user-configured JSON wins, then provider-type
/// defaults. Vectors and maps are merged additively (user list extends
/// default) so opting into a quirk-override doesn't silently drop the
/// shipped defaults — the user only "loses" defaults when they explicitly
/// supply an empty value (e.g. `"unsupported_fields": []`).
///
/// The merge favours "additive" because the alternative is silent surprise:
/// a user who adds one custom unsupported field would, under a "replace"
/// model, accidentally undo every default we ship. That's never what they
/// mean. If they truly want to drop a default, they can override the
/// individual field via this layer's own override mechanism (future work).
pub fn resolve_quirks(provider: &Provider) -> ProviderQuirks {
    let mut quirks = default_quirks_for_provider(&provider.provider_type);
    let user = provider.parse_quirks();

    // unsupported_fields: union, preserving order (default first, then user-added)
    for f in user.unsupported_fields {
        if !quirks.unsupported_fields.contains(&f) {
            quirks.unsupported_fields.push(f);
        }
    }
    // thinking_budget: user wins when supplied
    if user.thinking_budget.is_some() {
        quirks.thinking_budget = user.thinking_budget;
    }
    // reasoning_effort_values: user wins when supplied (it's a closed enum,
    // not extensible — Anthropic and OpenAI have different vocabularies)
    if !user.reasoning_effort_values.is_empty() {
        quirks.reasoning_effort_values = user.reasoning_effort_values;
    }
    // error_code_overrides: union (user can extend the table)
    for (k, v) in user.error_code_overrides {
        quirks.error_code_overrides.entry(k).or_insert(v);
    }
    quirks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::ProviderQuirks;

    fn make_provider(provider_type: &str, quirks_json: Option<&str>) -> Provider {
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

    fn make_settings(bf: bool, tr: bool, em: bool) -> GatewaySettings {
        GatewaySettings {
            id: 1,
            host: "127.0.0.1".into(),
            port: 9090,
            active_provider_id: None,
            input_protocol: "openai_responses".into(),
            output_protocol: "openai_chat_completions".into(),
            auto_start: false,
            log_retention_days: 14,
            body_filter_global: bf,
            thinking_rectifier_global: tr,
            error_mapper_global: em,
            health_probe_enabled: false,
            codex_compact_enabled: true,
            codex_compact_summary_max_tokens: 1500,
            updated_at: "now".into(),
        }
    }

    #[test]
    fn switches_all_off_when_global_off() {
        let p = make_provider("deepseek", None);
        let s = make_settings(false, false, false);
        let sw = EffectiveSwitches::for_request(&p, &s);
        assert!(sw.all_off());
    }

    #[test]
    fn switches_on_when_global_on_and_per_provider_unset() {
        let p = make_provider("deepseek", None);
        let s = make_settings(true, true, true);
        let sw = EffectiveSwitches::for_request(&p, &s);
        assert!(sw.body_filter);
        assert!(sw.thinking_rectifier);
        assert!(sw.error_mapper);
    }

    #[test]
    fn per_provider_opt_out_wins_over_global_on() {
        let mut p = make_provider("deepseek", None);
        p.body_filter_enabled = Some(0); // explicit off
        let s = make_settings(true, true, true);
        let sw = EffectiveSwitches::for_request(&p, &s);
        assert!(
            !sw.body_filter,
            "per-provider Some(0) must override global on"
        );
        assert!(sw.thinking_rectifier);
        assert!(sw.error_mapper);
    }

    #[test]
    fn per_provider_opt_in_does_not_override_global_off() {
        let mut p = make_provider("deepseek", None);
        p.body_filter_enabled = Some(1); // user wanted on, but global is off
        let s = make_settings(false, false, false);
        let sw = EffectiveSwitches::for_request(&p, &s);
        assert!(!sw.body_filter, "global off is the master kill");
    }

    #[test]
    fn resolve_quirks_merges_user_fields_onto_defaults() {
        let p = make_provider("deepseek", Some(r#"{"unsupported_fields":["my_extra"]}"#));
        let q = resolve_quirks(&p);
        // Default for DeepSeek includes web_search
        assert!(q.unsupported_fields.contains(&"web_search".to_string()));
        // User-added field appended
        assert!(q.unsupported_fields.contains(&"my_extra".to_string()));
    }

    #[test]
    fn resolve_quirks_user_thinking_budget_overrides_default() {
        let p = make_provider(
            "mimo",
            Some(r#"{"thinking_budget":{"min":2048,"max":16000}}"#),
        );
        let q = resolve_quirks(&p);
        let r = q.thinking_budget.unwrap();
        assert_eq!(r.min, 2048);
        assert_eq!(r.max, 16000);
    }

    #[test]
    fn resolve_quirks_unknown_provider_returns_default_empty() {
        let p = make_provider("brand-new-thing", None);
        let q = resolve_quirks(&p);
        assert!(q.unsupported_fields.is_empty());
        assert!(q.thinking_budget.is_none());
    }

    #[test]
    fn provider_quirks_default_struct_is_no_op() {
        let q = ProviderQuirks::default();
        assert!(q.unsupported_fields.is_empty());
        assert!(q.thinking_budget.is_none());
        assert!(q.reasoning_effort_values.is_empty());
        assert!(q.error_code_overrides.is_empty());
    }
}
