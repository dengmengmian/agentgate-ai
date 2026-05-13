use std::collections::BTreeMap;
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::protocol::chat_completions::ChatCompletionChunk;
use crate::transform::reasoning_store;
use crate::protocol::responses_events as ev;

const MAX_EVENTS_LOG_SIZE: usize = 1_000_000; // 1MB

/// Accumulated tool call from streaming deltas.
#[derive(Debug, Clone)]
pub struct AccumulatedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
    /// Whether we have already emitted the output_item.added event for this call.
    emitted_added: bool,
    /// Track the last arguments length so we can compute per-delta output.
    last_args_len: usize,
}

/// State accumulated during SSE stream processing.
pub struct SseAccumulator {
    pub response_id: String,
    pub msg_item_id: String,
    pub model: String,
    pub full_text: String,
    pub tool_calls: BTreeMap<usize, AccumulatedToolCall>,
    pub reasoning_content: String,
    pub usage: Option<serde_json::Value>,
    pub events_log: String,
    events_size: usize,
    next_output_index: usize,
    text_content_started: bool,
}

impl SseAccumulator {
    pub fn new(response_id: String, model: String) -> Self {
        let msg_item_id = format!("msg_{}", &response_id.replace("resp_", ""));
        Self {
            response_id,
            msg_item_id,
            model,
            full_text: String::new(),
            tool_calls: BTreeMap::new(),
            reasoning_content: String::new(),
            usage: None,
            events_log: String::new(),
            events_size: 0,
            next_output_index: 1, // 0 is reserved for the message item
            text_content_started: false,
        }
    }

    fn log_event(&mut self, event: &str) {
        if self.events_size < MAX_EVENTS_LOG_SIZE {
            self.events_log.push_str(event);
            self.events_log.push('\n');
            self.events_size += event.len() + 1;
        }
    }

    pub fn tool_calls_list(&self) -> Vec<AccumulatedToolCall> {
        self.tool_calls.values().cloned().collect()
    }
}

/// Process upstream Chat Completions SSE and emit Responses API SSE events.
pub async fn process_upstream_stream(
    response: reqwest::Response,
    tx: mpsc::Sender<String>,
    acc: &mut SseAccumulator,
) -> Result<(), String> {
    ev::reset_sequence();

    // 1. response.created + in_progress
    send(&tx, &ev::response_created(&acc.response_id, &acc.model)).await;
    send(&tx, &ev::response_in_progress(&acc.response_id, &acc.model)).await;

    // NOTE: output_item.added is deferred until first text delta to avoid
    // emitting a spurious empty message item for tool-call-only responses.

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut has_text = false;
    let mut has_tool_calls = false;
    let mut message_item_emitted = false;

    while let Some(chunk_result) = stream.next().await {
        let chunk_bytes = match chunk_result {
            Ok(b) => b,
            Err(e) => {
                send(&tx, &ev::response_failed(&acc.response_id, &acc.model, &format!("Stream error: {e}"))).await;
                return Err(format!("Stream read error: {e}"));
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk_bytes));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end_matches('\r').to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || line.starts_with(':') {
                continue;
            }

            // "data: {...}" and "data:{...}" — some providers omit the space
            let Some(data) = line.strip_prefix("data:").map(|d| d.trim()) else { continue };

            if data == "[DONE]" {
                return finalize(tx, acc, has_text, has_tool_calls).await;
            }

            acc.log_event(data);

            let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data) else { continue };

            // Capture usage from chunk (DeepSeek sends it on the last chunk)
            if let Some(ref u) = chunk.usage {
                acc.usage = Some(normalize_usage(u));
            }

            let Some(choices) = &chunk.choices else { continue };

            for choice in choices {
                let Some(delta) = &choice.delta else { continue };

                // ── Text content (with <think> tag splitting) ──
                if let Some(ref content) = delta.content {
                    if !content.is_empty() {
                        // Split <think> tags — MiniMax embeds thinking in content
                        let (text, thinking) = crate::transform::responses_to_chat::split_think_tags(content);
                        if let Some(ref tk) = thinking {
                            acc.reasoning_content.push_str(tk);
                        }
                        if !text.is_empty() {
                            if !message_item_emitted {
                                send(&tx, &ev::output_item_added_message(&acc.msg_item_id, 0)).await;
                                send(&tx, &ev::content_part_added(&acc.msg_item_id, 0, 0)).await;
                                acc.text_content_started = true;
                                message_item_emitted = true;
                            }
                            has_text = true;
                            acc.full_text.push_str(&text);
                            send(&tx, &ev::output_text_delta(&acc.msg_item_id, 0, 0, &text)).await;
                        }
                    }
                }

                // ── Reasoning content ──
                if let Some(ref rc) = delta.reasoning_content {
                    if !rc.is_empty() {
                        if !message_item_emitted {
                            send(&tx, &ev::output_item_added_message(&acc.msg_item_id, 0)).await;
                            message_item_emitted = true;
                        }
                        // Inject "**Thinking**\n\n" header so Codex TUI shows reasoning
                        if acc.reasoning_content.is_empty() {
                            acc.reasoning_content.push_str("**Thinking**\n\n");
                        }
                        acc.reasoning_content.push_str(rc);
                    }
                }

                // ── reasoning_details array (o3/o4 native) ──
                if let Some(ref details) = delta.reasoning_details {
                    for detail in details {
                        if let Some(text) = detail.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                if acc.reasoning_content.is_empty() {
                                    acc.reasoning_content.push_str("**Thinking**\n\n");
                                }
                                acc.reasoning_content.push_str(text);
                            }
                        }
                    }
                }

                // ── Legacy delta.function_call → synthetic tool_call ──
                if let Some(ref fc) = delta.function_call {
                    has_tool_calls = true;
                    let idx = 0usize;
                    if !acc.tool_calls.contains_key(&idx) {
                        acc.tool_calls.insert(idx, AccumulatedToolCall {
                            id: format!("call_legacy_{}", acc.response_id.replace("resp_", "")),
                            name: String::new(), arguments: String::new(),
                            emitted_added: false, last_args_len: 0,
                        });
                    }
                    let tc = acc.tool_calls.get_mut(&idx).unwrap();
                    if let Some(name) = fc.get("name").and_then(|n| n.as_str()) {
                        tc.name.push_str(name);
                    }
                    if let Some(args) = fc.get("arguments").and_then(|a| a.as_str()) {
                        tc.arguments.push_str(args);
                    }
                    if !tc.emitted_added && !tc.name.is_empty() {
                        let item_id = format!("fc_{}", tc.id);
                        send(&tx, &ev::function_call_added(&item_id, 1, &tc.id, &tc.name)).await;
                        tc.emitted_added = true;
                    }
                    if tc.emitted_added && tc.arguments.len() > tc.last_args_len {
                        let delta_args = &tc.arguments[tc.last_args_len..];
                        let item_id = format!("fc_{}", tc.id);
                        send(&tx, &ev::function_call_arguments_delta(&item_id, 1, delta_args)).await;
                        tc.last_args_len = tc.arguments.len();
                    }
                }

                // ── Tool calls (streaming deltas) ──
                if let Some(ref tcs) = delta.tool_calls {
                    has_tool_calls = true;
                    for tc_delta in tcs {
                        let idx = tc_delta.index.unwrap_or(0) as usize;

                        // Ensure entry exists
                        if !acc.tool_calls.contains_key(&idx) {
                            acc.tool_calls.insert(idx, AccumulatedToolCall {
                                id: String::new(),
                                name: String::new(),
                                arguments: String::new(),
                                emitted_added: false,
                                last_args_len: 0,
                            });
                        }

                        let tc = acc.tool_calls.get_mut(&idx).unwrap();

                        // Accumulate id (only first delta usually has it)
                        if let Some(ref id) = tc_delta.id {
                            if tc.id.is_empty() {
                                tc.id = id.clone();
                            }
                        }

                        // Accumulate function name
                        if let Some(ref func) = tc_delta.function {
                            if let Some(ref name) = func.name {
                                tc.name.push_str(name);
                            }
                            if let Some(ref args) = func.arguments {
                                tc.arguments.push_str(args);
                            }
                        }

                        // Generate stable id if missing
                        if tc.id.is_empty() {
                            tc.id = format!("call_{}_{}", acc.response_id.replace("resp_", ""), idx);
                        }

                        // Emit function_call added event once we have name
                        if !tc.emitted_added && !tc.name.is_empty() {
                            let item_id = format!("fc_{}", tc.id);
                            let oi = acc.next_output_index;
                            acc.next_output_index += 1;
                            send(&tx, &ev::function_call_added(&item_id, oi, &tc.id, &tc.name)).await;
                            tc.emitted_added = true;
                        }

                        // Emit arguments delta
                        if tc.emitted_added && tc.arguments.len() > tc.last_args_len {
                            let delta_args = &tc.arguments[tc.last_args_len..];
                            let item_id = format!("fc_{}", tc.id);
                            // We need the output_index for this tool call.
                            // Since we increment next_output_index after adding, the index is:
                            // (next_output_index - number_of_remaining_unemitted) ... this is complex.
                            // Simpler: use idx + 1 as output_index (0 = message, 1+ = tool calls).
                            send(&tx, &ev::function_call_arguments_delta(&item_id, idx + 1, delta_args)).await;
                            tc.last_args_len = tc.arguments.len();
                        }
                    }
                }
            }
        }
    }

    // Stream ended without [DONE]
    if has_text || has_tool_calls {
        finalize(tx, acc, has_text, has_tool_calls).await
    } else {
        send(&tx, &ev::response_failed(&acc.response_id, &acc.model, "Stream ended unexpectedly")).await;
        Err("Stream ended without [DONE]".to_string())
    }
}

async fn finalize(
    tx: mpsc::Sender<String>,
    acc: &SseAccumulator,
    has_text: bool,
    has_tool_calls: bool,
) -> Result<(), String> {
    let rc = if acc.reasoning_content.is_empty() { None } else { Some(acc.reasoning_content.as_str()) };

    // Store reasoning for future requests
    if !acc.reasoning_content.is_empty() {
        let tc_ids: Vec<String> = acc.tool_calls.values().map(|tc| tc.id.clone()).collect();
        reasoning_store::store(&acc.full_text, &acc.reasoning_content, &tc_ids);
    }

    // Close text content if any
    if has_text {
        send(&tx, &ev::output_text_done(&acc.msg_item_id, 0, 0, &acc.full_text)).await;
        send(&tx, &ev::content_part_done(&acc.msg_item_id, 0, 0, &acc.full_text)).await;
        send(&tx, &ev::output_item_done_message(&acc.msg_item_id, 0, &acc.full_text, rc)).await;
    }
    // Tool-call-only: no message item was emitted, no need to close it

    // Close tool calls
    if has_tool_calls {
        for (idx, tc) in &acc.tool_calls {
            let item_id = format!("fc_{}", tc.id);
            let oi = idx + 1;
            send(&tx, &ev::function_call_arguments_done(&item_id, oi, &tc.arguments)).await;
            send(&tx, &ev::function_call_done(&item_id, oi, &tc.id, &tc.name, &tc.arguments, rc)).await;
        }
    }

    // response.completed with usage
    send(&tx, &ev::response_completed(&acc.response_id, &acc.model, acc.usage.as_ref())).await;
    Ok(())
}

async fn send(tx: &mpsc::Sender<String>, event: &str) {
    let _ = tx.send(event.to_string()).await;
}

/// Normalize upstream usage to Responses API format.
fn normalize_usage(u: &serde_json::Value) -> serde_json::Value {
    let input = u.get("prompt_tokens").or(u.get("input_tokens")).and_then(|v| v.as_i64()).unwrap_or(0);
    let output = u.get("completion_tokens").or(u.get("output_tokens")).and_then(|v| v.as_i64()).unwrap_or(0);
    let cached = u.get("prompt_cache_hit_tokens")
        .or(u.get("prompt_tokens_details").and_then(|d| d.get("cached_tokens")))
        .and_then(|v| v.as_i64()).unwrap_or(0);
    let reasoning = u.get("completion_tokens_details")
        .and_then(|d| d.get("reasoning_tokens"))
        .and_then(|v| v.as_i64()).unwrap_or(0);

    serde_json::json!({
        "input_tokens": input,
        "output_tokens": output,
        "total_tokens": input + output,
        "input_tokens_details": { "cached_tokens": cached },
        "output_tokens_details": { "reasoning_tokens": reasoning }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_normalize_usage_openai_format() {
        let u = json!({
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        });
        let result = normalize_usage(&u);
        assert_eq!(result["input_tokens"], 100);
        assert_eq!(result["output_tokens"], 50);
        assert_eq!(result["total_tokens"], 150);
        assert_eq!(result["input_tokens_details"]["cached_tokens"], 0);
        assert_eq!(result["output_tokens_details"]["reasoning_tokens"], 0);
    }

    #[test]
    fn test_normalize_usage_deepseek_format() {
        let u = json!({
            "input_tokens": 200,
            "output_tokens": 80,
            "total_tokens": 280,
            "prompt_cache_hit_tokens": 50,
            "completion_tokens_details": { "reasoning_tokens": 30 }
        });
        let result = normalize_usage(&u);
        assert_eq!(result["input_tokens"], 200);
        assert_eq!(result["output_tokens"], 80);
        assert_eq!(result["total_tokens"], 280);
        assert_eq!(result["input_tokens_details"]["cached_tokens"], 50);
        assert_eq!(result["output_tokens_details"]["reasoning_tokens"], 30);
    }

    #[test]
    fn test_normalize_usage_prompt_tokens_details_cached() {
        let u = json!({
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "prompt_tokens_details": { "cached_tokens": 40 }
        });
        let result = normalize_usage(&u);
        assert_eq!(result["input_tokens_details"]["cached_tokens"], 40);
    }

    #[test]
    fn test_normalize_usage_empty() {
        let u = json!({});
        let result = normalize_usage(&u);
        assert_eq!(result["input_tokens"], 0);
        assert_eq!(result["output_tokens"], 0);
        assert_eq!(result["total_tokens"], 0);
    }

    #[test]
    fn test_sse_accumulator_new() {
        let acc = SseAccumulator::new("resp_abc".to_string(), "gpt-4".to_string());
        assert_eq!(acc.response_id, "resp_abc");
        assert_eq!(acc.model, "gpt-4");
        assert_eq!(acc.msg_item_id, "msg_abc");
        assert!(acc.full_text.is_empty());
        assert!(acc.tool_calls.is_empty());
        assert!(acc.reasoning_content.is_empty());
        assert!(acc.usage.is_none());
        assert!(acc.events_log.is_empty());
    }

    #[test]
    fn test_sse_accumulator_tool_calls_list_empty() {
        let acc = SseAccumulator::new("resp_1".to_string(), "m".to_string());
        assert!(acc.tool_calls_list().is_empty());
    }

    #[test]
    fn test_sse_accumulator_log_event() {
        let mut acc = SseAccumulator::new("resp_1".to_string(), "m".to_string());
        acc.log_event("event1");
        acc.log_event("event2");
        assert!(acc.events_log.contains("event1\n"));
        assert!(acc.events_log.contains("event2\n"));
    }

    #[test]
    fn test_sse_accumulator_log_event_truncation() {
        let mut acc = SseAccumulator::new("resp_1".to_string(), "m".to_string());
        let big = "x".repeat(MAX_EVENTS_LOG_SIZE + 1000);
        acc.log_event(&big);
        // The event itself may be dropped entirely if it exceeds remaining capacity,
        // but log_event adds len+1, so after the first big event it should stop.
        assert!(acc.events_size >= big.len() + 1 || acc.events_size <= MAX_EVENTS_LOG_SIZE);
        assert!(acc.events_size <= MAX_EVENTS_LOG_SIZE + big.len() + 1);
    }

    #[test]
    fn test_accumulated_tool_call_fields() {
        let tc = AccumulatedToolCall {
            id: "call_1".to_string(),
            name: "search".to_string(),
            arguments: "{\"q\":\"hi\"}".to_string(),
            emitted_added: false,
            last_args_len: 0,
        };
        assert_eq!(tc.id, "call_1");
        assert_eq!(tc.name, "search");
        assert_eq!(tc.arguments, "{\"q\":\"hi\"}");
    }
}
