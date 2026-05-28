//! Per-provider model name rewrites for the Anthropic Messages passthrough path.
//!
//! Some providers expose a CC-only ("Claude Code") syntax on their Anthropic-
//! compat endpoint that doesn't exist on their OpenAI endpoint. The canonical
//! example is the `[1m]` suffix used by DeepSeek to opt the request into a
//! 1M-token context window:
//!
//!   * DeepSeek: "deepseek-v4-pro[1m]"  (only on /anthropic/v1/messages)
//!
//! DeepSeek's OpenAI endpoint rejects the same suffix as an unknown model.
//! Because Codex uses the OpenAI path and Claude Code uses the Anthropic path,
//! we keep the user-facing model name suffix-free and inject the suffix only
//! when the request is heading to the Anthropic passthrough handler.
//!
//! MiMo's Claude Code documentation also supports `[1m]` on eligible models.
//! AgentGate does not force that suffix for MiMo because users may prefer the
//! base context window, and some keys or plans can reject the suffixed model.
//! If the user explicitly configures `mimo-v2.5-pro[1m]` through the provider
//! model or model mapping, the value passes through unchanged.

/// Models that accept `[1m]` on DeepSeek's Anthropic endpoint. Per DeepSeek's
/// Claude Code doc only V4 Pro supports 1M; V4 Flash is the recommended
/// HAIKU / SUBAGENT model and stays without the suffix.
const DEEPSEEK_1M_CAPABLE: &[&str] = &["deepseek-v4-pro"];

/// Rewrite the outgoing model field for the Anthropic passthrough path. If
/// the provider has no CC-only rewrite rule, returns the model unchanged.
pub fn for_anthropic(provider_type: &str, model: &str) -> String {
    if is_mimo(provider_type) {
        return model.to_string();
    }
    if is_deepseek(provider_type) {
        return with_1m_suffix(model, DEEPSEEK_1M_CAPABLE);
    }
    model.to_string()
}

fn is_mimo(provider_type: &str) -> bool {
    provider_type == "mimo" || provider_type == "xiaomi" || provider_type.contains("mimo")
}

fn is_deepseek(provider_type: &str) -> bool {
    provider_type == "deepseek"
}

/// Append `[1m]` if the base model id is in the capability list and the model
/// has no explicit `[...]` qualifier yet. Models outside the list pass through
/// unchanged so that smaller models (Flash / Omni) don't get the suffix forced
/// on them (which would 400 upstream).
fn with_1m_suffix(model: &str, capable: &[&str]) -> String {
    let (base, qualifier) = split_qualifier(model);
    if qualifier.is_some() {
        // User already specified an explicit qualifier — respect it.
        return model.to_string();
    }
    if capable.contains(&base) {
        return format!("{base}[1m]");
    }
    model.to_string()
}

/// Split "model[qualifier]" into ("model", Some("qualifier")), or
/// ("model", None) when no bracket qualifier is present.
fn split_qualifier(model: &str) -> (&str, Option<&str>) {
    if let Some(stripped) = model.strip_suffix(']') {
        if let Some(open) = stripped.rfind('[') {
            return (&stripped[..open], Some(&stripped[open + 1..]));
        }
    }
    (model, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── MiMo ──

    #[test]
    fn mimo_v25_pro_passes_through() {
        assert_eq!(for_anthropic("mimo", "mimo-v2.5-pro"), "mimo-v2.5-pro");
    }

    #[test]
    fn mimo_v2_pro_passes_through() {
        assert_eq!(for_anthropic("mimo", "mimo-v2-pro"), "mimo-v2-pro");
    }

    #[test]
    fn mimo_v25_passes_through() {
        assert_eq!(for_anthropic("mimo", "mimo-v2.5"), "mimo-v2.5");
    }

    #[test]
    fn mimo_flash_does_not_get_suffix() {
        assert_eq!(for_anthropic("mimo", "mimo-v2-flash"), "mimo-v2-flash");
    }

    #[test]
    fn mimo_omni_does_not_get_suffix() {
        // Omni tops at 256K.
        assert_eq!(for_anthropic("mimo", "mimo-v2-omni"), "mimo-v2-omni");
    }

    #[test]
    fn mimo_already_suffixed_passes_through() {
        assert_eq!(
            for_anthropic("mimo", "mimo-v2.5-pro[1m]"),
            "mimo-v2.5-pro[1m]"
        );
    }

    #[test]
    fn mimo_other_qualifier_respected() {
        // If user explicitly wrote [128k] or anything else, don't touch.
        assert_eq!(
            for_anthropic("mimo", "mimo-v2.5-pro[128k]"),
            "mimo-v2.5-pro[128k]"
        );
    }

    #[test]
    fn xiaomi_alias_recognized() {
        assert_eq!(for_anthropic("xiaomi", "mimo-v2.5-pro"), "mimo-v2.5-pro");
    }

    // ── DeepSeek ──

    #[test]
    fn deepseek_v4_pro_gets_1m_suffix() {
        assert_eq!(
            for_anthropic("deepseek", "deepseek-v4-pro"),
            "deepseek-v4-pro[1m]"
        );
    }

    #[test]
    fn deepseek_v4_flash_does_not_get_suffix() {
        // Per DeepSeek's CC doc, Flash is the recommended HAIKU model and
        // doesn't support [1m].
        assert_eq!(
            for_anthropic("deepseek", "deepseek-v4-flash"),
            "deepseek-v4-flash"
        );
    }

    #[test]
    fn deepseek_legacy_chat_passes_through() {
        // Legacy models (deepseek-chat / deepseek-reasoner) don't support [1m].
        assert_eq!(for_anthropic("deepseek", "deepseek-chat"), "deepseek-chat");
        assert_eq!(
            for_anthropic("deepseek", "deepseek-reasoner"),
            "deepseek-reasoner"
        );
    }

    #[test]
    fn deepseek_already_suffixed_passes_through() {
        assert_eq!(
            for_anthropic("deepseek", "deepseek-v4-pro[1m]"),
            "deepseek-v4-pro[1m]"
        );
    }

    // ── Other providers ──

    #[test]
    fn anthropic_native_unaffected() {
        assert_eq!(
            for_anthropic("anthropic", "claude-sonnet-4-6"),
            "claude-sonnet-4-6"
        );
    }

    #[test]
    fn openai_unaffected() {
        assert_eq!(for_anthropic("openai", "gpt-4o"), "gpt-4o");
    }

    #[test]
    fn unknown_provider_unaffected() {
        assert_eq!(
            for_anthropic("custom_openai_compatible", "anything"),
            "anything"
        );
    }
}
