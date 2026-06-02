use serde_json::{json, Value};

/// Convert an Anthropic Messages non-stream response into a Chat Completions response.
///
/// 字段映射：
/// - `content[type=="text"]` → `choices[0].message.content`（拼接）
/// - `content[type=="thinking"]` → `choices[0].message.reasoning_content`
/// - `content[type=="tool_use"]` → `choices[0].message.tool_calls`
/// - `stop_reason` → `finish_reason`（与 `from_chat_response` 反向对称）
/// - `usage.input_tokens` → `usage.prompt_tokens`
/// - `usage.output_tokens` → `usage.completion_tokens`
/// - `usage.cache_read_input_tokens` → `usage.prompt_tokens_details.cached_tokens`
///   （同时保留 `cache_read_input_tokens` / `cache_creation_input_tokens` 顶层字段，
///   `request_logs::extract_cache_tokens` 两边都认）
pub fn convert(upstream: &Value, model: &str) -> Value {
    let mut text_buf = String::new();
    let mut thinking_buf = String::new();
    let mut tool_calls: Vec<Value> = Vec::new();

    if let Some(content) = upstream.get("content").and_then(|c| c.as_array()) {
        for block in content {
            let bt = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match bt {
                "text" => {
                    if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                        text_buf.push_str(t);
                    }
                }
                "thinking" => {
                    if let Some(t) = block.get("thinking").and_then(|t| t.as_str()) {
                        thinking_buf.push_str(t);
                    }
                }
                "tool_use" => {
                    let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let input = block.get("input").cloned().unwrap_or(json!({}));
                    let arguments = serde_json::to_string(&input).unwrap_or_else(|_| "{}".into());
                    tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": arguments,
                        },
                    }));
                }
                _ => {}
            }
        }
    }

    let stop_reason = upstream
        .get("stop_reason")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    let finish_reason = map_stop_reason(stop_reason, !tool_calls.is_empty());

    let mut message = json!({
        "role": "assistant",
        "content": if text_buf.is_empty() { Value::Null } else { Value::String(text_buf) },
    });
    if !thinking_buf.is_empty() {
        message["reasoning_content"] = json!(thinking_buf);
    }
    if !tool_calls.is_empty() {
        message["tool_calls"] = json!(tool_calls);
    }

    let resp_id = upstream
        .get("id")
        .and_then(|i| i.as_str())
        .map(String::from)
        .unwrap_or_else(|| {
            format!(
                "chatcmpl_{}",
                uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string()
            )
        });

    let resp_model = upstream
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or(model);

    json!({
        "id": resp_id,
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": resp_model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason,
        }],
        "usage": remap_usage(upstream.get("usage")),
    })
}

/// Anthropic stop_reason → Chat finish_reason（与 `map_finish_reason_to_stop_reason` 反向）。
fn map_stop_reason(stop_reason: &str, has_tool_use: bool) -> &'static str {
    match stop_reason {
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        "refusal" => "content_filter",
        "end_turn" | "stop_sequence" => "stop",
        // 缺省靠 tool_use 形态兜底
        "" if has_tool_use => "tool_calls",
        "" => "stop",
        _ => "stop",
    }
}

/// Anthropic usage → Chat usage：
/// - input_tokens / output_tokens 重命名为 prompt_tokens / completion_tokens
/// - cache_read_input_tokens 同时塞 prompt_tokens_details.cached_tokens（OpenAI 形态）
///   和顶层 cache_read_input_tokens（保留 Anthropic 形态）；extract_cache_tokens 两边都识别。
fn remap_usage(anthropic_usage: Option<&Value>) -> Value {
    let Some(u) = anthropic_usage else {
        return json!({"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0});
    };
    let input = u.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let output = u.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let cache_read = u.get("cache_read_input_tokens").and_then(|v| v.as_i64());
    let cache_creation = u
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_i64());

    let mut out = json!({
        "prompt_tokens": input,
        "completion_tokens": output,
        "total_tokens": input + output,
    });

    if let Some(c) = cache_read {
        out["prompt_tokens_details"] = json!({"cached_tokens": c});
        out["cache_read_input_tokens"] = json!(c);
    }
    if let Some(c) = cache_creation {
        out["cache_creation_input_tokens"] = json!(c);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_block_flattened_to_message_content() {
        let upstream = json!({
            "id": "msg_abc",
            "type": "message",
            "role": "assistant",
            "model": "claude-3",
            "content": [{"type": "text", "text": "Hello!"}],
            "stop_reason": "end_turn",
        });
        let resp = convert(&upstream, "claude-3");
        assert_eq!(resp["choices"][0]["message"]["content"], "Hello!");
        assert_eq!(resp["choices"][0]["finish_reason"], "stop");
        assert_eq!(resp["id"], "msg_abc");
    }

    #[test]
    fn tool_use_block_becomes_tool_calls() {
        let upstream = json!({
            "content": [
                {"type": "text", "text": "Let me search"},
                {"type": "tool_use", "id": "tu1", "name": "search", "input": {"q": "rust"}}
            ],
            "stop_reason": "tool_use",
        });
        let resp = convert(&upstream, "claude-3");
        assert_eq!(resp["choices"][0]["finish_reason"], "tool_calls");
        let tcs = resp["choices"][0]["message"]["tool_calls"]
            .as_array()
            .unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0]["id"], "tu1");
        assert_eq!(tcs[0]["type"], "function");
        assert_eq!(tcs[0]["function"]["name"], "search");
        assert_eq!(tcs[0]["function"]["arguments"], r#"{"q":"rust"}"#);
    }

    #[test]
    fn thinking_block_becomes_reasoning_content() {
        let upstream = json!({
            "content": [
                {"type": "thinking", "thinking": "Let me consider..."},
                {"type": "text", "text": "Answer: 42"}
            ],
            "stop_reason": "end_turn",
        });
        let resp = convert(&upstream, "claude-3");
        assert_eq!(resp["choices"][0]["message"]["content"], "Answer: 42");
        assert_eq!(
            resp["choices"][0]["message"]["reasoning_content"],
            "Let me consider..."
        );
    }

    #[test]
    fn stop_reason_maps_to_finish_reason() {
        for (stop, expected) in [
            ("end_turn", "stop"),
            ("stop_sequence", "stop"),
            ("max_tokens", "length"),
            ("tool_use", "tool_calls"),
            ("refusal", "content_filter"),
        ] {
            let upstream = json!({
                "content": [{"type": "text", "text": "x"}],
                "stop_reason": stop,
            });
            let resp = convert(&upstream, "claude-3");
            assert_eq!(
                resp["choices"][0]["finish_reason"], expected,
                "stop_reason={stop} should map to finish_reason={expected}"
            );
        }
    }

    #[test]
    fn usage_remapped_to_chat_field_names() {
        let upstream = json!({
            "content": [{"type": "text", "text": "x"}],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_read_input_tokens": 30,
                "cache_creation_input_tokens": 20,
            }
        });
        let resp = convert(&upstream, "claude-3");
        let usage = &resp["usage"];
        assert_eq!(usage["prompt_tokens"], 100);
        assert_eq!(usage["completion_tokens"], 50);
        assert_eq!(usage["total_tokens"], 150);
        assert_eq!(usage["prompt_tokens_details"]["cached_tokens"], 30);
        // 同时保留 Anthropic 字段，extract_cache_tokens 两边都识别
        assert_eq!(usage["cache_read_input_tokens"], 30);
        assert_eq!(usage["cache_creation_input_tokens"], 20);
    }

    #[test]
    fn empty_text_content_becomes_null() {
        let upstream = json!({
            "content": [],
            "stop_reason": "end_turn",
        });
        let resp = convert(&upstream, "claude-3");
        assert!(resp["choices"][0]["message"]["content"].is_null());
        assert_eq!(resp["choices"][0]["finish_reason"], "stop");
    }

    #[test]
    fn multiple_text_blocks_concatenated() {
        let upstream = json!({
            "content": [
                {"type": "text", "text": "Part 1. "},
                {"type": "text", "text": "Part 2."}
            ],
            "stop_reason": "end_turn",
        });
        let resp = convert(&upstream, "claude-3");
        assert_eq!(resp["choices"][0]["message"]["content"], "Part 1. Part 2.");
    }

    #[test]
    fn missing_usage_emits_zero_totals() {
        let upstream = json!({
            "content": [{"type": "text", "text": "x"}],
            "stop_reason": "end_turn",
        });
        let resp = convert(&upstream, "claude-3");
        assert_eq!(resp["usage"]["prompt_tokens"], 0);
        assert_eq!(resp["usage"]["completion_tokens"], 0);
    }

    #[test]
    fn model_from_upstream_takes_priority() {
        let upstream = json!({
            "content": [{"type": "text", "text": "x"}],
            "stop_reason": "end_turn",
            "model": "claude-3-opus-20240229",
        });
        let resp = convert(&upstream, "claude-3");
        assert_eq!(resp["model"], "claude-3-opus-20240229");
    }
}
