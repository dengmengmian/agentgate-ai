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

    // A 修复：force_high_effort 兜底。env AGENTGATE_FORCE_HIGH_EFFORT_PROVIDERS
    // 列出 provider_type 白名单（如 "mimo,deepseek,glm"），客户端没传 effort
    // 时自动注 high。与 mimo2codex 的 forceHighEffort 等价。
    //
    // 现象：MiMo / DeepSeek-thinking 这类模型在 effort=medium 下 reasoning 后
    // 偶发只输出一句"意图声明"就 stop（finish_reason=stop, output_tokens 很短）。
    // 升 high 后倾向给更完整的 verbose 回复。
    //
    // 仅在客户端 None 时生效——客户端显式传 low/medium 是用户意图，不覆盖。
    let reasoning_effort = match reasoning_effort {
        Some(e) => Some(e),
        None => maybe_force_high_effort(provider.provider_type()),
    };

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
        // C 修复：同时填 max_completion_tokens（新标准）。MiMo / DeepSeek-thinking
        // 等都认这个；老 provider 不识别会被忽略，无副作用。两者同时填覆盖最广。
        max_completion_tokens: req.max_output_tokens,
        thinking: None,
        stream_options,
        response_format,
        reasoning_effort,
        seed: req.seed.clone(),
        stop: req.stop.clone(),
        frequency_penalty: req.frequency_penalty,
        presence_penalty: req.presence_penalty,
        parallel_tool_calls: req.parallel_tool_calls,
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

                let raw_call_id = item
                    .get("call_id")
                    .and_then(|c| c.as_str())
                    .unwrap_or("call_unknown");
                let call_id = crate::transform::tool_calls::sanitize_call_id(raw_call_id).into_owned();
                let name = item
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let raw_arguments = item
                    .get("arguments")
                    .map(|a| {
                        if a.is_string() {
                            a.as_str().unwrap().to_string()
                        } else {
                            a.to_string()
                        }
                    })
                    .unwrap_or_default();
                // #4 修复：入站 history function_call.arguments 校验 JSON 合法性。
                // 客户端历史里可能带上一轮被截断的半截 args，原样发上游 → 严格
                // provider 400 "unexpected end of data"。salvage 成 {} 让对话能继续。
                // 与 sse.rs 出站方向对称。
                let arguments = crate::transform::tool_calls::salvage_tool_arguments(
                    &raw_arguments, &name, &call_id, None,
                );

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

                let sanitized_id = crate::transform::tool_calls::sanitize_call_id(call_id.unwrap()).into_owned();
                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: Some(output),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some(sanitized_id),
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
            // Chat Completions 协议本身不支持 tool 角色消息含图——这是协议限制
            // 不是 AgentGate 的 bug。但应用层应该知道：tool 返回的截图/图表
            // 等视觉信息丢了，模型只能看到"image omitted"占位符。warn 一行
            // 留给排查"为什么模型没看到我的图"。
            eprintln!(
                "[transform] {dropped_images} image attachment(s) dropped from tool output \
                 (Chat Completions protocol does not support images in tool messages)"
            );
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
                        // Responses 协议 detail 字段在 input_image 顶层（"auto"/"low"/"high"）；
                        // Chat 协议把它塞在 image_url 对象里。两种位置都收，保留进出参一致。
                        let detail = part.get("detail").and_then(|d| d.as_str())
                            .or_else(|| part.get("image_url").and_then(|u| u.get("detail")).and_then(|d| d.as_str()));
                        let url = part.get("image_url").and_then(|u| u.as_str())
                            .or_else(|| part.get("image_url").and_then(|u| u.get("url")).and_then(|u| u.as_str()));
                        if let Some(url) = url {
                            let mut image_url = serde_json::json!({"url": url});
                            if let Some(d) = detail {
                                image_url["detail"] = serde_json::json!(d);
                            }
                            parts_out.push(serde_json::json!({
                                "type": "image_url",
                                "image_url": image_url,
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

/// 读 env `AGENTGATE_FORCE_HIGH_EFFORT_PROVIDERS`（逗号分隔 provider_type 白名单），
/// 当前 provider 在列表内则返回 `Some("high")`，否则 None。
///
/// 用法示例：`AGENTGATE_FORCE_HIGH_EFFORT_PROVIDERS=mimo,deepseek`
fn maybe_force_high_effort(provider_type: &str) -> Option<String> {
    let raw = std::env::var("AGENTGATE_FORCE_HIGH_EFFORT_PROVIDERS").ok()?;
    let matches = raw.split(',').any(|s| s.trim().eq_ignore_ascii_case(provider_type));
    if matches { Some("high".to_string()) } else { None }
}

/// 流式 `<think>...</think>` 切分器（有状态）。
///
/// 用于：上游用 inline `<think>` 模式（MiMo / GLM-air / Skywork / 部分 Qwen）
/// 流式输出 content 时，chunk 边界可能落在标签中间（`<thi` / `</th`），无状态
/// 的 [`split_think_tags`] 会把半截标签当文本——下个 chunk 来时也认不出剩余。
///
/// 解法：跨 chunk **carry 半截标签**，凑齐才识别。`process_chunk` 返回当前能确
/// 定的 `(visible, reasoning)`，stream 结束时调一次 [`ThinkSplitter::flush`] 把
/// carry 残留按当前状态（in_think? 是 reasoning : 是 visible）emit 出去。
///
/// 对没有 inline `<think>` 的上游（独立 reasoning_content 字段或没 reasoning）
/// 完全透明：所有 content 当 visible 原样输出。
#[derive(Debug, Default)]
pub struct ThinkSplitter {
    /// 上一个 chunk 末尾残留的"可能是半截开始/结束标签"的字节。
    carry: String,
    /// 当前是否在 `<think>...</think>` 内部。
    in_think: bool,
}

impl ThinkSplitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 消费一段 chunk content，返回当前能确定的 `(visible_text, reasoning_extracted)`。
    /// 半截标签会 carry 到下一次 `process_chunk`，不会泄露到 visible。
    pub fn process_chunk(&mut self, chunk: &str) -> (String, Option<String>) {
        if chunk.is_empty() {
            return (String::new(), None);
        }

        // carry + chunk 拼接为本次工作 buffer
        let mut buffer = std::mem::take(&mut self.carry);
        buffer.push_str(chunk);

        let mut visible = String::new();
        let mut reasoning = String::new();
        let mut i: usize = 0;

        while i < buffer.len() {
            if !self.in_think {
                // 状态：在普通文本里，找 `<think>` 开始标签
                if let Some(rel_start) = buffer[i..].find("<think>") {
                    visible.push_str(&buffer[i..i + rel_start]);
                    i = i + rel_start + 7; // 跳过 "<think>"
                    self.in_think = true;
                    continue;
                }
                // 没找到完整开始标签。检查末尾是不是半截 `<think>` 前缀
                if let Some(carry_offset) = trailing_partial(&buffer[i..], "<think>") {
                    visible.push_str(&buffer[i..i + carry_offset]);
                    self.carry = buffer[i + carry_offset..].to_string();
                } else {
                    visible.push_str(&buffer[i..]);
                }
                break;
            } else {
                // 状态：在 think 内，找 `</think>` 结束标签
                if let Some(rel_end) = buffer[i..].find("</think>") {
                    reasoning.push_str(&buffer[i..i + rel_end]);
                    i = i + rel_end + 8; // 跳过 "</think>"
                    self.in_think = false;
                    continue;
                }
                if let Some(carry_offset) = trailing_partial(&buffer[i..], "</think>") {
                    reasoning.push_str(&buffer[i..i + carry_offset]);
                    self.carry = buffer[i + carry_offset..].to_string();
                } else {
                    reasoning.push_str(&buffer[i..]);
                }
                break;
            }
        }

        let reasoning_opt = if reasoning.is_empty() { None } else { Some(reasoning) };
        (visible, reasoning_opt)
    }

    /// 流结束时调一次。carry 残留按当前状态 emit：in_think → reasoning，否则 visible。
    /// 半截标签按字面文本处理（不再当标签 carry）。
    pub fn flush(&mut self) -> (String, Option<String>) {
        let carry = std::mem::take(&mut self.carry);
        if carry.is_empty() {
            return (String::new(), None);
        }
        if self.in_think {
            (String::new(), Some(carry))
        } else {
            (carry, None)
        }
    }
}

/// 检测 `s` 的末尾是否是 `target` 的非空前缀。返回 carry 起点（即末尾匹配前缀的起始 byte）。
/// 仅对 ASCII target 有效（`<think>` / `</think>` 都是 ASCII）。
fn trailing_partial(s: &str, target: &str) -> Option<usize> {
    let s_bytes = s.as_bytes();
    let t_bytes = target.as_bytes();
    // target 前缀长度从最长往最短试，找第一个匹配。
    // 最长 = min(target.len()-1, s.len())——完整匹配已在 .find() 阶段处理。
    let max_k = (t_bytes.len() - 1).min(s_bytes.len());
    for k in (1..=max_k).rev() {
        let tail = &s_bytes[s_bytes.len() - k..];
        let head = &t_bytes[..k];
        if tail == head {
            return Some(s_bytes.len() - k);
        }
    }
    None
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
    fn test_extract_content_input_image_detail_top_level_preserved() {
        // Responses 协议规范：detail 在 input_image 顶层
        let content = json!([
            {"type": "input_image", "image_url": "https://x/y.png", "detail": "high"}
        ]);
        let result = extract_content(Some(&content));
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["type"], "image_url");
        assert_eq!(arr[0]["image_url"]["url"], "https://x/y.png");
        assert_eq!(arr[0]["image_url"]["detail"], "high");
    }

    #[test]
    fn test_extract_content_input_image_detail_nested_preserved() {
        // 部分 client（Codex 等）把 detail 嵌进 image_url 对象里——也要保留
        let content = json!([
            {"type": "input_image", "image_url": {"url": "https://x/y.png", "detail": "low"}}
        ]);
        let result = extract_content(Some(&content));
        let arr = result.as_array().unwrap();
        assert_eq!(arr[0]["image_url"]["detail"], "low");
    }

    #[test]
    fn test_extract_content_input_image_no_detail_no_field() {
        // 不指定 detail 时不要往 image_url 对象里塞 detail: null
        let content = json!([
            {"type": "input_image", "image_url": "https://x/y.png"}
        ]);
        let result = extract_content(Some(&content));
        let arr = result.as_array().unwrap();
        assert!(arr[0]["image_url"].get("detail").is_none());
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

    // ── ThinkSplitter（带状态，跨 chunk 边界） ────────────────────

    #[test]
    fn think_splitter_single_chunk_full_tags() {
        let mut sp = ThinkSplitter::new();
        let (vis, rs) = sp.process_chunk("hello <think>thinking</think> world");
        assert_eq!(vis, "hello  world");
        assert_eq!(rs.as_deref(), Some("thinking"));
        let (vis2, rs2) = sp.flush();
        assert!(vis2.is_empty() && rs2.is_none());
    }

    #[test]
    fn think_splitter_split_open_tag_across_chunks() {
        // chunk1 末尾是半截 `<thi`，chunk2 接上 `nk>...</think>`
        let mut sp = ThinkSplitter::new();
        let (v1, r1) = sp.process_chunk("hello <thi");
        assert_eq!(v1, "hello ");
        assert!(r1.is_none());
        let (v2, r2) = sp.process_chunk("nk>secret</think> world");
        assert_eq!(v2, " world");
        assert_eq!(r2.as_deref(), Some("secret"));
    }

    #[test]
    fn think_splitter_split_close_tag_across_chunks() {
        let mut sp = ThinkSplitter::new();
        let (_, _) = sp.process_chunk("a<think>think");
        let (v2, r2) = sp.process_chunk("ing</th");
        assert_eq!(v2, "");
        assert_eq!(r2.as_deref(), Some("ing"));
        let (v3, r3) = sp.process_chunk("ink>tail");
        assert_eq!(v3, "tail");
        assert!(r3.is_none());
    }

    #[test]
    fn think_splitter_no_think_tag_passes_through() {
        let mut sp = ThinkSplitter::new();
        let (v1, r1) = sp.process_chunk("just plain text");
        assert_eq!(v1, "just plain text");
        assert!(r1.is_none());
    }

    #[test]
    fn think_splitter_flush_with_unclosed_think() {
        // 上游 chunk 里 `<think>` 开了头但 chunk 末尾正好是个半截 `</thi`——carry 留着。
        // stream 结束时 flush，in_think 状态下 carry 当 reasoning emit 出去。
        let mut sp = ThinkSplitter::new();
        let (v1, r1) = sp.process_chunk("text<think>reasoning</thi");
        assert_eq!(v1, "text");
        // chunk 内已确定的 reasoning 部分先返回（"reasoning"），半截 `</thi` 进 carry
        assert_eq!(r1.as_deref(), Some("reasoning"));
        let (v, r) = sp.flush();
        // flush 时仍 in_think，carry `</thi` 当 reasoning 字面文本输出
        assert!(v.is_empty());
        assert_eq!(r.as_deref(), Some("</thi"));
    }

    #[test]
    fn think_splitter_flush_with_unclosed_partial_open() {
        // carry 是 `<thi` 这种半截开始标签，stream 结束时按字面文本输出。
        let mut sp = ThinkSplitter::new();
        let (v1, _) = sp.process_chunk("hello <thi");
        assert_eq!(v1, "hello ");
        let (v2, r2) = sp.flush();
        assert_eq!(v2, "<thi");
        assert!(r2.is_none());
    }

    #[test]
    fn think_splitter_multiple_think_blocks() {
        let mut sp = ThinkSplitter::new();
        let (v1, r1) = sp.process_chunk("a<think>X</think>b<think>Y</think>c");
        assert_eq!(v1, "abc");
        // 两段 reasoning 分别返回（拼接到一起，因为同一 chunk 内）
        assert_eq!(r1.as_deref(), Some("XY"));
    }

    #[test]
    fn think_splitter_tiny_chunks_byte_by_byte() {
        // 极端 case：上游逐字节流出 "<think>"，确保 carry 累积正确
        let mut sp = ThinkSplitter::new();
        for ch in "<think>r</think>".chars() {
            let _ = sp.process_chunk(&ch.to_string());
        }
        let (v, r) = sp.flush();
        assert_eq!(v, "");
        assert!(r.is_none());
    }

    #[test]
    fn trailing_partial_finds_longest_prefix() {
        // 末尾 `<thi` 是 `<think>` 的前 4 字节
        assert_eq!(trailing_partial("hello <thi", "<think>"), Some(6));
        // 末尾 `<t` 是 `<think>` 的前 2 字节
        assert_eq!(trailing_partial("hi<t", "<think>"), Some(2));
        // 末尾不是任何前缀
        assert_eq!(trailing_partial("hello", "<think>"), None);
        // 整个字符串是 target 前缀（不含完整 target）
        assert_eq!(trailing_partial("<thi", "<think>"), Some(0));
    }
}
