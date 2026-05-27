pub struct AnthropicProvider;

impl super::ProviderTransform for AnthropicProvider {}

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
