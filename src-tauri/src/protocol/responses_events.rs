use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn next_seq() -> u64 { SEQ.fetch_add(1, Ordering::Relaxed) }

pub fn reset_sequence() { SEQ.store(0, Ordering::Relaxed); }

fn sse(event_type: &str, mut data: Value) -> String {
    data["sequence_number"] = json!(next_seq());
    format!("event: {event_type}\ndata: {}\n\n", data)
}

/// Full response envelope matching Codex protocol expectations.
fn build_envelope(response_id: &str, model: &str, status: &str) -> Value {
    json!({
        "id": response_id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": status,
        "model": model,
        "output": [],
        "parallel_tool_calls": true,
        "tool_choice": "auto",
        "tools": [],
        "temperature": 1.0,
        "top_p": 1.0,
        "max_output_tokens": null,
        "previous_response_id": null,
        "reasoning": null,
        "instructions": null,
        "text": {"format": {"type": "text"}},
        "truncation": "disabled",
        "metadata": {},
        "usage": null,
        "incomplete_details": null,
        "error": null,
    })
}

pub fn response_created(response_id: &str, model: &str) -> String {
    let envelope = build_envelope(response_id, model, "in_progress");
    sse("response.created", json!({"type": "response.created", "response": envelope}))
}

pub fn response_in_progress(response_id: &str, model: &str) -> String {
    let envelope = build_envelope(response_id, model, "in_progress");
    sse("response.in_progress", json!({"type": "response.in_progress", "response": envelope}))
}

pub fn output_item_added_message(item_id: &str, output_index: usize) -> String {
    sse("response.output_item.added", json!({
        "type": "response.output_item.added", "output_index": output_index,
        "item": { "id": item_id, "type": "message", "status": "in_progress", "role": "assistant", "content": [] }
    }))
}

pub fn content_part_added(item_id: &str, output_index: usize, content_index: usize) -> String {
    sse("response.content_part.added", json!({
        "type": "response.content_part.added",
        "item_id": item_id, "output_index": output_index, "content_index": content_index,
        "part": { "type": "output_text", "text": "", "annotations": [] }
    }))
}

pub fn output_text_delta(item_id: &str, output_index: usize, content_index: usize, delta: &str) -> String {
    sse("response.output_text.delta", json!({
        "type": "response.output_text.delta",
        "item_id": item_id, "output_index": output_index, "content_index": content_index,
        "delta": delta
    }))
}

pub fn output_text_done(item_id: &str, output_index: usize, content_index: usize, text: &str) -> String {
    sse("response.output_text.done", json!({
        "type": "response.output_text.done",
        "item_id": item_id, "output_index": output_index, "content_index": content_index,
        "text": text
    }))
}

pub fn content_part_done(item_id: &str, output_index: usize, content_index: usize, text: &str) -> String {
    sse("response.content_part.done", json!({
        "type": "response.content_part.done",
        "item_id": item_id, "output_index": output_index, "content_index": content_index,
        "part": { "type": "output_text", "text": text }
    }))
}

pub fn output_item_done_message(item_id: &str, output_index: usize, text: &str, reasoning_content: Option<&str>) -> String {
    output_item_done_message_with_annotations(item_id, output_index, text, reasoning_content, &[])
}

/// Same as `output_item_done_message` but embeds web-search / citation
/// annotations collected during streaming. `annotations` are pass-through —
/// each entry is a JSON object with provider-defined shape (url, title,
/// summary, publish_time, etc., per MiMo / OpenAI search-preview spec).
pub fn output_item_done_message_with_annotations(
    item_id: &str,
    output_index: usize,
    text: &str,
    reasoning_content: Option<&str>,
    annotations: &[Value],
) -> String {
    let mut item = json!({
        "id": item_id, "type": "message", "status": "completed", "role": "assistant",
        "content": [{ "type": "output_text", "text": text, "annotations": annotations }]
    });
    if let Some(rc) = reasoning_content { item["reasoning_content"] = json!(rc); }
    sse("response.output_item.done", json!({ "type": "response.output_item.done", "output_index": output_index, "item": item }))
}

/// Open a reasoning output_item placeholder. Emit this when the first reasoning
/// chunk arrives so downstream consumers (Codex IDE plugin, Claude Code TUI)
/// know an upcoming series of `response.reasoning_summary_text.delta` events
/// belongs to a real item — without this, deltas would arrive against an
/// unknown `item_id`.
pub fn output_item_added_reasoning(item_id: &str, output_index: usize) -> String {
    sse("response.output_item.added", json!({
        "type": "response.output_item.added",
        "output_index": output_index,
        "item": {
            "id": item_id,
            "type": "reasoning",
            "status": "in_progress",
            "summary": [],
        }
    }))
}

/// Incremental reasoning text chunk. Emitted as the upstream's thinking-mode
/// tokens arrive so the UI can render "thinking..." live instead of waiting
/// for the entire trace at finalize.
pub fn reasoning_summary_text_delta(
    item_id: &str,
    output_index: usize,
    summary_index: usize,
    delta: &str,
) -> String {
    sse("response.reasoning_summary_text.delta", json!({
        "type": "response.reasoning_summary_text.delta",
        "item_id": item_id,
        "output_index": output_index,
        "summary_index": summary_index,
        "delta": delta,
    }))
}

/// Close the streamed reasoning summary. Always paired with a preceding
/// `output_item_added_reasoning` and one or more `reasoning_summary_text_delta`.
/// Followed by `output_item_done_reasoning` at finalize.
pub fn reasoning_summary_text_done(
    item_id: &str,
    output_index: usize,
    summary_index: usize,
    text: &str,
) -> String {
    sse("response.reasoning_summary_text.done", json!({
        "type": "response.reasoning_summary_text.done",
        "item_id": item_id,
        "output_index": output_index,
        "summary_index": summary_index,
        "text": text,
    }))
}

/// Emit a `reasoning` output_item with the full reasoning trace pinned in
/// `encrypted_content`. Codex echoes this item verbatim in subsequent
/// requests' `input` array, letting the gateway re-inject the trace into
/// the assistant message that carries tool_calls — critical for MiMo /
/// DeepSeek thinking-mode multi-turn workflows where the upstream rejects
/// 400 if reasoning_content is missing on a turn that has tool_calls.
///
/// `summary_text` is what Codex's TUI renders inline. Keep it the same as
/// `encrypted_content` for now; future versions may truncate the summary
/// while keeping `encrypted_content` full-fidelity.
pub fn output_item_done_reasoning(
    item_id: &str,
    output_index: usize,
    reasoning_text: &str,
) -> String {
    output_item_done_reasoning_with_blocks(item_id, output_index, reasoning_text, &[])
}

/// 与 [`output_item_done_reasoning`] 相同，但额外把 Anthropic thinking 块
/// 的签名信息编码进 `encrypted_content`——下一轮 client 把整个 reasoning
/// 项原样回传，AgentGate 即可解码出含签名的 thinking 块回放给 Anthropic，
/// 满足 sig-chain 校验。当 `blocks` 为空时退化为旧行为（encrypted_content
/// = reasoning_text 纯文本）。
pub fn output_item_done_reasoning_with_blocks(
    item_id: &str,
    output_index: usize,
    reasoning_text: &str,
    blocks: &[crate::transform::thinking_blocks::ThinkingBlock],
) -> String {
    let encrypted_content = crate::transform::thinking_blocks::encode_for_encrypted_content(blocks)
        .unwrap_or_else(|| reasoning_text.to_string());
    let item = json!({
        "id": item_id,
        "type": "reasoning",
        "status": "completed",
        "summary": [{"type": "summary_text", "text": reasoning_text}],
        "encrypted_content": encrypted_content,
    });
    sse(
        "response.output_item.done",
        json!({"type": "response.output_item.done", "output_index": output_index, "item": item}),
    )
}

/// SSE event for a single web-search citation arriving mid-stream. Codex
/// supports rendering annotations on the active output_text item; emit one
/// of these per annotation as soon as the upstream chunk surfaces it.
pub fn output_text_annotation_added(
    item_id: &str,
    output_index: usize,
    content_index: usize,
    annotation_index: usize,
    annotation: &Value,
) -> String {
    sse("response.output_text.annotation.added", json!({
        "type": "response.output_text.annotation.added",
        "item_id": item_id,
        "output_index": output_index,
        "content_index": content_index,
        "annotation_index": annotation_index,
        "annotation": annotation,
    }))
}

pub fn function_call_added(item_id: &str, output_index: usize, call_id: &str, name: &str) -> String {
    sse("response.output_item.added", json!({
        "type": "response.output_item.added", "output_index": output_index,
        "item": { "id": item_id, "type": "function_call", "status": "in_progress", "call_id": call_id, "name": name, "arguments": "" }
    }))
}

pub fn function_call_arguments_delta(item_id: &str, output_index: usize, delta: &str) -> String {
    sse("response.function_call_arguments.delta", json!({
        "type": "response.function_call_arguments.delta", "item_id": item_id, "output_index": output_index, "delta": delta
    }))
}

pub fn function_call_arguments_done(item_id: &str, output_index: usize, arguments: &str) -> String {
    sse("response.function_call_arguments.done", json!({
        "type": "response.function_call_arguments.done", "item_id": item_id, "output_index": output_index, "arguments": arguments
    }))
}

pub fn function_call_done(item_id: &str, output_index: usize, call_id: &str, name: &str, arguments: &str, reasoning_content: Option<&str>) -> String {
    let mut item = json!({
        "id": item_id, "type": "function_call", "status": "completed",
        "call_id": call_id, "name": name, "arguments": arguments
    });
    if let Some(rc) = reasoning_content { item["reasoning_content"] = json!(rc); }
    sse("response.output_item.done", json!({ "type": "response.output_item.done", "output_index": output_index, "item": item }))
}

pub fn response_completed(response_id: &str, model: &str, usage: Option<&Value>) -> String {
    response_completed_with_stop_reason(response_id, model, usage, None)
}

/// 与 [`response_completed`] 相同，但根据上游 stop_reason / finish_reason 映射
/// 出 Responses 协议的 `status` + `incomplete_details`。`stop_reason` 接受三家
/// 任意一种字符串：
/// - Anthropic: `end_turn` / `max_tokens` / `stop_sequence` / `tool_use` / `refusal`
/// - OpenAI Chat: `stop` / `length` / `tool_calls` / `function_call` / `content_filter`
/// - Gemini: `STOP` / `MAX_TOKENS` / `SAFETY` 等
///
/// 未识别的 stop_reason 退化为 `status: completed`、`incomplete_details: null`，
/// 与不传任何 stop_reason 时的行为一致——稳妥兜底。
pub fn response_completed_with_stop_reason(
    response_id: &str,
    model: &str,
    usage: Option<&Value>,
    stop_reason: Option<&str>,
) -> String {
    let default_usage = json!({
        "input_tokens": 0, "output_tokens": 0, "total_tokens": 0,
        "input_tokens_details": { "cached_tokens": 0 },
        "output_tokens_details": { "reasoning_tokens": 0 }
    });
    let u = usage.unwrap_or(&default_usage);
    let (status, incomplete_details) = map_stop_reason(stop_reason);
    let mut envelope = build_envelope(response_id, model, status);
    envelope["usage"] = u.clone();
    envelope["incomplete_details"] = incomplete_details;
    sse("response.completed", json!({"type": "response.completed", "response": envelope}))
}

/// 三家 stop_reason → Responses (status, incomplete_details) 映射。
fn map_stop_reason(stop_reason: Option<&str>) -> (&'static str, Value) {
    let Some(reason) = stop_reason else {
        return ("completed", Value::Null);
    };
    match reason {
        // 正常完成
        "end_turn" | "stop" | "stop_sequence" | "tool_use" | "tool_calls"
        | "function_call" | "STOP" => ("completed", Value::Null),
        // 命中输出长度上限
        "max_tokens" | "length" | "MAX_TOKENS" => (
            "incomplete",
            json!({ "reason": "max_output_tokens" }),
        ),
        // 安全审查 / 内容过滤
        "content_filter" | "SAFETY" | "RECITATION" | "refusal" => (
            "incomplete",
            json!({ "reason": "content_filter" }),
        ),
        // 未识别——按完成处理但留个 reason 字段供 client 调试
        _ => ("completed", Value::Null),
    }
}

pub fn response_failed(response_id: &str, model: &str, error_msg: &str) -> String {
    let mut envelope = build_envelope(response_id, model, "failed");
    envelope["error"] = json!({"message": error_msg, "code": "upstream_error"});
    sse("response.failed", json!({"type": "response.failed", "response": envelope}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_created_format() {
        reset_sequence();
        let s = response_created("r1", "gpt-4");
        assert!(s.starts_with("event: response.created"));
        assert!(s.contains("\"id\":\"r1\""));
        assert!(s.contains("\"status\":\"in_progress\""));
        assert!(s.contains("\"sequence_number\":"));
    }

    #[test]
    fn output_text_delta_format() {
        reset_sequence();
        let _ = response_created("r1", "gpt-4"); // consumes a seq
        let s = output_text_delta("i1", 0, 0, "hello");
        assert!(s.starts_with("event: response.output_text.delta"));
        assert!(s.contains("\"delta\":\"hello\""));
        assert!(s.contains("\"sequence_number\":"));
    }

    #[test]
    fn response_completed_includes_usage() {
        reset_sequence();
        let usage = json!({"input_tokens": 10, "output_tokens": 20});
        let s = response_completed("r1", "gpt-4", Some(&usage));
        assert!(s.starts_with("event: response.completed"));
        assert!(s.contains("\"status\":\"completed\""));
        assert!(s.contains("10"));
    }

    #[test]
    fn map_stop_reason_completed_variants() {
        for r in ["end_turn", "stop", "stop_sequence", "tool_use", "tool_calls", "function_call", "STOP"] {
            let (status, det) = map_stop_reason(Some(r));
            assert_eq!(status, "completed", "{r}");
            assert!(det.is_null(), "{r}");
        }
    }

    #[test]
    fn map_stop_reason_max_tokens_three_ways() {
        for r in ["max_tokens", "length", "MAX_TOKENS"] {
            let (status, det) = map_stop_reason(Some(r));
            assert_eq!(status, "incomplete", "{r}");
            assert_eq!(det["reason"], "max_output_tokens", "{r}");
        }
    }

    #[test]
    fn map_stop_reason_content_filter() {
        for r in ["content_filter", "SAFETY", "RECITATION", "refusal"] {
            let (status, det) = map_stop_reason(Some(r));
            assert_eq!(status, "incomplete", "{r}");
            assert_eq!(det["reason"], "content_filter", "{r}");
        }
    }

    #[test]
    fn map_stop_reason_none_or_unknown_completes() {
        let (status, det) = map_stop_reason(None);
        assert_eq!(status, "completed");
        assert!(det.is_null());
        let (status, det) = map_stop_reason(Some("some_future_reason"));
        assert_eq!(status, "completed");
        assert!(det.is_null());
    }

    #[test]
    fn response_completed_with_max_tokens_status() {
        reset_sequence();
        let s = response_completed_with_stop_reason("r1", "claude", None, Some("max_tokens"));
        assert!(s.contains("\"status\":\"incomplete\""));
        assert!(s.contains("\"reason\":\"max_output_tokens\""));
    }

    #[test]
    fn response_failed_format() {
        reset_sequence();
        let s = response_failed("r1", "gpt-4", "rate limit");
        assert!(s.starts_with("event: response.failed"));
        assert!(s.contains("\"status\":\"failed\""));
        assert!(s.contains("rate limit"));
    }

    #[test]
    fn function_call_events_format() {
        reset_sequence();
        let s1 = function_call_added("i1", 0, "c1", "get_weather");
        assert!(s1.contains("\"type\":\"function_call\""));
        let s2 = function_call_arguments_delta("i1", 0, r#"{"city":"B""#);
        assert!(s2.contains("city"));
        assert!(s2.contains("B"));
        let s3 = function_call_arguments_done("i1", 0, r#"{"city":"Beijing"}"#);
        assert!(s3.contains("Beijing"));
        let s4 = function_call_done("i1", 0, "c1", "get_weather", r#"{"city":"Beijing"}"#, None);
        assert!(s4.contains("\"status\":\"completed\""));
    }

    #[test]
    fn output_item_done_with_reasoning() {
        reset_sequence();
        let s = output_item_done_message("i1", 0, "result", Some("<think>trace</think>"));
        assert!(s.contains("result"));
        assert!(s.contains("<think>trace</think>"));
    }

    #[test]
    fn annotation_added_event_format() {
        let ann = json!({
            "type": "url_citation",
            "url": "https://example.com",
            "title": "Example",
        });
        let s = output_text_annotation_added("msg_1", 0, 0, 2, &ann);
        assert!(s.starts_with("event: response.output_text.annotation.added"));
        assert!(s.contains("\"annotation_index\":2"));
        assert!(s.contains("https://example.com"));
    }

    #[test]
    fn output_item_done_with_annotations_embeds_citations() {
        let anns: Vec<serde_json::Value> = vec![json!({"url": "https://a.com"}), json!({"url": "https://b.com"})];
        let s = output_item_done_message_with_annotations("msg_1", 0, "answer", None, &anns);
        assert!(s.contains("https://a.com"));
        assert!(s.contains("https://b.com"));
    }

    #[test]
    fn sequence_number_increments() {
        reset_sequence();
        let s1 = response_created("r1", "m1");
        let s2 = response_in_progress("r1", "m1");
        let s3 = output_item_added_message("i1", 0);
        assert!(s1.contains("\"sequence_number\":"));
        assert!(s2.contains("\"sequence_number\":"));
        assert!(s3.contains("\"sequence_number\":"));
    }
}
