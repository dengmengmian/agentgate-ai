use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Anthropic Messages API request (loosely typed for compatibility).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MessagesRequest {
    pub model: Option<String>,
    pub messages: Vec<AnthropicMessage>,
    pub system: Option<Value>,
    pub max_tokens: Option<i64>,
    pub stream: Option<bool>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub tools: Option<Vec<Value>>,
    pub tool_choice: Option<Value>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Value,
}

/// Convert Anthropic Messages -> Chat Completions messages.
pub fn to_chat_messages(
    req: &MessagesRequest,
) -> Vec<crate::protocol::chat_completions::ChatMessage> {
    let mut messages = Vec::new();

    // System message
    if let Some(ref sys) = req.system {
        let text = match sys {
            Value::String(s) => s.clone(),
            Value::Array(arr) => {
                arr.iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => sys.to_string(),
        };
        if !text.is_empty() {
            messages.push(crate::protocol::chat_completions::ChatMessage {
                role: "system".to_string(),
                content: Some(Value::String(text)),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
    }

    // Convert each message
    for msg in &req.messages {
        match msg.role.as_str() {
            "user" => {
                // Anthropic 把 tool_result 块塞在 user role 的 content 数组里。
                // 转 Chat Completions 时必须拆成独立的 role:"tool" 消息（按出现
                // 顺序）+ 一条 role:"user"（如果还有 text/image 等非 tool_result 块）。
                // 原实现只 extract_text 把 tool_result 整段吞了——多轮 tool use
                // 整段消失，Claude Code 必坏。
                split_user_content(&msg.content, &mut messages);
            }
            "assistant" => {
                let (text, tool_calls) = extract_assistant_content(&msg.content);
                messages.push(crate::protocol::chat_completions::ChatMessage {
                    role: "assistant".to_string(),
                    content: if text.is_empty() { None } else { Some(Value::String(text)) },
                    reasoning_content: None,
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                    tool_call_id: None,
                    name: None,
                });
            }
            _ => {
                // tool_result -> tool message
                if let Some(tool_use_id) = msg.content.get("tool_use_id").and_then(|v| v.as_str()) {
                    let output = extract_text_content(&msg.content);
                    messages.push(crate::protocol::chat_completions::ChatMessage {
                        role: "tool".to_string(),
                        content: Some(Value::String(output)),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(tool_use_id.to_string()),
                        name: None,
                    });
                } else if msg.content.is_array() {
                    // Array of tool_result blocks
                    for block in msg.content.as_array().unwrap_or(&vec![]) {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                            let tid = block.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let output = block.get("content").map(|c| extract_text_content(c)).unwrap_or_default();
                            messages.push(crate::protocol::chat_completions::ChatMessage {
                                role: "tool".to_string(),
                                content: Some(Value::String(output)),
                                reasoning_content: None,
                                tool_calls: None,
                                tool_call_id: Some(tid),
                                name: None,
                            });
                        }
                    }
                }
            }
        }
    }

    messages
}

/// Anthropic user 消息内容数组里可能混着 tool_result + text + image 块。
/// 这函数按 Chat Completions 的形态拆成多条消息 push 进 messages：
/// 1. 每个 tool_result → 单独一条 role:"tool"
/// 2. 其余 text/image 块 → 一条 role:"user"（多媒体保留为 array content）
/// 3. 顺序：tool 先 user 后，匹配"工具结果先到、再发新请求"的对话语义
fn split_user_content(
    content: &Value,
    messages: &mut Vec<crate::protocol::chat_completions::ChatMessage>,
) {
    use crate::protocol::chat_completions::ChatMessage;

    // 字符串内容 → 单条 user 消息，最常见路径
    if let Some(s) = content.as_str() {
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: Some(Value::String(s.to_string())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        return;
    }

    let Some(arr) = content.as_array() else {
        // 非字符串非数组（罕见）—— fallback：JSON-stringify 整段当 user 内容
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: Some(Value::String(content.to_string())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
        return;
    };

    let mut user_parts: Vec<Value> = Vec::new();
    let mut user_text_buffer = String::new();
    let mut has_image = false;

    for block in arr {
        let bt = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match bt {
            "tool_result" => {
                let tid = block.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                // tool_result.content 可能是 string、可能是 [text+image] array。
                // Chat 协议的 tool 消息 content 只接 string——图片信息无奈丢弃，
                // 至少把 text 块保留下来。flatten_tool_output 已经实现这个语义。
                let output = block.get("content")
                    .map(crate::transform::responses_to_chat::flatten_tool_output)
                    .unwrap_or_default();
                messages.push(ChatMessage {
                    role: "tool".to_string(),
                    content: Some(Value::String(output)),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: Some(tid),
                    name: None,
                });
            }
            "text" | "input_text" => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    user_text_buffer.push_str(t);
                }
            }
            "image" => {
                // Anthropic image: { type:"image", source:{type:"base64", media_type, data} }
                // 转 Chat image_url: { type:"image_url", image_url:{url:"data:<mt>;base64,<data>"} }
                if let Some(src) = block.get("source") {
                    let mt = src.get("media_type").and_then(|m| m.as_str()).unwrap_or("image/png");
                    let data = src.get("data").and_then(|d| d.as_str()).unwrap_or("");
                    if !data.is_empty() {
                        if !user_text_buffer.is_empty() {
                            user_parts.push(serde_json::json!({"type":"text","text":user_text_buffer.clone()}));
                            user_text_buffer.clear();
                        }
                        user_parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": { "url": format!("data:{mt};base64,{data}") }
                        }));
                        has_image = true;
                    }
                }
            }
            _ => {
                // 未知 block 类型——把 text 字段（如果有）累进 text buffer
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    user_text_buffer.push_str(t);
                }
            }
        }
    }

    // 收尾：如果有累积的 text 或 image，emit user 消息
    if !user_text_buffer.is_empty() || has_image {
        let content = if has_image {
            if !user_text_buffer.is_empty() {
                user_parts.insert(0, serde_json::json!({"type":"text","text":user_text_buffer}));
            }
            Value::Array(user_parts)
        } else {
            Value::String(user_text_buffer)
        };
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: Some(content),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }
}

fn extract_text_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            arr.iter()
                .filter_map(|p| {
                    let t = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if t == "text" || t == "input_text" {
                        p.get("text").and_then(|t| t.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        }
        _ => content.to_string(),
    }
}

fn extract_assistant_content(content: &Value) -> (String, Vec<crate::protocol::chat_completions::ToolCall>) {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    match content {
        Value::String(s) => text = s.clone(),
        Value::Array(arr) => {
            for block in arr {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            text.push_str(t);
                        }
                    }
                    "tool_use" => {
                        let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let input = block.get("input").map(|v| v.to_string()).unwrap_or("{}".to_string());
                        tool_calls.push(crate::protocol::chat_completions::ToolCall {
                            id,
                            call_type: "function".to_string(),
                            function: crate::protocol::chat_completions::ToolCallFunction { name, arguments: input },
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    (text, tool_calls)
}

/// Convert Chat Completions response back to Anthropic Messages format.
pub fn from_chat_response(upstream: &Value, model: &str) -> Value {
    let mut content = Vec::<Value>::new();
    let mut finish_reason: Option<String> = None;

    if let Some(choices) = upstream.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            // finish_reason → Anthropic stop_reason 映射的源头；优先用上游的，
            // 不是只靠"内容里有没有 tool_use"猜（length / content_filter 会漏）
            if let Some(fr) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                if !fr.is_empty() {
                    finish_reason = Some(fr.to_string());
                }
            }
            if let Some(msg) = choice.get("message") {
                // 上游若返回 reasoning_content（DeepSeek-thinking / MiMo / o1 风格），
                // 包成 Anthropic thinking 块塞最前面。Anthropic content 顺序约束：
                // thinking → text → tool_use。
                if let Some(rc) = msg.get("reasoning_content").and_then(|r| r.as_str()) {
                    if !rc.is_empty() {
                        content.push(serde_json::json!({"type": "thinking", "thinking": rc}));
                    }
                }
                if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        content.push(serde_json::json!({"type": "text", "text": text}));
                    }
                }
                if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let raw_name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
                        // 对称 sanitize：与请求侧一致，避免上游 echo 出含 `.`/中文
                        // 的 tool name 让下游 Claude Code 拒收。
                        let name = crate::transform::tool_calls::sanitize_tool_name(raw_name);
                        let sanitized_id = crate::transform::tool_calls::sanitize_call_id(id);
                        let args_str = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()).unwrap_or("{}");
                        let input: Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                        content.push(serde_json::json!({
                            "type": "tool_use",
                            "id": sanitized_id.as_ref(),
                            "name": name.as_ref(),
                            "input": input
                        }));
                    }
                }
            }
        }
    }

    let stop_reason = map_finish_reason_to_stop_reason(&finish_reason, &content);

    serde_json::json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason,
        "usage": remap_usage_to_anthropic(upstream.get("usage")),
    })
}

/// Chat Completions finish_reason → Anthropic stop_reason。
/// 上游没提供 finish_reason 时退回到 content 推断（兜底，旧行为）。
fn map_finish_reason_to_stop_reason(fr: &Option<String>, content: &[Value]) -> &'static str {
    if let Some(fr) = fr {
        return match fr.as_str() {
            "length" => "max_tokens",
            "tool_calls" | "function_call" => "tool_use",
            "content_filter" => "refusal",
            "stop" => "end_turn",
            _ => "end_turn",
        };
    }
    // 没有 finish_reason 字段（极少数老上游）：靠 content 形状兜底
    if content.iter().any(|c| c.get("type").and_then(|t| t.as_str()) == Some("tool_use")) {
        "tool_use"
    } else {
        "end_turn"
    }
}

/// Chat Completions usage → Anthropic usage 字段重命名。
/// Anthropic client（Claude Code）期望 `input_tokens` / `output_tokens` /
/// `cache_creation_input_tokens` / `cache_read_input_tokens`——原样塞
/// `prompt_tokens` / `completion_tokens` 客户端会显示 token 为 0。
fn remap_usage_to_anthropic(chat_usage: Option<&Value>) -> Value {
    let Some(u) = chat_usage else {
        return serde_json::json!({});
    };
    let mut out = serde_json::json!({
        "input_tokens": u.get("prompt_tokens").or_else(|| u.get("input_tokens")).and_then(|v| v.as_i64()).unwrap_or(0),
        "output_tokens": u.get("completion_tokens").or_else(|| u.get("output_tokens")).and_then(|v| v.as_i64()).unwrap_or(0),
    });
    // Cache 字段：OpenAI 在 prompt_tokens_details.cached_tokens（只读）；
    // Anthropic 直接给 cache_read_input_tokens / cache_creation_input_tokens。
    let (cw, cr) = crate::storage::request_logs::extract_cache_tokens(u);
    if let Some(c) = cw {
        out["cache_creation_input_tokens"] = serde_json::json!(c);
    }
    if let Some(c) = cr {
        out["cache_read_input_tokens"] = serde_json::json!(c);
    }
    out
}

/// 把一条完整的（非流式的）Anthropic-shape response 合成一组 Anthropic SSE
/// 事件流。用在"client 用 /v1/messages 且 stream:true，但上游只支持
/// Chat Completions"的 fallback 路径上——AgentGate 做一次非流式上游调用、
/// 拿到完整 chat response、转 Anthropic shape、然后**合成**SSE 事件序列
/// 发给客户端。语义不严格"边收边发"（首字延迟等于上游完整耗时），但比
/// 给 Anthropic client 返一坨 JSON 强得多——它至少能正确解析。
///
/// 真正"边收边发"需要 chat-completions-stream → anthropic-SSE 增量转换器
/// （类似 sse_anthropic.rs 反向），那是更大的工程，留给未来。
pub fn synthesize_sse_events(response: &Value) -> Vec<String> {
    let mut events: Vec<String> = Vec::new();

    let id = response.get("id").and_then(|v| v.as_str()).unwrap_or("msg_synth");
    let model = response.get("model").and_then(|v| v.as_str()).unwrap_or("");
    let stop_reason = response.get("stop_reason").and_then(|v| v.as_str()).unwrap_or("end_turn");
    let usage = response.get("usage").cloned().unwrap_or(serde_json::json!({}));
    let empty_content = Vec::<Value>::new();
    let content_blocks: &Vec<Value> = response
        .get("content")
        .and_then(|c| c.as_array())
        .unwrap_or(&empty_content);

    // message_start —— Anthropic 流的第一帧，message 字段含完整 envelope 但
    // content 为空（实际内容靠后续 content_block_* 事件追加）。usage 这里
    // 先发 input_tokens，output_tokens 留给最后的 message_delta 报。
    let mut start_msg = serde_json::json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [],
        "stop_reason": Value::Null,
        "stop_sequence": Value::Null,
    });
    // 初始 usage 只暴露 input_tokens + cache 字段；output_tokens 初值给 1
    // （Anthropic 习惯性占位，与官方流一致）。
    let mut start_usage = serde_json::json!({
        "input_tokens": usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
        "output_tokens": 1,
    });
    if let Some(v) = usage.get("cache_creation_input_tokens") {
        start_usage["cache_creation_input_tokens"] = v.clone();
    }
    if let Some(v) = usage.get("cache_read_input_tokens") {
        start_usage["cache_read_input_tokens"] = v.clone();
    }
    start_msg["usage"] = start_usage;
    events.push(format!(
        "event: message_start\ndata: {}\n\n",
        serde_json::json!({"type": "message_start", "message": start_msg})
    ));

    // 逐 content block 发 start / delta / stop
    for (idx, block) in content_blocks.iter().enumerate() {
        let bt = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match bt {
            "text" => {
                let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
                events.push(format!(
                    "event: content_block_start\ndata: {}\n\n",
                    serde_json::json!({
                        "type": "content_block_start", "index": idx,
                        "content_block": {"type": "text", "text": ""}
                    })
                ));
                if !text.is_empty() {
                    events.push(format!(
                        "event: content_block_delta\ndata: {}\n\n",
                        serde_json::json!({
                            "type": "content_block_delta", "index": idx,
                            "delta": {"type": "text_delta", "text": text}
                        })
                    ));
                }
                events.push(format!(
                    "event: content_block_stop\ndata: {}\n\n",
                    serde_json::json!({"type": "content_block_stop", "index": idx})
                ));
            }
            "thinking" => {
                let thinking = block.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                let signature = block.get("signature").and_then(|s| s.as_str()).unwrap_or("");
                events.push(format!(
                    "event: content_block_start\ndata: {}\n\n",
                    serde_json::json!({
                        "type": "content_block_start", "index": idx,
                        "content_block": {"type": "thinking", "thinking": ""}
                    })
                ));
                if !thinking.is_empty() {
                    events.push(format!(
                        "event: content_block_delta\ndata: {}\n\n",
                        serde_json::json!({
                            "type": "content_block_delta", "index": idx,
                            "delta": {"type": "thinking_delta", "thinking": thinking}
                        })
                    ));
                }
                if !signature.is_empty() {
                    events.push(format!(
                        "event: content_block_delta\ndata: {}\n\n",
                        serde_json::json!({
                            "type": "content_block_delta", "index": idx,
                            "delta": {"type": "signature_delta", "signature": signature}
                        })
                    ));
                }
                events.push(format!(
                    "event: content_block_stop\ndata: {}\n\n",
                    serde_json::json!({"type": "content_block_stop", "index": idx})
                ));
            }
            "tool_use" => {
                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let empty = serde_json::json!({});
                let input = block.get("input").unwrap_or(&empty);
                events.push(format!(
                    "event: content_block_start\ndata: {}\n\n",
                    serde_json::json!({
                        "type": "content_block_start", "index": idx,
                        "content_block": {"type": "tool_use", "id": id, "name": name, "input": {}}
                    })
                ));
                // tool_use input 通过 input_json_delta 发字符串化的 JSON
                let input_json = serde_json::to_string(input).unwrap_or_else(|_| "{}".to_string());
                events.push(format!(
                    "event: content_block_delta\ndata: {}\n\n",
                    serde_json::json!({
                        "type": "content_block_delta", "index": idx,
                        "delta": {"type": "input_json_delta", "partial_json": input_json}
                    })
                ));
                events.push(format!(
                    "event: content_block_stop\ndata: {}\n\n",
                    serde_json::json!({"type": "content_block_stop", "index": idx})
                ));
            }
            _ => { /* 未知 block，跳过 */ }
        }
    }

    // message_delta —— 携带最终 stop_reason + output_tokens
    let out_tok = usage.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    events.push(format!(
        "event: message_delta\ndata: {}\n\n",
        serde_json::json!({
            "type": "message_delta",
            "delta": {"stop_reason": stop_reason, "stop_sequence": Value::Null},
            "usage": {"output_tokens": out_tok}
        })
    ));

    // message_stop —— 关流
    events.push(format!(
        "event: message_stop\ndata: {}\n\n",
        serde_json::json!({"type": "message_stop"})
    ));

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;


    #[test]
    fn test_to_chat_messages_with_string_system() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![],
            system: Some(json!("You are helpful")),
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, Some(json!("You are helpful")));
    }

    #[test]
    fn test_to_chat_messages_with_array_system() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![],
            system: Some(json!([
                {"type": "text", "text": "Part 1"},
                {"type": "text", "text": "Part 2"}
            ])),
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages[0].content, Some(json!("Part 1\nPart 2")));
    }

    #[test]
    fn test_to_chat_messages_user_message() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: json!("hello"),
            }],
            system: None,
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, Some(json!("hello")));
    }

    #[test]
    fn test_to_chat_messages_assistant_with_tool_use() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![AnthropicMessage {
                role: "assistant".to_string(),
                content: json!([
                    {"type": "text", "text": "Let me check"},
                    {"type": "tool_use", "id": "tu1", "name": "search", "input": {"q": "test"}}
                ]),
            }],
            system: None,
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "assistant");
        assert_eq!(messages[0].content, Some(json!("Let me check")));
        assert!(messages[0].tool_calls.is_some());
        let tcs = messages[0].tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "tu1");
        assert_eq!(tcs[0].function.name, "search");
    }

    #[test]
    fn test_to_chat_messages_tool_result_non_user_role() {
        // Non-user/non-assistant role with array of tool_result blocks
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![AnthropicMessage {
                role: "tool".to_string(),
                content: json!([{"type": "tool_result", "tool_use_id": "tu1", "content": "result data"}]),
            }],
            system: None,
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "tool");
        assert_eq!(messages[0].tool_call_id, Some("tu1".to_string()));
        assert_eq!(messages[0].content, Some(json!("result data")));
    }

    #[test]
    fn test_to_chat_messages_tool_result_array_non_user_role() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![AnthropicMessage {
                role: "tool".to_string(),
                content: json!([
                    {"type": "tool_result", "tool_use_id": "tu1", "content": "r1"},
                    {"type": "tool_result", "tool_use_id": "tu2", "content": "r2"}
                ]),
            }],
            system: None,
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].tool_call_id, Some("tu1".to_string()));
        assert_eq!(messages[1].tool_call_id, Some("tu2".to_string()));
    }

    #[test]
    fn test_from_chat_response_text_only() {
        let upstream = json!({
            "choices": [{"message": {"content": "Hello!"}}],
            "usage": {"input_tokens": 10}
        });
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["role"], "assistant");
        assert_eq!(resp["model"], "claude-3");
        assert_eq!(resp["stop_reason"], "end_turn");
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Hello!");
    }

    #[test]
    fn test_from_chat_response_with_tool_calls() {
        let upstream = json!({
            "choices": [{"message": {
                "content": "",
                "tool_calls": [
                    {"id": "tc1", "function": {"name": "search", "arguments": "{\"q\":\"hi\"}"}}
                ]
            }}]
        });
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["stop_reason"], "tool_use");
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["name"], "search");
    }

    #[test]
    fn test_from_chat_response_empty_choices() {
        let upstream = json!({"choices": []});
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["role"], "assistant");
        let content = resp["content"].as_array().unwrap();
        assert!(content.is_empty());
        assert_eq!(resp["stop_reason"], "end_turn");
    }

    #[test]
    fn user_message_with_tool_result_splits_into_tool_messages() {
        // Claude Code 典型形态：user 消息内嵌多个 tool_result + 后续文本。
        // 转 Chat 必须拆成多条 tool + 一条 user，否则 tool 结果整段丢失。
        let req = MessagesRequest {
            model: Some("claude-3".into()),
            messages: vec![AnthropicMessage {
                role: "user".into(),
                content: json!([
                    {"type": "tool_result", "tool_use_id": "tu1", "content": "result 1"},
                    {"type": "tool_result", "tool_use_id": "tu2", "content": "result 2"},
                    {"type": "text", "text": "Now what?"}
                ]),
            }],
            system: None, max_tokens: None, stream: None, temperature: None, top_p: None,
            tools: None, tool_choice: None, extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "tool");
        assert_eq!(messages[0].tool_call_id.as_deref(), Some("tu1"));
        assert_eq!(messages[1].role, "tool");
        assert_eq!(messages[1].tool_call_id.as_deref(), Some("tu2"));
        assert_eq!(messages[2].role, "user");
        assert_eq!(messages[2].content, Some(json!("Now what?")));
    }

    #[test]
    fn user_message_with_only_tool_results_emits_no_trailing_user() {
        // 全是 tool_result 没文字时，只发 tool 消息，不要硬塞一条空 user。
        let req = MessagesRequest {
            model: Some("claude-3".into()),
            messages: vec![AnthropicMessage {
                role: "user".into(),
                content: json!([
                    {"type": "tool_result", "tool_use_id": "tu1", "content": "ok"}
                ]),
            }],
            system: None, max_tokens: None, stream: None, temperature: None, top_p: None,
            tools: None, tool_choice: None, extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "tool");
    }

    #[test]
    fn user_message_with_image_preserves_multipart() {
        let req = MessagesRequest {
            model: Some("claude-3".into()),
            messages: vec![AnthropicMessage {
                role: "user".into(),
                content: json!([
                    {"type": "text", "text": "describe"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "abc123"}}
                ]),
            }],
            system: None, max_tokens: None, stream: None, temperature: None, top_p: None,
            tools: None, tool_choice: None, extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        let content = messages[0].content.as_ref().unwrap();
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[1]["type"], "image_url");
        assert!(arr[1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/png;base64,"));
    }

    #[test]
    fn from_chat_response_maps_finish_reason_length_to_max_tokens() {
        let upstream = json!({
            "choices": [{"message": {"content": "truncated..."}, "finish_reason": "length"}]
        });
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["stop_reason"], "max_tokens");
    }

    #[test]
    fn from_chat_response_maps_finish_reason_content_filter_to_refusal() {
        let upstream = json!({
            "choices": [{"message": {"content": ""}, "finish_reason": "content_filter"}]
        });
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["stop_reason"], "refusal");
    }

    #[test]
    fn from_chat_response_remaps_usage_to_anthropic_names() {
        let upstream = json!({
            "choices": [{"message": {"content": "hi"}, "finish_reason": "stop"}],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "prompt_tokens_details": {"cached_tokens": 30}
            }
        });
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["usage"]["input_tokens"], 100);
        assert_eq!(resp["usage"]["output_tokens"], 50);
        assert_eq!(resp["usage"]["cache_read_input_tokens"], 30);
        // 没有 cache_creation 字段时不应出现
        assert!(resp["usage"].get("cache_creation_input_tokens").is_none());
    }

    #[test]
    fn synthesize_sse_events_emits_full_lifecycle_for_text_response() {
        let resp = json!({
            "id": "msg_x", "type": "message", "role": "assistant",
            "model": "claude-3", "content": [{"type": "text", "text": "Hello!"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 5, "output_tokens": 3}
        });
        let events = synthesize_sse_events(&resp);
        let joined = events.join("");
        // 必备帧顺序
        assert!(joined.contains("event: message_start"));
        assert!(joined.contains("\"input_tokens\":5"));
        assert!(joined.contains("event: content_block_start"));
        assert!(joined.contains("event: content_block_delta"));
        assert!(joined.contains("\"text\":\"Hello!\""));
        assert!(joined.contains("event: content_block_stop"));
        assert!(joined.contains("event: message_delta"));
        assert!(joined.contains("\"stop_reason\":\"end_turn\""));
        assert!(joined.contains("\"output_tokens\":3"));
        assert!(joined.contains("event: message_stop"));
    }

    #[test]
    fn synthesize_sse_events_handles_tool_use_block() {
        let resp = json!({
            "id": "msg_x", "model": "claude-3",
            "content": [
                {"type": "text", "text": "Calling search"},
                {"type": "tool_use", "id": "tu1", "name": "search", "input": {"q": "rust"}}
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 8}
        });
        let events = synthesize_sse_events(&resp);
        let joined = events.join("");
        assert!(joined.contains("\"type\":\"tool_use\""));
        assert!(joined.contains("\"input_json_delta\""));
        // input 经 partial_json 字符串化后，内部 JSON 的引号被 SSE 外层 JSON
        // 转义成 \"——找转义后的形式才对。
        assert!(joined.contains("\\\"q\\\":\\\"rust\\\""));
        assert!(joined.contains("\"stop_reason\":\"tool_use\""));
    }

    #[test]
    fn synthesize_sse_events_propagates_thinking_signature() {
        let resp = json!({
            "id": "msg_x", "model": "claude-3",
            "content": [
                {"type": "thinking", "thinking": "step 1...", "signature": "sig_abc"},
                {"type": "text", "text": "answer"}
            ],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 5, "output_tokens": 5}
        });
        let events = synthesize_sse_events(&resp);
        let joined = events.join("");
        assert!(joined.contains("\"type\":\"thinking_delta\""));
        assert!(joined.contains("\"type\":\"signature_delta\""));
        assert!(joined.contains("\"signature\":\"sig_abc\""));
    }

    #[test]
    fn from_chat_response_promotes_reasoning_content_to_thinking_block() {
        // DeepSeek-thinking / MiMo 上游返 reasoning_content，应包成 thinking
        // 块放在 content 数组最前面（Anthropic 顺序约束）。
        let upstream = json!({
            "choices": [{"message": {
                "content": "The answer is 42",
                "reasoning_content": "Let me think step by step..."
            }, "finish_reason": "stop"}]
        });
        let resp = from_chat_response(&upstream, "claude-3");
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "thinking");
        assert_eq!(content[0]["thinking"], "Let me think step by step...");
        assert_eq!(content[1]["type"], "text");
    }
}
