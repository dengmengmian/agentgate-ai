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
    let mut item = json!({
        "id": item_id, "type": "message", "status": "completed", "role": "assistant",
        "content": [{ "type": "output_text", "text": text, "annotations": [] }]
    });
    if let Some(rc) = reasoning_content { item["reasoning_content"] = json!(rc); }
    sse("response.output_item.done", json!({ "type": "response.output_item.done", "output_index": output_index, "item": item }))
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

/// Annotation event for web search citations.
pub fn annotation_added(item_id: &str, output_index: usize, content_index: usize, annotation: &Value) -> String {
    sse("response.output_text.annotation.added", json!({
        "type": "response.output_text.annotation.added",
        "item_id": item_id, "output_index": output_index, "content_index": content_index,
        "annotation": annotation,
    }))
}

pub fn response_completed(response_id: &str, model: &str, usage: Option<&Value>) -> String {
    let default_usage = json!({
        "input_tokens": 0, "output_tokens": 0, "total_tokens": 0,
        "input_tokens_details": { "cached_tokens": 0 },
        "output_tokens_details": { "reasoning_tokens": 0 }
    });
    let u = usage.unwrap_or(&default_usage);
    let mut envelope = build_envelope(response_id, model, "completed");
    envelope["usage"] = u.clone();
    sse("response.completed", json!({"type": "response.completed", "response": envelope}))
}

pub fn response_failed(response_id: &str, model: &str, error_msg: &str) -> String {
    let mut envelope = build_envelope(response_id, model, "failed");
    envelope["error"] = json!({"message": error_msg, "code": "upstream_error"});
    sse("response.failed", json!({"type": "response.failed", "response": envelope}))
}
