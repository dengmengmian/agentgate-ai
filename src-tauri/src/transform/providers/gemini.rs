pub struct GeminiProvider;

impl super::ProviderTransform for GeminiProvider {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::ChatCompletionsRequest;
    use crate::transform::providers::ProviderTransform;

    fn req() -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: "gemini-2.5-flash".into(),
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
    fn gemini_provider_type() {
        assert_eq!(GeminiProvider.provider_type(), "");
    }

    #[test]
    fn gemini_clean_schemas_false() {
        assert!(!GeminiProvider.clean_schemas());
    }

    #[test]
    fn gemini_finalize_request_no_op() {
        let mut r = req();
        r.temperature = Some(0.5);
        GeminiProvider.finalize_request(&mut r, &None);
        assert_eq!(r.temperature, Some(0.5));
    }

    #[test]
    fn gemini_process_messages_pass_through() {
        let msgs = req().messages;
        let out = GeminiProvider.process_messages(msgs.clone()).unwrap();
        assert_eq!(out.len(), 0);
    }
}
