use crate::providers::adapter::ProviderConfig;
use crate::protocol::chat_completions::{ChatCompletionsRequest, ChatMessage};
use crate::errors::AppError;
use serde_json::Value;

mod deepseek;
mod kimi;
mod minimax;
mod default;
mod anthropic;
mod gemini;
mod mimo;

pub use deepseek::DeepSeekProvider;
pub use kimi::KimiProvider;
pub use minimax::MiniMaxProvider;
pub use default::DefaultProvider;
pub use anthropic::AnthropicProvider;
pub use gemini::GeminiProvider;
pub use mimo::MimoProvider;

/// Per-provider hooks for transforming Responses API → Chat Completions API.
///
/// Each provider only overrides what it needs; all other behavior falls through
/// to the common logic in `responses_to_chat::convert_with_provider`.
pub trait ProviderTransform: Send + Sync {
    /// Process messages after initial conversion from Responses format.
    /// Called before merge_consecutive_messages and tool argument sanitization.
    ///
    /// Default impl runs a structural safety pass that synthesizes placeholder
    /// tool messages for any orphaned tool_call_id (cancelled turns, dropped
    /// outputs, etc.) without reordering. Providers that need stricter handling
    /// (DeepSeek, MiMo) override and use `fix_tool_message_order` instead.
    fn process_messages(&self, messages: Vec<ChatMessage>) -> Result<Vec<ChatMessage>, AppError> {
        Ok(crate::transform::tool_calls::synthesize_orphan_tool_outputs(messages))
    }

    /// Finalize the ChatCompletionsRequest before sending to the provider.
    /// Called after all common fields are set; provider can override any field
    /// (e.g. thinking, reasoning_effort, response_format).
    fn finalize_request(&self, _req: &mut ChatCompletionsRequest, _tools: &Option<Vec<Value>>) {}

    /// Whether to clean JSON schemas (remove `strict`, `additionalProperties`).
    fn clean_schemas(&self) -> bool {
        false
    }

    /// Provider type string, used for tool conversion awareness
    /// (e.g. Kimi's web_search → builtin_function).
    fn provider_type(&self) -> &str {
        ""
    }

    /// Map an upstream HTTP error (non-2xx) to an actionable suggestion that
    /// the gateway will attach to the AppError. Called from the request
    /// dispatcher with the sanitized response body snippet so providers can
    /// pattern-match against known error markers (e.g. MiMo's
    /// "webSearchEnabled is false" 400 → "activate the Web Search Plugin").
    /// Return None to use the generic upstream error formatting.
    ///
    /// Default implementation detects common context-window-exceeded markers
    /// across vendors and surfaces a `/compact` hint. Providers that override
    /// to handle other errors should call `detect_context_overflow(...)` as
    /// a fallback when their own match fails.
    fn enhance_error(&self, status: u16, body_snippet: &str) -> Option<String> {
        detect_context_overflow(status, body_snippet)
    }
}

/// Shared detection for "insufficient balance / quota / credit" 4xx errors.
/// Returns a generic suggestion text; callers (per-provider enhance_error)
/// can wrap with a provider-specific top-up URL.
pub fn detect_insufficient_balance(status: u16, body_snippet: &str) -> bool {
    if status != 402 && status != 403 && status != 429 && status != 400 {
        return false;
    }
    let lower = body_snippet.to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "insufficient_balance",
        "insufficient balance",
        "insufficient_quota",
        "insufficient quota",
        "credit_balance",
        "out of credit",
        "no credits",
        "account balance",
        "billing_hard_limit_reached",
        "balance is too low",
        "exceeded your current quota",
        "余额不足",
        "余额为",
        "账户余额",
        "请充值",
        "请前往",
        "费用已超",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
}

/// Shared detection for "invalid / missing / expired API key" 401 / 403 errors.
pub fn detect_auth_error(status: u16, body_snippet: &str) -> bool {
    if status != 401 && status != 403 {
        return false;
    }
    let lower = body_snippet.to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "invalid_api_key",
        "invalid api key",
        "invalid token",
        "incorrect api key",
        "authentication_error",
        "authentication failed",
        "unauthorized",
        "api key not valid",
        "api_key_invalid",
        "missing api key",
        "expired",
        "未授权",
        "鉴权失败",
        "认证失败",
        "api key 无效",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
}

/// Shared detection for rate-limit 429 (also some 503 with retry-after).
pub fn detect_rate_limit(status: u16, body_snippet: &str) -> bool {
    if status != 429 && status != 503 {
        return false;
    }
    let lower = body_snippet.to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "rate_limit",
        "rate limit",
        "too many requests",
        "too_many_requests",
        "tpm_limit",
        "rpm_limit",
        "quota_exceeded",
        "请求过于频繁",
        "速率限制",
        "请求过快",
    ];
    // 429 alone is usually rate limit even without marker text
    status == 429 || MARKERS.iter().any(|m| lower.contains(m))
}

/// Shared detection for "context window exceeded" 400s across providers.
/// Pattern set covers OpenAI / Anthropic / DeepSeek / MiMo / Kimi / MiniMax /
/// generic Chinese wording. Case-insensitive substring match against the
/// upstream's error body snippet.
pub fn detect_context_overflow(status: u16, body_snippet: &str) -> Option<String> {
    if status != 400 {
        return None;
    }
    let lower = body_snippet.to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "context length",
        "context_length",
        "context window",
        "context_window_exceeded",
        "maximum context",
        "exceeds the maximum",
        "too long for context",
        "prompt is too long",
        "input is too long",
        "tokens exceeded",
        "上下文长度",
        "上下文窗口",
        "超出最大",
        "超出上下文",
    ];
    if MARKERS.iter().any(|m| lower.contains(m)) {
        Some(
            "请求超出模型上下文窗口。\n\
             • 如果你在 Codex / Claude Code 中，输入 /compact 压缩历史后重试。\n\
             • 或开启新会话，避免过长的对话历史。"
                .to_string(),
        )
    } else {
        None
    }
}

#[cfg(test)]
mod detector_tests {
    use super::*;

    #[test]
    fn balance_markers_trigger() {
        for (status, snippet) in [
            (402, r#"{"error":{"code":"insufficient_balance"}}"#),
            (403, "Insufficient Balance"),
            (400, "余额不足，请前往充值"),
            (429, "exceeded your current quota"),
            (400, "credit_balance is too low"),
            (400, "billing_hard_limit_reached"),
        ] {
            assert!(detect_insufficient_balance(status, snippet),
                "should detect: {status} / {snippet}");
        }
    }

    #[test]
    fn balance_skips_unrelated_status() {
        assert!(!detect_insufficient_balance(200, "Insufficient Balance"));
        assert!(!detect_insufficient_balance(500, "余额不足"));
        assert!(!detect_insufficient_balance(400, "completely unrelated error"));
    }

    #[test]
    fn auth_markers_trigger() {
        for (status, snippet) in [
            (401, "Unauthorized"),
            (401, r#"{"error":{"type":"authentication_error"}}"#),
            (401, "Invalid API key"),
            (403, "api_key_invalid"),
            (403, "未授权访问"),
            (401, "鉴权失败"),
        ] {
            assert!(detect_auth_error(status, snippet), "should detect: {status} / {snippet}");
        }
    }

    #[test]
    fn auth_skips_non_4xx() {
        assert!(!detect_auth_error(429, "Unauthorized"));
        assert!(!detect_auth_error(500, "Invalid API key"));
    }

    #[test]
    fn rate_limit_triggers_on_429_even_without_marker() {
        // Some upstreams return just "429 Too Many Requests" with empty body.
        assert!(detect_rate_limit(429, ""));
        assert!(detect_rate_limit(429, "anything at all"));
    }

    #[test]
    fn rate_limit_503_needs_marker() {
        assert!(detect_rate_limit(503, "rate_limit"));
        assert!(!detect_rate_limit(503, ""));
    }

    #[test]
    fn rate_limit_skips_other_status() {
        assert!(!detect_rate_limit(200, "rate_limit"));
        assert!(!detect_rate_limit(400, "rate_limit"));
    }

    #[test]
    fn detectors_are_independent_no_overlap() {
        // A pure balance error shouldn't be flagged as auth or rate_limit.
        let snippet = "余额不足";
        assert!(detect_insufficient_balance(402, snippet));
        assert!(!detect_auth_error(402, snippet));
        // 429 always means rate_limit even with balance-ish text, that's
        // intentional — `detect_insufficient_balance` returns true too on
        // 429 with quota markers, callers check balance first by convention.
    }
}

/// Dispatch to the correct provider transform based on the provider config.
pub fn for_config(config: &ProviderConfig) -> Box<dyn ProviderTransform + Send + Sync> {
    let pt = config.provider_type.as_str();
    if pt == "deepseek" {
        Box::new(DeepSeekProvider)
    } else if pt == "kimi" || pt.contains("moonshot") {
        Box::new(KimiProvider)
    } else if pt == "minimax" || pt.contains("minimax") {
        Box::new(MiniMaxProvider)
    } else if pt == "anthropic" || pt == "claude" {
        Box::new(AnthropicProvider)
    } else if pt == "google_gemini" {
        Box::new(GeminiProvider)
    } else if pt == "mimo" || pt == "xiaomi" || pt.contains("mimo") {
        Box::new(MimoProvider)
    } else {
        Box::new(DefaultProvider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::adapter::ProviderConfig;

    fn config(provider_type: &str) -> ProviderConfig {
        ProviderConfig {
            name: "Test".into(),
            provider_type: provider_type.into(),
            base_url: "http://localhost".into(),
            api_keys: vec!["sk-test".into()],
            default_model: "model".into(),
            reasoning_model: None,
            timeout_seconds: 30,
            extra_headers: std::collections::HashMap::new(),
            anthropic_base_url: None,
            responses_base_url: None,
        }
    }

    #[test]
    fn for_config_deepseek() {
        let t = for_config(&config("deepseek"));
        assert_eq!(t.provider_type(), "deepseek");
    }

    #[test]
    fn for_config_kimi() {
        let t = for_config(&config("kimi"));
        assert_eq!(t.provider_type(), "kimi");
    }

    #[test]
    fn for_config_moonshot() {
        let t = for_config(&config("moonshot"));
        assert_eq!(t.provider_type(), "kimi");
    }

    #[test]
    fn for_config_minimax() {
        let t = for_config(&config("minimax"));
        assert_eq!(t.provider_type(), "minimax");
    }

    #[test]
    fn for_config_anthropic() {
        let t = for_config(&config("anthropic"));
        assert_eq!(t.provider_type(), "");
    }

    #[test]
    fn for_config_claude() {
        let t = for_config(&config("claude"));
        assert_eq!(t.provider_type(), "");
    }

    #[test]
    fn for_config_google_gemini() {
        let t = for_config(&config("google_gemini"));
        assert_eq!(t.provider_type(), "");
    }

    #[test]
    fn for_config_mimo() {
        let t = for_config(&config("mimo"));
        assert_eq!(t.provider_type(), "mimo");
    }

    #[test]
    fn for_config_xiaomi() {
        let t = for_config(&config("xiaomi"));
        assert_eq!(t.provider_type(), "mimo");
    }

    #[test]
    fn for_config_unknown_defaults() {
        let t = for_config(&config("openai"));
        assert_eq!(t.provider_type(), "");
    }
}
