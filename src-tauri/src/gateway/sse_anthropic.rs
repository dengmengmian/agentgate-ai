use std::collections::BTreeMap;
use tokio::sync::mpsc;
use serde_json::{json, Value};

use crate::protocol::responses_events as ev;

const MAX_EVENTS_LOG_SIZE: usize = 1_000_000;

/// Sanitize tool call ID (whitelist `[a-zA-Z0-9_-]`, max 64 chars).
/// Symmetric with the request path via `transform::tool_calls::sanitize_call_id`.
fn clamp_call_id(id: &str) -> String {
    crate::transform::tool_calls::sanitize_call_id(id).into_owned()
}

/// Accumulated tool call from Claude streaming.
#[derive(Debug, Clone)]
pub struct AccumulatedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
    last_args_len: usize,
    output_index: usize,
}

/// State for converting Claude SSE stream to Responses API SSE events.
pub struct AnthropicSseAccumulator {
    pub response_id: String,
    pub model: String,
    pub full_text: String,
    pub tool_calls: BTreeMap<usize, AccumulatedToolCall>,
    pub reasoning_content: String,
    pub usage: Option<Value>,
    pub events_log: String,
    events_size: usize,
    next_output_index: usize,
    text_item_emitted: bool,
    msg_item_id: String,
    reasoning_output_index: Option<usize>,
    reasoning_item_id: String,
}

impl AnthropicSseAccumulator {
    pub fn new(response_id: String, model: String) -> Self {
        let msg_item_id = format!("msg_{}", response_id.replace("resp_", ""));
        let reasoning_item_id = format!("rs_{}", response_id.replace("resp_", ""));
        Self {
            response_id, model,
            full_text: String::new(),
            tool_calls: BTreeMap::new(),
            reasoning_content: String::new(),
            usage: None,
            events_log: String::new(),
            events_size: 0,
            next_output_index: 0,
            text_item_emitted: false,
            msg_item_id,
            reasoning_output_index: None,
            reasoning_item_id,
        }
    }

    pub fn tool_calls_list(&self) -> Vec<&AccumulatedToolCall> {
        self.tool_calls.values().collect()
    }
}

/// Process a Claude SSE stream and emit Responses API SSE events.
///
/// The caller is expected to have already run `sse_bootstrap::bootstrap_detect`
/// on the upstream response — the `Bootstrap.prefix` carries bytes already
/// consumed during the scan and gets replayed before pulling from the live
/// stream, so frames straddling the bootstrap boundary are handled exactly
/// once.
pub async fn process_anthropic_stream(
    boot: crate::gateway::sse_bootstrap::Bootstrap,
    tx: mpsc::Sender<String>,
    acc: &mut AnthropicSseAccumulator,
) -> Result<(), String> {
    use futures::StreamExt;

    crate::protocol::responses_events::reset_sequence();

    // Seed buffer with the bootstrap prefix; first loop iteration parses any
    // complete frames already present without pulling from the live stream.
    let mut buffer = String::from_utf8_lossy(&boot.prefix).into_owned();
    buffer = buffer.replace("\r\n", "\n");
    let mut current_event_type = String::new();
    let mut stream = boot.stream;
    let mut bootstrap_replayed = false;

    loop {
        if bootstrap_replayed {
            let chunk = match stream.next().await {
                Some(Ok(b)) => b,
                Some(Err(e)) => {
                    let err_msg = crate::gateway::sse_bootstrap::describe_stream_error(&e);
                    send(&tx, &ev::response_failed(&acc.response_id, &acc.model, &err_msg)).await;
                    return Err(err_msg);
                }
                None => break,
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            buffer = buffer.replace("\r\n", "\n");
        }
        bootstrap_replayed = true;

        while let Some(frame_end) = buffer.find("\n\n") {
            let frame = buffer[..frame_end].to_string();
            buffer = buffer[frame_end + 2..].to_string();

            // Parse event type and data from frame
            let mut event_type = String::new();
            let mut data_str = String::new();

            for line in frame.lines() {
                // Handle both "event: X" and "event:X"
                if let Some(et) = line.strip_prefix("event:").map(|s| s.trim()) {
                    event_type = et.to_string();
                } else if let Some(d) = line.strip_prefix("data:").map(|s| s.trim()) {
                    data_str = d.to_string();
                }
            }

            if event_type.is_empty() {
                event_type = current_event_type.clone();
            } else {
                current_event_type = event_type.clone();
            }

            if data_str.is_empty() {
                continue;
            }

            let data: Value = match serde_json::from_str(&data_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Log event
            if acc.events_size < MAX_EVENTS_LOG_SIZE {
                let entry = format!("event: {event_type} data: {data_str}\n");
                acc.events_size += entry.len();
                acc.events_log.push_str(&entry);
            }

            match event_type.as_str() {
                "message_start" => {
                    // Capture model and input usage
                    if let Some(msg) = data.get("message") {
                        if let Some(m) = msg.get("model").and_then(|m| m.as_str()) {
                            acc.model = m.to_string();
                        }
                        if let Some(u) = msg.get("usage") {
                            acc.usage = Some(json!({
                                "input_tokens": u.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                                "output_tokens": 0
                            }));
                        }
                    }
                    // Emit response.created + in_progress
                    send(&tx, &ev::response_created(&acc.response_id, &acc.model)).await;
                    send(&tx, &ev::response_in_progress(&acc.response_id, &acc.model)).await;
                }
                "content_block_start" => {
                    let index = data.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let empty = json!({});
                    let block = data.get("content_block").unwrap_or(&empty);
                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match block_type {
                        "text" => {
                            if !acc.text_item_emitted {
                                let oi = acc.next_output_index;
                                acc.next_output_index += 1;
                                send(&tx, &ev::output_item_added_message(&acc.msg_item_id, oi)).await;
                                send(&tx, &ev::content_part_added(&acc.msg_item_id, oi, 0)).await;
                                acc.text_item_emitted = true;
                            }
                        }
                        "tool_use" => {
                            let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let clamped_id = clamp_call_id(id);
                            let oi = acc.next_output_index;
                            acc.next_output_index += 1;
                            let item_id = format!("fc_{}", clamped_id);
                            acc.tool_calls.insert(index, AccumulatedToolCall {
                                id: clamped_id.clone(),
                                name: name.to_string(),
                                arguments: String::new(),
                                last_args_len: 0,
                                output_index: oi,
                            });
                            send(&tx, &ev::function_call_added(&item_id, oi, &clamped_id, name)).await;
                        }
                        "thinking" => {
                            // Just track that we're in thinking mode — accumulate in deltas
                        }
                        _ => {}
                    }
                }
                "content_block_delta" => {
                    let index = data.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    let empty_delta = json!({});
                    let delta = data.get("delta").unwrap_or(&empty_delta);
                    let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match delta_type {
                        "text_delta" => {
                            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                acc.full_text.push_str(text);
                                if acc.text_item_emitted {
                                    send(&tx, &ev::output_text_delta(&acc.msg_item_id, 0, 0, text)).await;
                                }
                            }
                        }
                        "input_json_delta" => {
                            if let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str()) {
                                if let Some(tc) = acc.tool_calls.get_mut(&index) {
                                    tc.arguments.push_str(partial);
                                    let delta_args = &tc.arguments[tc.last_args_len..];
                                    let item_id = format!("fc_{}", tc.id);
                                    send(&tx, &ev::function_call_arguments_delta(&item_id, tc.output_index, delta_args)).await;
                                    tc.last_args_len = tc.arguments.len();
                                }
                            }
                        }
                        "thinking_delta" => {
                            if let Some(thinking) = delta.get("thinking").and_then(|t| t.as_str()) {
                                if acc.reasoning_content.is_empty() {
                                    acc.reasoning_content.push_str("**Thinking**\n\n");
                                }
                                acc.reasoning_content.push_str(thinking);
                                stream_reasoning_delta(&tx, acc, thinking).await;
                            }
                        }
                        _ => {}
                    }
                }
                "content_block_stop" => {
                    // No specific action needed — finalize handles closing
                }
                "message_delta" => {
                    if let Some(u) = data.get("usage") {
                        let out_tokens = u.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                        if let Some(ref mut existing) = acc.usage {
                            existing["output_tokens"] = json!(out_tokens);
                        } else {
                            acc.usage = Some(json!({"input_tokens": 0, "output_tokens": out_tokens}));
                        }
                    }
                }
                "message_stop" => {
                    // Finalize
                    finalize(acc, &tx).await;
                }
                "error" => {
                    let err_msg = data.get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown Claude API error");
                    let full_err = format!("Claude API error: {err_msg}");
                    send(&tx, &ev::response_failed(&acc.response_id, &acc.model, &full_err)).await;
                    return Err(full_err);
                }
                _ => {}
            }
        }
    }

    // If message_stop wasn't received, finalize anyway
    if !acc.full_text.is_empty() || !acc.tool_calls.is_empty() {
        finalize(acc, &tx).await;
    } else {
        // Stream ended with no content at all — notify client
        send(&tx, &ev::response_failed(&acc.response_id, &acc.model, "Stream ended unexpectedly")).await;
    }

    Ok(())
}

/// 发送一段 reasoning 增量（Anthropic thinking_delta → Responses API delta）。
/// 首次调用占位 output_item.added(reasoning) 并抢占 output_index。
async fn stream_reasoning_delta(
    tx: &mpsc::Sender<String>,
    acc: &mut AnthropicSseAccumulator,
    delta: &str,
) {
    if delta.is_empty() {
        return;
    }
    let oi = match acc.reasoning_output_index {
        Some(oi) => oi,
        None => {
            let oi = acc.next_output_index;
            acc.next_output_index += 1;
            acc.reasoning_output_index = Some(oi);
            send(tx, &ev::output_item_added_reasoning(&acc.reasoning_item_id, oi)).await;
            oi
        }
    };
    send(tx, &ev::reasoning_summary_text_delta(&acc.reasoning_item_id, oi, 0, delta)).await;
}

async fn finalize(acc: &mut AnthropicSseAccumulator, tx: &mpsc::Sender<String>) {
    // Store reasoning for multi-turn
    if !acc.reasoning_content.is_empty() {
        let tc_ids: Vec<String> = acc.tool_calls.values().map(|tc| tc.id.clone()).collect();
        crate::transform::reasoning_store::store(&acc.full_text, &acc.reasoning_content, &tc_ids);
    }

    // Close streamed reasoning summary, if any. Emitted before text/tool dones
    // so the order matches OpenAI Responses canonical event ordering.
    if let Some(oi) = acc.reasoning_output_index {
        send(tx, &ev::reasoning_summary_text_done(&acc.reasoning_item_id, oi, 0, &acc.reasoning_content)).await;
        send(tx, &ev::output_item_done_reasoning(&acc.reasoning_item_id, oi, &acc.reasoning_content)).await;
    }

    // Text done events
    if acc.text_item_emitted {
        send(tx, &ev::output_text_done(&acc.msg_item_id, 0, 0, &acc.full_text)).await;
        send(tx, &ev::content_part_done(&acc.msg_item_id, 0, 0, &acc.full_text)).await;
        let rc = if acc.reasoning_content.is_empty() { None } else { Some(acc.reasoning_content.as_str()) };
        send(tx, &ev::output_item_done_message(&acc.msg_item_id, 0, &acc.full_text, rc)).await;
    }

    // Tool call done events
    for tc in acc.tool_calls.values() {
        let item_id = format!("fc_{}", tc.id);
        let rc = if acc.reasoning_content.is_empty() { None } else { Some(acc.reasoning_content.as_str()) };
        send(tx, &ev::function_call_arguments_done(&item_id, tc.output_index, &tc.arguments)).await;
        send(tx, &ev::function_call_done(&item_id, tc.output_index, &tc.id, &tc.name, &tc.arguments, rc)).await;
    }

    // response.completed
    send(tx, &ev::response_completed(&acc.response_id, &acc.model, acc.usage.as_ref())).await;
}

async fn send(tx: &mpsc::Sender<String>, event: &str) {
    let _ = tx.send(event.to_string()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_call_id_short() {
        assert_eq!(clamp_call_id("abc"), "abc");
    }

    #[test]
    fn clamp_call_id_exact_length() {
        let id = "a".repeat(64);
        assert_eq!(clamp_call_id(&id).len(), 64);
    }

    #[test]
    fn clamp_call_id_long() {
        let id = "a".repeat(100);
        let clamped = clamp_call_id(&id);
        assert_eq!(clamped.len(), 64);
    }

    #[test]
    fn accumulator_new() {
        let acc = AnthropicSseAccumulator::new("resp_123".into(), "claude-sonnet".into());
        assert_eq!(acc.response_id, "resp_123");
        assert_eq!(acc.model, "claude-sonnet");
        assert!(acc.full_text.is_empty());
        assert!(acc.tool_calls.is_empty());
    }

    #[test]
    fn accumulator_tool_calls_list_empty() {
        let acc = AnthropicSseAccumulator::new("resp_1".into(), "model".into());
        assert!(acc.tool_calls_list().is_empty());
    }
}
