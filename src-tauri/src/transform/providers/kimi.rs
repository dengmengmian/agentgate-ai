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

    fn enhance_error(&self, status: u16, body: &str) -> Option<String> {
        use crate::transform::providers as p;
        if p::detect_insufficient_balance(status, body) {
            return Some(
                "Kimi 账户余额 / 配额不足。\n\
                 • 充值入口：https://platform.moonshot.cn/console/account\n\
                 • 用量查询：https://platform.moonshot.cn/console/usage\n\
                 • AgentGate 会自动 failover 到其它非冷却 provider。"
                    .to_string(),
            );
        }
        if p::detect_auth_error(status, body) {
            return Some(
                "Kimi API key 无效 / 过期。\n\
                 • 重建 key：https://platform.moonshot.cn/console/api-keys"
                    .to_string(),
            );
        }
        if p::detect_rate_limit(status, body) {
            return Some(
                "Kimi 触发限流。RPM / TPM 上限因账户级别不同：\n\
                 • https://platform.moonshot.cn/console/info 查看你的速率配额\n\
                 • AgentGate 已冷却该 provider，路由会优先尝试其它候选"
                    .to_string(),
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
            parallel_tool_calls: None,
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
