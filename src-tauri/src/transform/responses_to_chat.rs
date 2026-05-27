use serde_json::Value;

use crate::errors::AppError;
use crate::protocol::chat_completions::{ChatCompletionsRequest, ChatMessage, ToolCall, ToolCallFunction};
use crate::protocol::openai_responses::ResponsesRequest;
use crate::transform::tool_calls;
use crate::transform::reasoning_store;
use super::providers::ProviderTransform;

pub fn convert_with_provider(
    req: &ResponsesRequest,
    model: &str,
    provider: &dyn ProviderTransform,
) -> Result<ChatCompletionsRequest, AppError> {
    convert_with_provider_matrix(req, model, provider, &Default::default())
}

/// Same as convert_with_provider but consults a per-model capability matrix
/// when emitting provider-builtin tools (e.g. MiMo's web_search). Production
/// gateway uses this so users can opt out of capabilities per model by
/// editing the matrix; tests can use the simpler 3-arg form.
pub fn convert_with_provider_matrix(
    req: &ResponsesRequest,
    model: &str,
    provider: &dyn ProviderTransform,
    matrix: &std::collections::HashMap<String, Vec<String>>,
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

    // 3. Convert tools (provider + matrix aware: Kimi $web_search builtin,
    //    MiMo web_search builtin gated by per-model capability matrix)
    let converted_tools = req.tools.as_ref().map(|t| {
        tool_calls::convert_tools_with_matrix(t, provider.clean_schemas(), provider.provider_type(), model, matrix)
    }).filter(|t| !t.is_empty());

    // 4. Convert tool_choice
    let tool_choice = req.tool_choice.as_ref().map(tool_calls::convert_tool_choice);

    // 5. Provider-specific message processing (e.g. DeepSeek: fix order, strip images, inject reasoning)
    messages = provider.process_messages(messages)?;

    // 6. Merge consecutive messages of the same role
    //    (some providers reject user→user or assistant→assistant sequences)
    messages = merge_consecutive_messages(messages);

    // 7. Sanitize tool call arguments (invalid JSON -> "{}")
    for msg in &mut messages {
        if let Some(ref mut tcs) = msg.tool_calls {
            for tc in tcs {
                if !tc.function.arguments.is_empty() {
                    if serde_json::from_str::<Value>(&tc.function.arguments).is_err() {
                        eprintln!("[warn] Invalid JSON in tool call '{}' arguments, replaced with {{}}: {}",
                            tc.function.name, tc.function.arguments);
                        tc.function.arguments = "{}".to_string();
                    }
                }
            }
        }
    }

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

    // Convert text.format → response_format
    let response_format = req.text.as_ref()
        .and_then(|t| t.get("format"))
        .and_then(|f| {
            let fmt_type = f.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match fmt_type {
                "json_object" => Some(serde_json::json!({"type": "json_object"})),
                "json_schema" => Some(f.clone()),
                _ => None,
            }
        });

    let mut chat_req = ChatCompletionsRequest {
        model: model.to_string(),
        messages,
        tools: converted_tools,
        tool_choice,
        stream: req.stream.unwrap_or(false),
        temperature: req.temperature,
        top_p: req.top_p,
        max_tokens: req.max_output_tokens,
        thinking: None,
        stream_options,
        response_format,
        reasoning_effort,
        seed: req.seed.clone(),
        stop: req.stop.clone(),
        frequency_penalty: req.frequency_penalty,
        presence_penalty: req.presence_penalty,
    };

    // 8. Provider-specific finalization (thinking, reasoning_effort, response_format overrides)
    let tools_clone = chat_req.tools.clone();
    provider.finalize_request(&mut chat_req, &tools_clone);

    Ok(chat_req)
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

                // Check for embedded tool_calls in content array (Codex multi-turn history format)
                let mut embedded_text = String::new();
                let mut embedded_tool_calls: Vec<ToolCall> = Vec::new();
                let mut has_embedded_tool_calls = false;
                if let Some(Value::Array(parts)) = item.get("content") {
                    for part in parts {
                        let pt = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        match pt {
                            "input_text" | "output_text" | "text" => {
                                if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                                    if !embedded_text.is_empty() {
                                        embedded_text.push('\n');
                                    }
                                    embedded_text.push_str(t);
                                }
                            }
                            "tool_call" => {
                                has_embedded_tool_calls = true;
                                embedded_tool_calls.push(ToolCall {
                                    id: part.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
                                    call_type: "function".to_string(),
                                    function: ToolCallFunction {
                                        name: part.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                                        arguments: part.get("arguments").map(|a| {
                                            if a.is_string() { a.as_str().unwrap().to_string() }
                                            else { a.to_string() }
                                        }).unwrap_or_default(),
                                    },
                                });
                            }
                            _ => {}
                        }
                    }
                }

                let content = if has_embedded_tool_calls {
                    Value::String(embedded_text)
                } else {
                    extract_content(item.get("content"))
                };

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
                    tool_calls: if embedded_tool_calls.is_empty() { None } else { Some(embedded_tool_calls) },
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
                    flatten_tool_output(o)
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
                // Reasoning round-trip priority:
                //   1. `encrypted_content` — full uncondensed trace pinned by the
                //      gateway on the previous response. Codex echoes it back
                //      verbatim, so this survives even when summary[] is truncated.
                //      Critical for MiMo / DeepSeek thinking-mode multi-turn tool
                //      calls (upstream 400s if the assistant turn that carried
                //      tool_calls is missing its reasoning_content).
                //   2. `content` — legacy field used by some clients.
                //   3. `summary[].text` — Codex's short summary, lossy fallback.
                if let Some(rc) = item.get("encrypted_content").and_then(|v| v.as_str()) {
                    if !rc.is_empty() {
                        pending_reasoning = Some(rc.to_string());
                    }
                }
                if pending_reasoning.is_none() {
                    if let Some(rc) = item.get("content").and_then(|v| v.as_str()) {
                        if !rc.is_empty() {
                            pending_reasoning = Some(rc.to_string());
                        }
                    }
                }
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

/// Flatten tool output to a string.
/// Chat Completions tool role only accepts `content: string`, but Codex Responses API
/// may send `output` as a ContentPart array (e.g. when a tool returns images + text).
/// We extract text parts, drop images with a placeholder notice.
pub fn flatten_tool_output(output: &Value) -> String {
    if let Some(s) = output.as_str() {
        return s.to_string();
    }
    if let Some(parts) = output.as_array() {
        let mut chunks = Vec::new();
        let mut dropped_images = 0u32;
        for part in parts {
            let pt = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match pt {
                "input_text" | "output_text" | "text" => {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            chunks.push(text.to_string());
                        }
                    }
                }
                "input_image" | "image_url" => {
                    dropped_images += 1;
                }
                _ => {}
            }
        }
        if dropped_images > 0 {
            let suffix = if dropped_images > 1 { "s" } else { "" };
            chunks.push(format!(
                "[{dropped_images} image attachment{suffix} omitted from tool output]"
            ));
        }
        if chunks.is_empty() {
            return output.to_string();
        }
        return chunks.join("");
    }
    output.to_string()
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
            let mut parts_out: Vec<Value> = Vec::new();
            let mut has_image = false;

            for part in arr {
                let pt = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match pt {
                    "input_text" | "output_text" | "text" => {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            parts_out.push(serde_json::json!({"type": "text", "text": text}));
                        }
                    }
                    "input_image" => {
                        // Convert Responses API input_image to Chat Completions image_url format
                        has_image = true;
                        if let Some(url) = part.get("image_url").and_then(|u| u.as_str()) {
                            parts_out.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": {"url": url}
                            }));
                        } else if let Some(b64) = part.get("image_url").and_then(|u| u.get("url")).and_then(|u| u.as_str()) {
                            parts_out.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": {"url": b64}
                            }));
                        }
                    }
                    "image_url" => {
                        // Already in Chat Completions format, pass through
                        has_image = true;
                        parts_out.push(part.clone());
                    }
                    _ => {
                        // Fallback: try "text" field
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            parts_out.push(serde_json::json!({"type": "text", "text": text}));
                        }
                    }
                }
            }

            if parts_out.is_empty() {
                Value::String(serde_json::to_string(arr).unwrap_or_default())
            } else if has_image {
                // Return multipart content array to preserve images
                Value::Array(parts_out)
            } else {
                // Text-only: join into a single string for compatibility
                let text = parts_out.iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                Value::String(text)
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

    // Only trim if think tags were actually found and extracted
    if !thinking_parts.is_empty() {
        let trimmed = remaining.trim().to_string();
        (trimmed, Some(thinking_parts.join("\n\n")))
    } else {
        (remaining, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::providers::{DefaultProvider, KimiProvider};
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
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
        assert!(convert_with_provider(&req, "gpt-4", &DefaultProvider).is_err());
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
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
        let result = convert_with_provider(&req, "kimi-k2", &KimiProvider).unwrap();
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
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
        let result = convert_with_provider(&req, "gpt-4", &DefaultProvider).unwrap();
        let tool_msg = &result.messages[1];
        let content_str = tool_msg.content.as_ref().unwrap().as_str().unwrap();
        assert_eq!(content_str, chinese_output, "Chinese tool output should pass through intact");
    }

    // ── split_think_tags whitespace preservation (critical for markdown rendering) ──

    #[test]
    fn test_split_think_tags_preserves_whitespace_no_tags() {
        // SSE delta chunks with leading/trailing newlines must be preserved
        // for markdown tables and headers to render correctly
        let (text, reasoning) = split_think_tags("\n\n## Header\n\n");
        assert_eq!(text, "\n\n## Header\n\n");
        assert_eq!(reasoning, None);
    }

    #[test]
    fn test_split_think_tags_preserves_table_newlines() {
        let chunk = "\n| col1 | col2 |\n| --- | --- |\n| a | b |\n";
        let (text, reasoning) = split_think_tags(chunk);
        assert_eq!(text, chunk, "Table newlines must be preserved for markdown rendering");
        assert_eq!(reasoning, None);
    }

    #[test]
    fn test_split_think_tags_preserves_leading_newline() {
        let (text, reasoning) = split_think_tags("\nhello");
        assert_eq!(text, "\nhello");
        assert_eq!(reasoning, None);
    }

    #[test]
    fn test_split_think_tags_preserves_trailing_newline() {
        let (text, reasoning) = split_think_tags("hello\n\n");
        assert_eq!(text, "hello\n\n");
        assert_eq!(reasoning, None);
    }

    #[test]
    fn test_split_think_tags_preserves_spaces_in_delta() {
        // A delta chunk that is just whitespace (common in streaming)
        let (text, reasoning) = split_think_tags("  ");
        assert_eq!(text, "  ");
        assert_eq!(reasoning, None);
    }

    #[test]
    fn test_split_think_tags_with_tags_does_trim() {
        // When think tags are extracted, trimming the remaining text is OK
        let (text, reasoning) = split_think_tags("  <think>thinking</think>  hello  ");
        assert_eq!(text, "hello");
        assert_eq!(reasoning, Some("thinking".to_string()));
    }

    // ── flatten_tool_output tests ──

    #[test]
    fn test_flatten_tool_output_string() {
        assert_eq!(flatten_tool_output(&json!("hello")), "hello");
    }

    #[test]
    fn test_flatten_tool_output_array_text_parts() {
        let output = json!([
            {"type": "output_text", "text": "result line 1"},
            {"type": "output_text", "text": "result line 2"}
        ]);
        assert_eq!(flatten_tool_output(&output), "result line 1result line 2");
    }

    #[test]
    fn test_flatten_tool_output_array_with_images() {
        let output = json!([
            {"type": "output_text", "text": "some text"},
            {"type": "input_image", "image_url": {"url": "data:image/png;base64,abc"}}
        ]);
        let result = flatten_tool_output(&output);
        assert!(result.contains("some text"));
        assert!(result.contains("[1 image attachment omitted from tool output]"));
    }

    #[test]
    fn test_flatten_tool_output_array_multiple_images() {
        let output = json!([
            {"type": "input_image", "image_url": {"url": "img1"}},
            {"type": "input_image", "image_url": {"url": "img2"}},
            {"type": "input_image", "image_url": {"url": "img3"}}
        ]);
        let result = flatten_tool_output(&output);
        assert!(result.contains("[3 image attachments omitted from tool output]"));
    }

    #[test]
    fn test_flatten_tool_output_non_string_non_array() {
        // Numbers, objects, etc. → JSON stringify
        assert_eq!(flatten_tool_output(&json!(42)), "42");
        assert_eq!(flatten_tool_output(&json!({"key": "val"})), "{\"key\":\"val\"}");
    }

    // ── extract_content image preservation tests ──

    #[test]
    fn test_extract_content_text_only() {
        let content = json!([
            {"type": "input_text", "text": "hello"},
            {"type": "text", "text": " world"}
        ]);
        let result = extract_content(Some(&content));
        // Text-only → joined string
        assert_eq!(result, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_extract_content_with_image_preserves_array() {
        let content = json!([
            {"type": "input_text", "text": "describe this"},
            {"type": "input_image", "image_url": "data:image/png;base64,abc123"}
        ]);
        let result = extract_content(Some(&content));
        // Has image → returns array
        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "describe this");
        assert_eq!(arr[1]["type"], "image_url");
        assert_eq!(arr[1]["image_url"]["url"], "data:image/png;base64,abc123");
    }

    #[test]
    fn test_extract_content_image_url_passthrough() {
        let content = json!([
            {"type": "text", "text": "hi"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,xyz"}}
        ]);
        let result = extract_content(Some(&content));
        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[1]["type"], "image_url");
    }

    #[test]
    fn test_extract_content_input_image_nested_url() {
        let content = json!([
            {"type": "input_image", "image_url": {"url": "data:image/jpeg;base64,abc"}}
        ]);
        let result = extract_content(Some(&content));
        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["image_url"]["url"], "data:image/jpeg;base64,abc");
    }

    #[test]
    fn test_extract_content_string_unchanged() {
        let result = extract_content(Some(&json!("plain text")));
        assert_eq!(result, Value::String("plain text".to_string()));
    }

    #[test]
    fn test_extract_content_none() {
        let result = extract_content(None);
        assert_eq!(result, Value::String(String::new()));
    }

    #[test]
    fn reasoning_encrypted_content_round_trips_to_assistant_message() {
        // Codex echoes a `reasoning` item with `encrypted_content` after a
        // prior turn; convert_input must pull that text and attach it to the
        // next assistant message as reasoning_content.
        let items = vec![
            json!({"type": "message", "role": "user", "content": "what's 2+2"}),
            json!({"type": "reasoning", "encrypted_content": "Let me think... 4."}),
            json!({"type": "message", "role": "assistant", "content": "4"}),
        ];
        let msgs = convert_input_array(&items).unwrap();
        // user, assistant(reasoning=...)
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].reasoning_content.as_deref(), Some("Let me think... 4."));
    }

    #[test]
    fn reasoning_encrypted_content_attaches_to_tool_call_turn() {
        // The critical case: tool_calls turn missing reasoning_content would
        // 400 on MiMo / DeepSeek thinking mode. encrypted_content from input
        // must land on the function_call turn.
        let items = vec![
            json!({"type": "message", "role": "user", "content": "search for X"}),
            json!({"type": "reasoning", "encrypted_content": "I should search."}),
            json!({
                "type": "function_call",
                "call_id": "c1",
                "name": "search",
                "arguments": "{\"q\":\"X\"}",
            }),
            json!({"type": "function_call_output", "call_id": "c1", "output": "found"}),
        ];
        let msgs = convert_input_array(&items).unwrap();
        // user + assistant(tool_calls, reasoning) + tool
        let assistant = msgs.iter().find(|m| m.role == "assistant").expect("assistant present");
        assert_eq!(assistant.reasoning_content.as_deref(), Some("I should search."));
        assert!(assistant.tool_calls.is_some());
    }

    #[test]
    fn reasoning_encrypted_content_takes_priority_over_summary() {
        let items = vec![
            json!({
                "type": "reasoning",
                "encrypted_content": "full trace",
                "summary": [{"type": "summary_text", "text": "short summary"}],
            }),
            json!({"type": "message", "role": "assistant", "content": "ok"}),
        ];
        let msgs = convert_input_array(&items).unwrap();
        assert_eq!(msgs[0].reasoning_content.as_deref(), Some("full trace"));
    }
}
