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

    fn enhance_error(&self, status: u16, body: &str) -> Option<String> {
        use crate::transform::providers as p;
        if p::detect_insufficient_balance(status, body) {
            return Some(
                "MiniMax 账户余额不足。\n\
                 • 充值入口：https://platform.minimaxi.com/user-center/finance/balance\n\
                 • 用量查询：https://platform.minimaxi.com/user-center/finance/usage"
                    .to_string(),
            );
        }
        if p::detect_auth_error(status, body) {
            return Some(
                "MiniMax API key 无效 / 过期。\n\
                 • 重建 key：https://platform.minimaxi.com/user-center/basic-information/interface-key"
                    .to_string(),
            );
        }
        if p::detect_rate_limit(status, body) {
            return Some(
                "MiniMax 触发限流。AgentGate 已冷却该 provider，路由会自动切换候选。".to_string(),
            );
        }
        p::detect_common_400(status, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::ChatCompletionsRequest;
    use crate::transform::providers::ProviderTransform;

    fn req() -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: "minimax-pro".into(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: false,
            temperature: None,
            top_p: None,
            max_tokens: None,
            max_completion_tokens: None,
            thinking: None,
            stream_options: None,
            response_format: None,
            reasoning_effort: None,
            seed: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            parallel_tool_calls: None,
            diagnostic_events: Vec::new(),
        }
    }

    #[test]
    fn minimax_strips_reasoning_effort() {
        let mut r = req();
        r.reasoning_effort = Some("high".into());
        MiniMaxProvider.finalize_request(&mut r, &None);
        assert!(r.reasoning_effort.is_none());
    }

    #[test]
    fn minimax_strips_response_format() {
        let mut r = req();
        r.response_format = Some(serde_json::json!({"type": "json_object"}));
        MiniMaxProvider.finalize_request(&mut r, &None);
        assert!(r.response_format.is_none());
    }

    #[test]
    fn minimax_provider_type() {
        assert_eq!(MiniMaxProvider.provider_type(), "minimax");
    }
}
