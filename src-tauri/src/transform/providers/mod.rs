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
