use serde_json::Value;

use crate::errors::AppError;
use crate::protocol::chat_completions::{ChatCompletionsRequest, ChatMessage, ToolCall, ToolCallFunction};
use crate::protocol::openai_responses::ResponsesRequest;
use crate::transform::tool_calls;
use crate::transform::reasoning_store;

pub fn convert_with_provider(
    req: &ResponsesRequest,
    model: &str,
    clean_for_deepseek: bool,
    provider_type: &str,
) -> Result<ChatCompletionsRequest, AppError> {
    let mut messages = Vec::new();

    // 0. Replay history from previous_response_id (session store)
    if let Some(ref prev_id) = req.previous_response_id {
        if let Some(history) = crate::gateway::session_store::get_history(prev_id) {
            messages.extend(history);
        }
    }

    // 1. instructions / system -> system message (only if not already present from history)
    let system_text = req.instructions.as_ref().or(req.system.as_ref());
    if let Some(text) = system_text {
        if !text.is_empty() {
            // Remove any existing system message from history to avoid duplication
            messages.retain(|m| m.role != "system");
            messages.insert(0, ChatMessage {
                role: "system".to_string(),
                content: Some(Value::String(text.clone())),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
    }

    // 2. Convert input
    let input_messages = convert_input(&req.input)?;
    messages.extend(input_messages);

    // 3. Convert tools (provider-aware for Kimi web_search)
    let converted_tools = req.tools.as_ref().map(|t| {
        tool_calls::convert_tools_for_provider(t, clean_for_deepseek, provider_type)
    }).filter(|t| !t.is_empty());

    // 4. Convert tool_choice
    let tool_choice = req.tool_choice.as_ref().map(tool_calls::convert_tool_choice);

    // 5. Fix tool message order for DeepSeek
    if clean_for_deepseek {
        messages = tool_calls::fix_tool_message_order(messages)?;

        // 6. Strip image_url content from messages (DeepSeek 400s on image_url)
        for msg in &mut messages {
            if let Some(Value::Array(parts)) = &msg.content {
                let has_image = parts.iter().any(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url"));
                if has_image {
                    let text_only: Vec<Value> = parts.iter()
                        .filter(|p| p.get("type").and_then(|t| t.as_str()) != Some("image_url"))
                        .cloned().collect();
                    msg.content = if text_only.is_empty() {
                        Some(Value::String(String::new()))
                    } else {
                        Some(Value::Array(text_only))
                    };
                }
            }
        }

        // 7. Ensure reasoning_content on assistant messages with tool_calls
        //    (DeepSeek thinking mode requires this, empty " " as placeholder)
        for msg in &mut messages {
            if msg.role == "assistant" && msg.tool_calls.is_some() && msg.reasoning_content.is_none() {
                // Check if any reasoning exists in the store for this context
                let text = msg.content.as_ref()
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let stored = reasoning_store::lookup_by_content(text)
                    .or_else(|| {
                        msg.tool_calls.as_ref().and_then(|tcs| {
                            tcs.iter().find_map(|tc| reasoning_store::lookup_by_tool_call_id(&tc.id))
                        })
                    });
                msg.reasoning_content = stored.or_else(|| Some(" ".to_string()));
            }
        }
    }

    // 8. Merge consecutive messages of the same role
    //    (some providers reject user→user or assistant→assistant sequences)
    messages = merge_consecutive_messages(messages);

    // 9. Sanitize tool call arguments (invalid JSON -> "{}")
    for msg in &mut messages {
        if let Some(ref mut tcs) = msg.tool_calls {
            for tc in tcs {
                if !tc.function.arguments.is_empty() {
                    if serde_json::from_str::<Value>(&tc.function.arguments).is_err() {
                        tc.function.arguments = "{}".to_string();
                    }
                }
            }
        }
    }

    // Kimi: force disable thinking when $web_search tool is present
    let thinking = if let Some(ref tools) = converted_tools {
        if tool_calls::contains_kimi_web_search(tools) {
            Some(serde_json::json!({"type": "disabled"}))
        } else {
            None
        }
    } else {
        None
    };

    // Auto-inject stream_options for usage capture
    let stream_options = if req.stream.unwrap_or(false) {
        Some(serde_json::json!({"include_usage": true}))
    } else {
        None
    };

    // Convert reasoning.effort → reasoning_effort
    // "智能"=auto → None (provider default), "超高"=xhigh → "high"
    let reasoning_effort = req.reasoning.as_ref()
        .and_then(|r| r.get("effort"))
        .and_then(|e| e.as_str())
        .and_then(|e| match e.trim().to_ascii_lowercase().as_str() {
            "minimal" | "low" | "medium" | "high" => Some(e.trim().to_ascii_lowercase()),
            "xhigh" | "max" | "highest" => Some("high".to_string()),
            "none" | "off" | "auto" | "" => None, // 智能 → let provider decide
            _ => None,
        });

    // MiniMax: strip unsupported fields (reasoning_effort, response_format)
    let is_minimax = provider_type == "minimax" || provider_type.contains("minimax");
    let reasoning_effort = if is_minimax { None } else { reasoning_effort };
    let response_format_allowed = !is_minimax;

    // Convert text.format → response_format (skip for MiniMax)
    let response_format = if !response_format_allowed { None } else { req.text.as_ref()
        .and_then(|t| t.get("format"))
        .and_then(|f| {
            let fmt_type = f.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match fmt_type {
                "json_object" => Some(serde_json::json!({"type": "json_object"})),
                "json_schema" => {
                    // DeepSeek doesn't support json_schema, downgrade to json_object
                    if clean_for_deepseek {
                        Some(serde_json::json!({"type": "json_object"}))
                    } else {
                        Some(f.clone())
                    }
                }
                _ => None,
            }
        })};

    Ok(ChatCompletionsRequest {
        model: model.to_string(),
        messages,
        tools: converted_tools,
        tool_choice,
        stream: req.stream.unwrap_or(false),
        temperature: req.temperature,
        top_p: req.top_p,
        max_tokens: req.max_output_tokens,
        thinking,
        stream_options,
        response_format,
        reasoning_effort,
        seed: req.seed.clone(),
        stop: req.stop.clone(),
        frequency_penalty: req.frequency_penalty,
        presence_penalty: req.presence_penalty,
    })
}

fn convert_input(input: &Value) -> Result<Vec<ChatMessage>, AppError> {
    match input {
        Value::String(s) => {
            Ok(vec![msg("user", Value::String(s.clone()))])
        }
        Value::Array(items) => convert_input_array(items),
        Value::Object(_) => {
            let content = extract_content(Some(input));
            Ok(vec![msg("user", content)])
        }
        _ => {
            Ok(vec![msg("user", Value::String(input.to_string()))])
        }
    }
}

fn convert_input_array(items: &[Value]) -> Result<Vec<ChatMessage>, AppError> {
    let mut messages = Vec::new();
    let mut pending_tool_calls: Vec<ToolCall> = Vec::new();
    let mut pending_reasoning: Option<String> = None;

    for item in items {
        let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match item_type {
            "message" => {
                flush_tool_calls(&mut messages, &mut pending_tool_calls, &mut pending_reasoning);

                let role = map_role(
                    item.get("role").and_then(|r| r.as_str()).unwrap_or("user"),
                );
                let content = extract_content(item.get("content"));

                // reasoning_content: from item itself, or pending, or look up from store
                let reasoning = if role == "assistant" {
                    item.get("reasoning_content")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .filter(|s| !s.is_empty())
                        .or_else(|| pending_reasoning.take())
                        .or_else(|| {
                            // Look up from reasoning store by content hash
                            let text = extract_content(item.get("content"));
                            let text_str = text.as_str().unwrap_or("");
                            reasoning_store::lookup_by_content(text_str)
                        })
                } else {
                    None
                };

                messages.push(ChatMessage {
                    role,
                    content: Some(content),
                    reasoning_content: reasoning,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
            "function_call" => {
                // Capture reasoning_content from function_call items (DeepSeek thinking mode)
                if let Some(rc) = item.get("reasoning_content").and_then(|v| v.as_str()) {
                    if !rc.is_empty() && pending_reasoning.is_none() {
                        pending_reasoning = Some(rc.to_string());
                    }
                }

                let call_id = item
                    .get("call_id")
                    .and_then(|c| c.as_str())
                    .unwrap_or("call_unknown")
                    .to_string();
                let name = item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = item
                    .get("arguments")
                    .map(|a| {
                        if a.is_string() {
                            a.as_str().unwrap().to_string()
                        } else {
                            a.to_string()
                        }
                    })
                    .unwrap_or_default();

                pending_tool_calls.push(ToolCall {
                    id: call_id,
                    call_type: "function".to_string(),
                    function: ToolCallFunction { name, arguments },
                });
            }
            "function_call_output" => {
                // Flush pending tool calls before adding tool response
                flush_tool_calls(&mut messages, &mut pending_tool_calls, &mut pending_reasoning);

                let call_id = item
                    .get("call_id")
                    .and_then(|c| c.as_str());

                if call_id.is_none() || call_id == Some("") {
                    return Err(AppError::new(
                        "FUNCTION_CALL_OUTPUT_ID_MISSING",
                        "function_call_output is missing call_id",
                    ).with_suggestion("Each function_call_output must have a call_id matching a previous function_call"));
                }

                let raw_output = item.get("output").map(|o| {
                    if o.is_string() { o.as_str().unwrap().to_string() }
                    else { o.to_string() }
                }).unwrap_or_default();
                let output = Value::String(raw_output);

                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: Some(output),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some(call_id.unwrap().to_string()),
                    name: None,
                });
            }
            "compaction" | "context_compaction" | "compaction_summary" => {
                // Codex auto-compact: convert summary to user message
                flush_tool_calls(&mut messages, &mut pending_tool_calls, &mut pending_reasoning);
                let summary = item.get("summary")
                    .or(item.get("content"))
                    .map(|v| extract_content(Some(v)))
                    .unwrap_or(Value::String("[context compacted]".to_string()));
                messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: Some(summary),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
            "reasoning" => {
                // Capture reasoning content to attach to next assistant message
                if let Some(rc) = item.get("content").and_then(|v| v.as_str()) {
                    if !rc.is_empty() {
                        pending_reasoning = Some(rc.to_string());
                    }
                }
                // Also check summary field
                if pending_reasoning.is_none() {
                    if let Some(rc) = item.get("summary").and_then(|v| {
                        if v.is_string() { v.as_str().map(String::from) }
                        else if v.is_array() {
                            let texts: Vec<String> = v.as_array().unwrap().iter()
                                .filter_map(|p| p.get("text").and_then(|t| t.as_str()).map(String::from))
                                .collect();
                            if texts.is_empty() { None } else { Some(texts.join("")) }
                        } else { None }
                    }) {
                        if !rc.is_empty() { pending_reasoning = Some(rc); }
                    }
                }
            }
            _ => {
                // Unknown item: try to extract as message if it has role/content
                if let Some(role) = item.get("role").and_then(|r| r.as_str()) {
                    flush_tool_calls(&mut messages, &mut pending_tool_calls, &mut pending_reasoning);
                    let content = extract_content(item.get("content"));
                    messages.push(ChatMessage {
                        role: map_role(role),
                        content: Some(content),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                }
                // else: silently skip
            }
        }
    }

    // Flush remaining pending tool calls
    flush_tool_calls(&mut messages, &mut pending_tool_calls, &mut pending_reasoning);

    Ok(messages)
}

fn flush_tool_calls(messages: &mut Vec<ChatMessage>, pending: &mut Vec<ToolCall>, reasoning: &mut Option<String>) {
    if pending.is_empty() {
        return;
    }
    // Try to find reasoning from store by tool_call_id if not already available
    let rc = reasoning.take().or_else(|| {
        for tc in pending.iter() {
            if let Some(r) = reasoning_store::lookup_by_tool_call_id(&tc.id) {
                return Some(r);
            }
        }
        None
    });
    messages.push(ChatMessage {
        role: "assistant".to_string(),
        content: None,
        reasoning_content: rc,
        tool_calls: Some(std::mem::take(pending)),
        tool_call_id: None,
        name: None,
    });
}

fn msg(role: &str, content: Value) -> ChatMessage {
    ChatMessage {
        role: role.to_string(),
        content: Some(content),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }
}

fn map_role(role: &str) -> String {
    match role {
        "developer" => "system".to_string(),
        other => other.to_string(),
    }
}

fn extract_content(content: Option<&Value>) -> Value {
    match content {
        None => Value::String(String::new()),
        Some(Value::String(s)) => Value::String(s.clone()),
        Some(Value::Array(arr)) => {
            let texts: Vec<String> = arr
                .iter()
                .filter_map(|part| {
                    // Support input_text, output_text, text types
                    let pt = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match pt {
                        "input_text" | "output_text" | "text" => {
                            part.get("text").and_then(|t| t.as_str()).map(String::from)
                        }
                        _ => {
                            // Fallback: try "text" field anyway
                            part.get("text").and_then(|t| t.as_str()).map(String::from)
                        }
                    }
                })
                .collect();
            if texts.is_empty() {
                // If no text parts found, stringify the whole array
                Value::String(serde_json::to_string(arr).unwrap_or_default())
            } else {
                Value::String(texts.join(""))
            }
        }
        Some(Value::Object(obj)) => {
            if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                Value::String(text.to_string())
            } else {
                Value::String(serde_json::to_string(obj).unwrap_or_default())
            }
        }
        Some(other) => Value::String(other.to_string()),
    }
}

/// Merge consecutive messages of the same role (user+user, assistant+assistant).
/// Some providers reject consecutive same-role messages.
fn merge_consecutive_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut result: Vec<ChatMessage> = Vec::new();

    for msg in messages {
        if let Some(last) = result.last_mut() {
            // Only merge if same role, no tool_calls, no tool_call_id
            if last.role == msg.role
                && last.tool_calls.is_none()
                && msg.tool_calls.is_none()
                && last.tool_call_id.is_none()
                && msg.tool_call_id.is_none()
                && (msg.role == "user" || msg.role == "system")
            {
                // Merge content
                let last_text = last.content.as_ref().and_then(|c| c.as_str()).unwrap_or("");
                let new_text = msg.content.as_ref().and_then(|c| c.as_str()).unwrap_or("");
                if !new_text.is_empty() {
                    let merged = if last_text.is_empty() {
                        new_text.to_string()
                    } else {
                        format!("{last_text}\n\n{new_text}")
                    };
                    last.content = Some(Value::String(merged));
                }
                continue;
            }
        }
        result.push(msg);
    }

    result
}

/// Split `<think>...</think>` tags from content into reasoning_content.
/// Used for MiniMax-like providers that embed thinking in content.
/// Handles multiple `<think>` blocks by extracting all of them.
pub fn split_think_tags(content: &str) -> (String, Option<String>) {
    let mut remaining = String::new();
    let mut thinking_parts = Vec::new();
    let mut search_from = 0;

    while let Some(start) = content[search_from..].find("<think>") {
        let abs_start = search_from + start;
        // Append text before this <think> tag
        remaining.push_str(&content[search_from..abs_start]);

        if let Some(end) = content[abs_start..].find("</think>") {
            let abs_end = abs_start + end;
            let block = content[abs_start + 7..abs_end].trim();
            if !block.is_empty() {
                thinking_parts.push(block.to_string());
            }
            search_from = abs_end + 8;
        } else {
            // No closing tag — keep the rest as-is
            search_from = abs_start;
            break;
        }
    }
    // Append any trailing text
    remaining.push_str(&content[search_from..]);
    let remaining = remaining.trim().to_string();

    let thinking = if thinking_parts.is_empty() {
        None
    } else {
        Some(thinking_parts.join("\n\n"))
    };
    (remaining, thinking)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_convert_simple_string_input() {
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!("hello"),
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        assert_eq!(result.model, "gpt-4");
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].role, "user");
        assert_eq!(result.messages[0].content, Some(json!("hello")));
    }

    #[test]
    fn test_convert_with_instructions() {
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!("hello"),
            instructions: Some("Be helpful".to_string()),
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, "system");
        assert_eq!(result.messages[0].content, Some(json!("Be helpful")));
        assert_eq!(result.messages[1].role, "user");
    }

    #[test]
    fn test_convert_instructions_priority_over_system() {
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!("hello"),
            instructions: Some("Instr".to_string()),
            system: Some("Sys".to_string()),
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        assert_eq!(result.messages[0].content, Some(json!("Instr")));
    }

    #[test]
    fn test_convert_input_array_messages() {
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!([
                {"type": "message", "role": "user", "content": "hi"},
                {"type": "message", "role": "assistant", "content": "hello"}
            ]),
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, "user");
        assert_eq!(result.messages[1].role, "assistant");
    }

    #[test]
    fn test_convert_function_call_and_output() {
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!([
                {"type": "function_call", "call_id": "call_1", "name": "search", "arguments": "{\"q\":\"hi\"}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "result"}
            ]),
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].role, "assistant");
        assert!(result.messages[0].tool_calls.is_some());
        assert_eq!(result.messages[1].role, "tool");
        assert_eq!(result.messages[1].tool_call_id, Some("call_1".to_string()));
    }

    #[test]
    fn test_convert_missing_call_id_errors() {
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!([
                {"type": "function_call_output", "call_id": "", "output": "result"}
            ]),
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        };
        assert!(convert_with_provider(&req, "gpt-4", false, "").is_err());
    }

    #[test]
    fn test_convert_reuse_stream_options() {
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!("hi"),
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: Some(true),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        assert!(result.stream);
        assert!(result.stream_options.is_some());
        assert_eq!(result.stream_options.unwrap()["include_usage"], true);
    }

    #[test]
    fn test_convert_preserves_temperature_top_p_max_tokens() {
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!("hi"),
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: Some(0.7),
            top_p: Some(0.9),
            max_output_tokens: Some(1024),
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        assert_eq!(result.temperature, Some(0.7));
        assert_eq!(result.top_p, Some(0.9));
        assert_eq!(result.max_tokens, Some(1024));
    }

    #[test]
    fn test_convert_deepseek_strips_image_url_from_history() {
        // When content is already an Array (e.g. from history), image_url parts are stripped
        let mut messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(json!([
                {"type": "text", "text": "look"},
                {"type": "image_url", "image_url": {"url": "http://example.com/img.png"}}
            ])),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        // Replicate the DeepSeek image stripping logic
        for msg in &mut messages {
            if let Some(Value::Array(parts)) = &msg.content {
                let has_image = parts.iter().any(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url"));
                if has_image {
                    let text_only: Vec<Value> = parts.iter()
                        .filter(|p| p.get("type").and_then(|t| t.as_str()) != Some("image_url"))
                        .cloned().collect();
                    msg.content = if text_only.is_empty() {
                        Some(Value::String(String::new()))
                    } else {
                        Some(Value::Array(text_only))
                    };
                }
            }
        }
        let content = messages[0].content.as_ref().unwrap();
        if let Value::Array(parts) = content {
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0]["type"], "text");
        } else {
            panic!("Expected array content");
        }
    }

    #[test]
    fn test_convert_deepseek_image_only_becomes_empty_string_in_history() {
        let mut messages = vec![ChatMessage {
            role: "user".to_string(),
            content: Some(json!([
                {"type": "image_url", "image_url": {"url": "http://example.com/img.png"}}
            ])),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }];
        for msg in &mut messages {
            if let Some(Value::Array(parts)) = &msg.content {
                let has_image = parts.iter().any(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url"));
                if has_image {
                    let text_only: Vec<Value> = parts.iter()
                        .filter(|p| p.get("type").and_then(|t| t.as_str()) != Some("image_url"))
                        .cloned().collect();
                    msg.content = if text_only.is_empty() {
                        Some(Value::String(String::new()))
                    } else {
                        Some(Value::Array(text_only))
                    };
                }
            }
        }
        assert_eq!(messages[0].content, Some(json!("")));
    }

    #[test]
    fn test_merge_consecutive_user_messages() {
        let messages = vec![
            msg("user", json!("hello")),
            msg("user", json!("world")),
        ];
        let merged = merge_consecutive_messages(messages);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].content, Some(json!("hello\n\nworld")));
    }

    #[test]
    fn test_merge_consecutive_system_messages() {
        let messages = vec![
            msg("system", json!("sys1")),
            msg("system", json!("sys2")),
        ];
        let merged = merge_consecutive_messages(messages);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].content, Some(json!("sys1\n\nsys2")));
    }

    #[test]
    fn test_do_not_merge_assistant_messages() {
        let messages = vec![
            msg("assistant", json!("a1")),
            msg("assistant", json!("a2")),
        ];
        let merged = merge_consecutive_messages(messages);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_do_not_merge_messages_with_tool_calls() {
        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(json!("call")),
                reasoning_content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "tc1".to_string(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction { name: "f".to_string(), arguments: "{}".to_string() },
                }]),
                tool_call_id: None,
                name: None,
            },
            msg("assistant", json!("a2")),
        ];
        let merged = merge_consecutive_messages(messages);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_sanitize_invalid_tool_arguments() {
        let mut messages = vec![ChatMessage {
            role: "assistant".to_string(),
            content: None,
            reasoning_content: None,
            tool_calls: Some(vec![ToolCall {
                id: "tc1".to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction { name: "f".to_string(), arguments: "not json".to_string() },
            }]),
            tool_call_id: None,
            name: None,
        }];
        // Directly test the sanitization logic by replicating the loop
        for msg in &mut messages {
            if let Some(ref mut tcs) = msg.tool_calls {
                for tc in tcs {
                    if !tc.function.arguments.is_empty() {
                        if serde_json::from_str::<Value>(&tc.function.arguments).is_err() {
                            tc.function.arguments = "{}".to_string();
                        }
                    }
                }
            }
        }
        assert_eq!(messages[0].tool_calls.as_ref().unwrap()[0].function.arguments, "{}");
    }

    #[test]
    fn test_kimi_web_search_disables_thinking() {
        let req = ResponsesRequest {
            model: Some("kimi-k2".to_string()),
            input: json!("search"),
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: Some(vec![json!({"type": "web_search"})]),
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        };
        let result = convert_with_provider(&req, "kimi-k2", false, "kimi").unwrap();
        assert!(result.thinking.is_some());
        assert_eq!(result.thinking.unwrap()["type"], "disabled");
    }

    #[test]
    fn test_split_think_tags_basic() {
        let (text, reasoning) = split_think_tags("Hello <think>thinking</think> world");
        assert_eq!(text, "Hello  world");
        assert_eq!(reasoning, Some("thinking".to_string()));
    }

    #[test]
    fn test_split_think_tags_no_tags() {
        let (text, reasoning) = split_think_tags("Just text");
        assert_eq!(text, "Just text");
        assert_eq!(reasoning, None);
    }

    #[test]
    fn test_split_think_tags_empty_thinking() {
        let (text, reasoning) = split_think_tags("Hello <think>   </think> world");
        assert_eq!(text, "Hello  world");
        assert_eq!(reasoning, None);
    }

    #[test]
    fn test_map_role_developer_to_system() {
        assert_eq!(map_role("developer"), "system");
        assert_eq!(map_role("user"), "user");
        assert_eq!(map_role("assistant"), "assistant");
    }

    #[test]
    fn test_extract_content_string() {
        assert_eq!(extract_content(Some(&json!("hello"))), json!("hello"));
    }

    #[test]
    fn test_extract_content_array_text_parts() {
        let arr = json!([
            {"type": "input_text", "text": "hello"},
            {"type": "output_text", "text": " world"},
            {"type": "text", "text": "!"}
        ]);
        assert_eq!(extract_content(Some(&arr)), json!("hello world!"));
    }

    #[test]
    fn test_extract_content_array_no_text() {
        let arr = json!([{"type": "image", "url": "http://example.com"}]);
        assert_eq!(extract_content(Some(&arr)), json!("[{\"type\":\"image\",\"url\":\"http://example.com\"}]"));
    }

    #[test]
    fn test_extract_content_object_with_text() {
        let obj = json!({"text": "hello", "format": "plain"});
        assert_eq!(extract_content(Some(&obj)), json!("hello"));
    }

    #[test]
    fn test_extract_content_object_no_text() {
        let obj = json!({"format": "plain"});
        assert_eq!(extract_content(Some(&obj)), json!("{\"format\":\"plain\"}"));
    }

    #[test]
    fn test_convert_input_object() {
        let input = json!({"text": "hello object"});
        let result = convert_input(&input).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, Some(json!("hello object")));
    }

    #[test]
    fn test_convert_input_number() {
        let input = json!(42);
        let result = convert_input(&input).unwrap();
        assert_eq!(result[0].content, Some(json!("42")));
    }

    // ── Tests for fixes ──

    #[test]
    fn test_split_think_tags_multiple_blocks() {
        let (text, reasoning) = split_think_tags("<think>A</think> middle <think>B</think> end");
        assert_eq!(text, "middle  end");
        assert_eq!(reasoning, Some("A\n\nB".to_string()));
    }

    #[test]
    fn test_split_think_tags_unclosed() {
        let (text, reasoning) = split_think_tags("hello <think>unclosed");
        assert_eq!(text, "hello <think>unclosed");
        assert_eq!(reasoning, None);
    }

    #[test]
    fn test_split_think_tags_adjacent() {
        let (text, reasoning) = split_think_tags("<think>first</think><think>second</think>");
        assert_eq!(reasoning, Some("first\n\nsecond".to_string()));
        assert_eq!(text, "");
    }

    #[test]
    fn test_large_tool_output_not_truncated() {
        let big_output = "x".repeat(10000);
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!([
                {"type": "function_call", "call_id": "c1", "name": "read", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": big_output}
            ]),
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        let tool_msg = &result.messages[1];
        let content_str = tool_msg.content.as_ref().unwrap().as_str().unwrap();
        assert_eq!(content_str.len(), 10000, "Tool output should not be truncated");
    }

    #[test]
    fn test_chinese_tool_output_not_truncated() {
        let chinese_output = "中文".repeat(3000); // 6000 chars, ~18000 bytes
        let req = ResponsesRequest {
            model: Some("gpt-4".to_string()),
            input: json!([
                {"type": "function_call", "call_id": "c1", "name": "read", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": chinese_output}
            ]),
            ..Default::default()
        };
        let result = convert_with_provider(&req, "gpt-4", false, "").unwrap();
        let tool_msg = &result.messages[1];
        let content_str = tool_msg.content.as_ref().unwrap().as_str().unwrap();
        assert_eq!(content_str, chinese_output, "Chinese tool output should pass through intact");
    }
}
