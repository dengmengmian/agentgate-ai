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
    fn process_messages(&self, messages: Vec<ChatMessage>) -> Result<Vec<ChatMessage>, AppError> {
        Ok(messages)
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
    fn enhance_error(&self, _status: u16, _body_snippet: &str) -> Option<String> {
        None
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
