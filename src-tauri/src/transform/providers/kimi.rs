use crate::protocol::chat_completions::ChatCompletionsRequest;
use crate::transform::tool_calls;
use serde_json::{json, Value};

pub struct KimiProvider;

impl super::ProviderTransform for KimiProvider {
    fn finalize_request(&self, req: &mut ChatCompletionsRequest, tools: &Option<Vec<Value>>) {
        // Disable thinking when $web_search tool is present
        if let Some(ref tools) = tools {
            if tool_calls::contains_kimi_web_search(tools) {
                req.thinking = Some(json!({"type": "disabled"}));
            }
        }
    }

    fn provider_type(&self) -> &str {
        "kimi"
    }
}
