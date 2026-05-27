use std::collections::BTreeMap;
use tokio::sync::mpsc;
use serde_json::{json, Value};

use crate::protocol::responses_events as ev;

const MAX_EVENTS_LOG_SIZE: usize = 1_000_000;

/// Accumulated tool call from Gemini streaming.
#[derive(Debug, Clone)]
pub struct AccumulatedToolCall {
    pub name: String,
    pub arguments: String,
    pub output_index: usize,
}

/// State for converting Gemini SSE stream to Responses API SSE events.
pub struct GeminiSseAccumulator {
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
    tool_call_counter: usize,
}

impl GeminiSseAccumulator {
    pub fn new(response_id: String, model: String) -> Self {
        let msg_item_id = format!("msg_{}", response_id.replace("resp_", ""));
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
            tool_call_counter: 0,
        }
    }

    #[allow(dead_code)]
    pub fn tool_calls_list(&self) -> Vec<&AccumulatedToolCall> {
        self.tool_calls.values().collect()
    }
}

/// Process a Gemini SSE stream and emit Responses API SSE events.
///
/// Caller is expected to have already run `sse_bootstrap::bootstrap_detect`
/// — `Bootstrap.prefix` is replayed first so frames pulled during the scan
/// are processed before we read more from the live stream.
pub async fn process_gemini_stream(
    boot: crate::gateway::sse_bootstrap::Bootstrap,
    tx: mpsc::Sender<String>,
    acc: &mut GeminiSseAccumulator,
) -> Result<(), String> {
    use futures::StreamExt;

    crate::protocol::responses_events::reset_sequence();

    // Emit response.created + in_progress
    send(&tx, &ev::response_created(&acc.response_id, &acc.model)).await;
    send(&tx, &ev::response_in_progress(&acc.response_id, &acc.model)).await;

    let mut buffer = String::from_utf8_lossy(&boot.prefix).into_owned();
    buffer = buffer.replace("\r\n", "\n");
    let mut stream = boot.stream;
    let mut bootstrap_replayed = false;

    loop {
        if bootstrap_replayed {
            let chunk = match stream.next().await {
                Some(Ok(b)) => b,
                Some(Err(e)) => {
                    let err_msg = format!("Stream error: {e}");
                    send(&tx, &ev::response_failed(&acc.response_id, &acc.model, &err_msg)).await;
                    return Err(err_msg);
                }
                None => break,
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
            buffer = buffer.replace("\r\n", "\n");
        }
        bootstrap_replayed = true;

        // Process complete SSE frames
        while let Some(frame_end) = buffer.find("\n\n") {
            let frame = buffer[..frame_end].to_string();
            buffer = buffer[frame_end + 2..].to_string();

            // Parse data line
            let mut data_str = String::new();
            for line in frame.lines() {
                if let Some(d) = line.strip_prefix("data:").map(|s| s.trim()) {
                    data_str = d.to_string();
                }
            }

            if data_str.is_empty() { continue; }

            // Log
            if acc.events_size < MAX_EVENTS_LOG_SIZE {
                let entry = format!("data: {data_str}\n");
                acc.events_size += entry.len();
                acc.events_log.push_str(&entry);
            }

            let data: Value = match serde_json::from_str(&data_str) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Check for error
            if let Some(err) = data.get("error") {
                let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("Gemini API error");
                let full_err = format!("Gemini API error: {msg}");
                send(&tx, &ev::response_failed(&acc.response_id, &acc.model, &full_err)).await;
                return Err(full_err);
            }

            // Extract candidates[0]
            let candidate = match data.get("candidates")
                .and_then(|c| c.as_array())
                .and_then(|a| a.first()) {
                Some(c) => c,
                None => {
                    // May be a usage-only event at the end
                    if let Some(usage) = data.get("usageMetadata") {
                        acc.usage = Some(json!({
                            "input_tokens": usage.get("promptTokenCount").and_then(|v| v.as_i64()).unwrap_or(0),
                            "output_tokens": usage.get("candidatesTokenCount").and_then(|v| v.as_i64()).unwrap_or(0),
                        }));
                    }
                    continue;
                }
            };

            // Extract usage from this event
            if let Some(usage) = data.get("usageMetadata") {
                acc.usage = Some(json!({
                    "input_tokens": usage.get("promptTokenCount").and_then(|v| v.as_i64()).unwrap_or(0),
                    "output_tokens": usage.get("candidatesTokenCount").and_then(|v| v.as_i64()).unwrap_or(0),
                }));
            }

            // Process parts
            if let Some(parts) = candidate.get("content")
                .and_then(|c| c.get("parts"))
                .and_then(|p| p.as_array()) {
                for part in parts {
                    // Text part
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        if !acc.text_item_emitted {
                            let oi = acc.next_output_index;
                            acc.next_output_index += 1;
                            send(&tx, &ev::output_item_added_message(&acc.msg_item_id, oi)).await;
                            send(&tx, &ev::content_part_added(&acc.msg_item_id, oi, 0)).await;
                            acc.text_item_emitted = true;
                        }
                        acc.full_text.push_str(text);
                        send(&tx, &ev::output_text_delta(&acc.msg_item_id, 0, 0, text)).await;
                    }

                    // Function call part (Gemini sends complete functionCall, not streamed)
                    if let Some(fc) = part.get("functionCall") {
                        let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                        let args = fc.get("args").map(|a| a.to_string()).unwrap_or("{}".to_string());
                        let call_id = format!("call_gemini_{}", acc.tool_call_counter);
                        acc.tool_call_counter += 1;

                        let oi = acc.next_output_index;
                        acc.next_output_index += 1;
                        let item_id = format!("fc_{call_id}");

                        acc.tool_calls.insert(oi, AccumulatedToolCall {
                            name: name.to_string(),
                            arguments: args.clone(),
                            output_index: oi,
                        });

                        send(&tx, &ev::function_call_added(&item_id, oi, &call_id, name)).await;
                        send(&tx, &ev::function_call_arguments_delta(&item_id, oi, &args)).await;
                    }
                }
            }

            // Check finish reason
            if let Some(_finish) = candidate.get("finishReason").and_then(|f| f.as_str()) {
                // Will finalize after stream ends
            }
        }
    }

    // Finalize
    finalize(acc, &tx).await;

    Ok(())
}

async fn finalize(acc: &mut GeminiSseAccumulator, tx: &mpsc::Sender<String>) {
    // Store reasoning for multi-turn
    if !acc.reasoning_content.is_empty() {
        let tc_ids: Vec<String> = acc.tool_calls.values()
            .map(|tc| format!("call_gemini_{}", tc.output_index))
            .collect();
        crate::transform::reasoning_store::store(&acc.full_text, &acc.reasoning_content, &tc_ids);
    }

    // Text done events
    if acc.text_item_emitted {
        send(tx, &ev::output_text_done(&acc.msg_item_id, 0, 0, &acc.full_text)).await;
        send(tx, &ev::content_part_done(&acc.msg_item_id, 0, 0, &acc.full_text)).await;
        let rc = if acc.reasoning_content.is_empty() { None } else { Some(acc.reasoning_content.as_str()) };
        send(tx, &ev::output_item_done_message(&acc.msg_item_id, 0, &acc.full_text, rc)).await;
    }

    // Tool call done events
    for (idx, tc) in &acc.tool_calls {
        let call_id = format!("call_gemini_{idx}");
        let item_id = format!("fc_{call_id}");
        let rc = if acc.reasoning_content.is_empty() { None } else { Some(acc.reasoning_content.as_str()) };
        send(tx, &ev::function_call_arguments_done(&item_id, tc.output_index, &tc.arguments)).await;
        send(tx, &ev::function_call_done(&item_id, tc.output_index, &call_id, &tc.name, &tc.arguments, rc)).await;
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
    fn gemini_accumulator_new() {
        let acc = GeminiSseAccumulator::new("resp_456".into(), "gemini-2.5-flash".into());
        assert_eq!(acc.response_id, "resp_456");
        assert_eq!(acc.model, "gemini-2.5-flash");
        assert!(acc.full_text.is_empty());
        assert!(acc.tool_calls.is_empty());
        assert_eq!(acc.tool_call_counter, 0);
    }

    #[test]
    fn gemini_accumulator_tool_calls_list_empty() {
        let acc = GeminiSseAccumulator::new("resp_1".into(), "model".into());
        assert!(acc.tool_calls_list().is_empty());
    }
}
