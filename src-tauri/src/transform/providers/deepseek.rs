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
}
