use crate::protocol::chat_completions::ChatCompletionsRequest;
use serde_json::Value;

pub struct MiniMaxProvider;

impl super::ProviderTransform for MiniMaxProvider {
    fn finalize_request(&self, req: &mut ChatCompletionsRequest, _tools: &Option<Vec<Value>>) {
        // MiniMax doesn't support reasoning_effort
        req.reasoning_effort = None;
        // MiniMax doesn't support response_format
        req.response_format = None;
    }

    fn provider_type(&self) -> &str {
        "minimax"
    }
}
