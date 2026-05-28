//! Model name handling for the Anthropic Messages passthrough path.
//!
//! Some providers expose a CC-only ("Claude Code") syntax on their Anthropic-
//! compat endpoint that doesn't exist on their OpenAI endpoint. Examples are
//! `[1m]` suffixed model IDs used to opt the request into a 1M-token context
//! window:
//!
//!   * MiMo:     "mimo-v2.5-pro[1m]"     (only on /anthropic/v1/messages)
//!   * DeepSeek: "deepseek-v4-pro[1m]"  (only on /anthropic/v1/messages)
//!
//! AgentGate does not force these suffixes. If the user explicitly configures
//! a suffixed model through the provider default model or model_mapping, the
//! value passes through unchanged.

/// Rewrite the outgoing model field for the Anthropic passthrough path. If
/// the provider has no CC-only rewrite rule, returns the model unchanged.
pub fn for_anthropic(_provider_type: &str, model: &str) -> String {
    model.to_string()
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
    fn deepseek_v4_pro_passes_through() {
        assert_eq!(for_anthropic("deepseek", "deepseek-v4-pro"), "deepseek-v4-pro");
    }

    #[test]
    fn deepseek_v4_flash_passes_through() {
        assert_eq!(
            for_anthropic("deepseek", "deepseek-v4-flash"),
            "deepseek-v4-flash"
        );
    }

    #[test]
    fn deepseek_legacy_chat_passes_through() {
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
