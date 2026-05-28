use crate::errors::AppError;
use crate::protocol::chat_completions::{ChatCompletionsRequest, ChatMessage};
use crate::transform::tool_calls;
use crate::transform::reasoning_store;
use serde_json::{json, Value};

pub struct DeepSeekProvider;

impl super::ProviderTransform for DeepSeekProvider {
    fn process_messages(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Vec<ChatMessage>, AppError> {
        let mut messages = tool_calls::fix_tool_message_order(messages)?;

        // Strip image_url content from messages (DeepSeek 400s on image_url)
        for msg in &mut messages {
            if let Some(Value::Array(parts)) = &msg.content {
                let has_image = parts
                    .iter()
                    .any(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url"));
                if has_image {
                    let text_only: Vec<Value> = parts
                        .iter()
                        .filter(|p| {
                            p.get("type").and_then(|t| t.as_str()) != Some("image_url")
                        })
                        .cloned()
                        .collect();
                    msg.content = if text_only.is_empty() {
                        Some(Value::String(String::new()))
                    } else {
                        Some(Value::Array(text_only))
                    };
                }
            }
        }

        // Ensure reasoning_content on assistant messages with tool_calls
        // (DeepSeek thinking mode requires this, empty " " as placeholder)
        for msg in &mut messages {
            if msg.role == "assistant" && msg.tool_calls.is_some() && msg.reasoning_content.is_none()
            {
                let text = msg
                    .content
                    .as_ref()
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let stored = reasoning_store::lookup_by_content(text).or_else(|| {
                    msg.tool_calls.as_ref().and_then(|tcs| {
                        tcs.iter()
                            .find_map(|tc| reasoning_store::lookup_by_tool_call_id(&tc.id))
                    })
                });
                msg.reasoning_content = stored.or_else(|| Some(" ".to_string()));
            }
        }

        Ok(messages)
    }

    fn finalize_request(&self, req: &mut ChatCompletionsRequest, _tools: &Option<Vec<Value>>) {
        // Don't send `thinking` field — it's MiMo-specific, DeepSeek ignores unknown fields.
        // DeepSeek V4 reasoning is controlled by the model itself, not by a request parameter.
        req.thinking = None;
        // DeepSeek doesn't support reasoning_effort
        req.reasoning_effort = None;
        // Downgrade json_schema to json_object (DeepSeek doesn't support json_schema)
        if let Some(ref fmt) = req.response_format {
            if fmt.get("type").and_then(|t| t.as_str()) == Some("json_schema") {
                req.response_format = Some(json!({"type": "json_object"}));
            }
        }
    }

    fn clean_schemas(&self) -> bool {
        true
    }

    fn provider_type(&self) -> &str {
        "deepseek"
    }

    fn enhance_error(&self, status: u16, body: &str) -> Option<String> {
        use crate::transform::providers as p;
        if p::detect_insufficient_balance(status, body) {
            return Some(
                "DeepSeek 账户余额不足。\n\
                 • 充值入口：https://platform.deepseek.com/top_up\n\
                 • 用量查询：https://platform.deepseek.com/usage\n\
                 • 或临时切换到 AgentGate 里其它 provider（路由会自动 failover 到非冷却状态的候选）。"
                    .to_string(),
            );
        }
        if p::detect_auth_error(status, body) {
            return Some(
                "DeepSeek API key 无效 / 过期。\n\
                 • 查看 / 重建 key：https://platform.deepseek.com/api_keys\n\
                 • 检查 AgentGate provider 配置里的 key 是否粘贴完整。"
                    .to_string(),
            );
        }
        if p::detect_rate_limit(status, body) {
            return Some(
                "DeepSeek 触发限流。AgentGate 已自动冷却该 provider 一段时间；\n\
                 • 路由 profile 有其它 candidate 时会自动 failover\n\
                 • 配额信息：https://platform.deepseek.com/usage"
                    .to_string(),
            );
        }
        // Fall back to shared context-overflow detection.
        p::detect_context_overflow(status, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::{ChatCompletionsRequest, ChatMessage, ToolCall, ToolCallFunction};
    use crate::transform::providers::ProviderTransform;
    use serde_json::json;

    fn req() -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: "deepseek-chat".into(),
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
    fn deepseek_strips_thinking() {
        let mut r = req();
        r.thinking = Some(json!({"type": "enabled"}));
        DeepSeekProvider.finalize_request(&mut r, &None);
        assert!(r.thinking.is_none());
    }

    #[test]
    fn deepseek_strips_reasoning_effort() {
        let mut r = req();
        r.reasoning_effort = Some("high".into());
        DeepSeekProvider.finalize_request(&mut r, &None);
        assert!(r.reasoning_effort.is_none());
    }

    #[test]
    fn deepseek_downgrades_json_schema() {
        let mut r = req();
        r.response_format = Some(json!({"type": "json_schema", "schema": {}}));
        DeepSeekProvider.finalize_request(&mut r, &None);
        assert_eq!(r.response_format, Some(json!({"type": "json_object"})));
    }

    #[test]
    fn deepseek_keeps_json_object() {
        let mut r = req();
        r.response_format = Some(json!({"type": "json_object"}));
        DeepSeekProvider.finalize_request(&mut r, &None);
        assert_eq!(r.response_format, Some(json!({"type": "json_object"})));
    }

    #[test]
    fn deepseek_process_messages_strips_image_url() {
        let messages = vec![ChatMessage {
            role: "user".into(),
            content: Some(json!([
                {"type": "text", "text": "look"},
                {"type": "image_url", "image_url": {"url": "http://example.com/img.png"}}
            ])),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        let out = DeepSeekProvider.process_messages(messages).unwrap();
        let arr = out[0].content.as_ref().unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "text");
    }

    #[test]
    fn deepseek_process_messages_image_only_becomes_empty_string() {
        let messages = vec![ChatMessage {
            role: "user".into(),
            content: Some(json!([
                {"type": "image_url", "image_url": {"url": "http://example.com/img.png"}}
            ])),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        let out = DeepSeekProvider.process_messages(messages).unwrap();
        assert_eq!(out[0].content, Some(json!("")));
    }

    #[test]
    fn deepseek_process_messages_backfills_reasoning_for_tool_calls() {
        let messages = vec![ChatMessage {
            role: "assistant".into(),
            content: Some(json!("text")),
            reasoning_content: None,
            tool_calls: Some(vec![ToolCall {
                id: "tc-1".into(),
                call_type: "function".into(),
                function: ToolCallFunction { name: "f".into(), arguments: "{}".into() },
            }]),
            tool_call_id: None,
            name: None,
        }];
        let out = DeepSeekProvider.process_messages(messages).unwrap();
        assert!(out[0].reasoning_content.is_some());
    }

    #[test]
    fn deepseek_provider_type() {
        assert_eq!(DeepSeekProvider.provider_type(), "deepseek");
    }

    #[test]
    fn deepseek_clean_schemas() {
        assert!(DeepSeekProvider.clean_schemas());
    }

    #[test]
    fn deepseek_enhances_insufficient_balance() {
        let hint = DeepSeekProvider
            .enhance_error(402, r#"{"error":{"code":"insufficient_balance"}}"#)
            .unwrap();
        assert!(hint.contains("DeepSeek"));
        assert!(hint.contains("top_up"));
    }

    #[test]
    fn deepseek_enhances_invalid_key() {
        let hint = DeepSeekProvider.enhance_error(401, "Invalid API key").unwrap();
        assert!(hint.contains("api_keys"));
    }

    #[test]
    fn deepseek_enhances_rate_limit() {
        let hint = DeepSeekProvider.enhance_error(429, "").unwrap();
        assert!(hint.contains("限流") || hint.contains("usage"));
    }

    #[test]
    fn deepseek_falls_back_to_context_overflow() {
        let hint = DeepSeekProvider.enhance_error(400, "context_length_exceeded").unwrap();
        assert!(hint.contains("compact") || hint.contains("上下文"));
    }
}
