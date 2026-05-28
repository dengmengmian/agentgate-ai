pub struct AnthropicProvider;

impl super::ProviderTransform for AnthropicProvider {
    fn enhance_error(&self, status: u16, body: &str) -> Option<String> {
        use crate::transform::providers as p;
        let lower = body.to_ascii_lowercase();
        // Anthropic-specific marker
        if lower.contains("credit_balance") || p::detect_insufficient_balance(status, body) {
            return Some(
                "Anthropic 账户余额不足。\n\
                 • 充值入口：https://console.anthropic.com/settings/billing\n\
                 • 用量查询：https://console.anthropic.com/settings/usage\n\
                 • AgentGate 路由若有其它候选会自动 failover。"
                    .to_string(),
            );
        }
        if lower.contains("overloaded_error") {
            return Some(
                "Anthropic 当前负载过高（overloaded_error）—— 不是你的账户问题。\n\
                 AgentGate 已自动重试，仍失败建议稍后重发或切到 DeepSeek / MiMo 等其它 provider。"
                    .to_string(),
            );
        }
        if p::detect_auth_error(status, body) {
            return Some(
                "Anthropic API key 无效 / 过期。\n\
                 • 重建 key：https://console.anthropic.com/settings/keys"
                    .to_string(),
            );
        }
        if p::detect_rate_limit(status, body) {
            return Some(
                "Anthropic 触发限流（rate_limit_error）。\n\
                 • 提升等级 / 速率配额：https://console.anthropic.com/settings/limits"
                    .to_string(),
            );
        }
        p::detect_context_overflow(status, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::ChatCompletionsRequest;
    use crate::transform::providers::ProviderTransform;

    fn req() -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: "claude-sonnet".into(),
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
    fn anthropic_default_provider_type() {
        assert_eq!(AnthropicProvider.provider_type(), "");
    }

    #[test]
    fn anthropic_default_clean_schemas() {
        assert!(!AnthropicProvider.clean_schemas());
    }

    #[test]
    fn anthropic_process_messages_pass_through() {
        let msgs = req().messages;
        let out = AnthropicProvider.process_messages(msgs.clone()).unwrap();
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn anthropic_finalize_request_no_op() {
        let mut r = req();
        r.temperature = Some(0.5);
        AnthropicProvider.finalize_request(&mut r, &None);
        assert_eq!(r.temperature, Some(0.5));
    }

    #[test]
    fn anthropic_enhance_error_returns_none() {
        assert!(AnthropicProvider.enhance_error(400, "error").is_none());
    }
}
