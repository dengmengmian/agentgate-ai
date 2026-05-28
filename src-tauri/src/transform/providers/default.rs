pub struct DefaultProvider;

impl super::ProviderTransform for DefaultProvider {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::ChatCompletionsRequest;
    use crate::transform::providers::ProviderTransform;

    fn req() -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: "gpt-4".into(),
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
    fn default_provider_type() {
        assert_eq!(DefaultProvider.provider_type(), "");
    }

    #[test]
    fn default_clean_schemas() {
        assert!(!DefaultProvider.clean_schemas());
    }

    #[test]
    fn default_finalize_request_no_op() {
        let mut r = req();
        r.temperature = Some(0.7);
        DefaultProvider.finalize_request(&mut r, &None);
        assert_eq!(r.temperature, Some(0.7));
    }

    #[test]
    fn default_process_messages_pass_through() {
        let msgs = req().messages;
        let out = DefaultProvider.process_messages(msgs.clone()).unwrap();
        assert_eq!(out.len(), 0);
    }
}
