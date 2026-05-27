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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::ChatCompletionsRequest;
    use crate::transform::providers::ProviderTransform;
    use serde_json::json;

    fn req() -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: "kimi-k2".into(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: false,
            temperature: None,
            top_p: None,
            max_tokens: None,
            thinking: None,
            stream_options: None,
            response_format: None,
            reasoning_effort: None,
            seed: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
        }
    }

    #[test]
    fn kimi_disables_thinking_with_web_search() {
        let mut r = req();
        let tools = Some(vec![json!({"type": "builtin_function", "function": {"name": "$web_search"}})]);
        r.tools = tools.clone();
        KimiProvider.finalize_request(&mut r, &tools);
        assert_eq!(r.thinking, Some(json!({"type": "disabled"})));
    }

    #[test]
    fn kimi_keeps_thinking_without_web_search() {
        let mut r = req();
        r.thinking = Some(json!({"type": "enabled"}));
        let tools = Some(vec![json!({"type": "function", "function": {"name": "get_weather"}})]);
        r.tools = tools.clone();
        KimiProvider.finalize_request(&mut r, &tools);
        assert_eq!(r.thinking, Some(json!({"type": "enabled"})));
    }

    #[test]
    fn kimi_provider_type() {
        assert_eq!(KimiProvider.provider_type(), "kimi");
    }
}
