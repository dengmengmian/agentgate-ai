use std::collections::BTreeMap;
use futures::StreamExt;
use tokio::sync::mpsc;

use crate::protocol::chat_completions::ChatCompletionChunk;
use crate::transform::reasoning_store;
use crate::protocol::responses_events as ev;

const MAX_EVENTS_LOG_SIZE: usize = 1_000_000; // 1MB
/// Sanitize tool call ID (whitelist `[a-zA-Z0-9_-]`, max 64 chars).
///
/// Thin wrapper around `transform::tool_calls::sanitize_call_id` so the response
/// path stays symmetric with the request path — same id transformation applied
/// at every boundary means no per-session mapping table is needed for the
/// client to re-correlate tool results.
fn clamp_call_id(id: &str) -> String {
    crate::transform::tool_calls::sanitize_call_id(id).into_owned()
}

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
    /// Web-search citations / annotations collected from delta.annotations
    /// as they stream in. Embedded into the final output_item.done message
    /// so the client sees the references.
    pub annotations: Vec<serde_json::Value>,
    pub events_log: String,
    events_size: usize,
    next_output_index: usize,
    text_content_started: bool,
    /// Output index reserved for the reasoning item when the first reasoning
    /// chunk arrives. `None` if no reasoning has streamed yet — set lazily so
    /// non-thinking responses don't emit a spurious empty reasoning item.
    reasoning_output_index: Option<usize>,
    /// Cached reasoning item id matching `output_item_added_reasoning`; used
    /// by subsequent deltas + the finalize close-out.
    reasoning_item_id: String,
}

impl SseAccumulator {
    pub fn new(response_id: String, model: String) -> Self {
        let msg_item_id = format!("msg_{}", &response_id.replace("resp_", ""));
        let reasoning_item_id = format!("rs_{}", &response_id.replace("resp_", ""));
        Self {
            response_id,
            msg_item_id,
            model,
            full_text: String::new(),
            tool_calls: BTreeMap::new(),
            reasoning_content: String::new(),
            usage: None,
            annotations: Vec::new(),
            events_log: String::new(),
            events_size: 0,
            next_output_index: 1, // 0 is reserved for the message item
            text_content_started: false,
            reasoning_output_index: None,
            reasoning_item_id,
        }
    }

    fn log_event(&mut self, event: &str) {
        let remaining = MAX_EVENTS_LOG_SIZE.saturating_sub(self.events_size);
        if remaining > 0 {
            let to_add = event.len().min(remaining);
            self.events_log.push_str(&event[..to_add]);
            self.events_log.push('\n');
            self.events_size += to_add + 1;
        }
    }

    pub fn tool_calls_list(&self) -> Vec<AccumulatedToolCall> {
        self.tool_calls.values().cloned().collect()
    }
}

/// Process upstream Chat Completions SSE and emit Responses API SSE events.
///
/// The caller is expected to have already run `sse_bootstrap::bootstrap_detect`
/// on the upstream response so any HTTP-200-with-error-frame failure mode has
/// been turned into a clean Err that triggers failover before we commit to
/// streaming. The `Bootstrap.prefix` carries whatever bytes the bootstrap
/// scan already consumed — we replay those first, then drain the live stream.
pub async fn process_upstream_stream(
    boot: crate::gateway::sse_bootstrap::Bootstrap,
    tx: mpsc::Sender<String>,
    acc: &mut SseAccumulator,
) -> Result<(), String> {
    ev::reset_sequence();

    // 1. response.created + in_progress
    send(&tx, &ev::response_created(&acc.response_id, &acc.model)).await;
    send(&tx, &ev::response_in_progress(&acc.response_id, &acc.model)).await;

    // NOTE: output_item.added is deferred until first text delta to avoid
    // emitting a spurious empty message item for tool-call-only responses.

    let mut stream = boot.stream;
    let mut buffer = String::from_utf8_lossy(&boot.prefix).into_owned();
    let mut has_text = false;
    let mut has_tool_calls = false;
    let mut message_item_emitted = false;

    // Drain the prefix buffer before we ever poll the live stream — preserves
    // any complete SSE frames that bootstrap already pulled.
    loop {
        // First, parse out any complete lines already buffered.
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end_matches('\r').to_string();
            buffer = buffer[line_end + 1..].to_string();

            match dispatch_line(
                &line,
                &tx,
                acc,
                &mut has_text,
                &mut has_tool_calls,
                &mut message_item_emitted,
            )
            .await
            {
                LineOutcome::Continue => {}
                LineOutcome::Done(result) => return result,
            }
        }

        // Then pull more from upstream.
        let chunk_bytes = match stream.next().await {
            Some(Ok(b)) => b,
            Some(Err(e)) => {
                let msg = crate::gateway::sse_bootstrap::describe_stream_error(&e);
                send(&tx, &ev::response_failed(&acc.response_id, &acc.model, &msg)).await;
                return Err(msg);
            }
            None => {
                if has_text || has_tool_calls {
                    return finalize(tx, acc, has_text, has_tool_calls).await;
                }
                send(
                    &tx,
                    &ev::response_failed(&acc.response_id, &acc.model, "Stream ended unexpectedly"),
                )
                .await;
                return Err("Stream ended without [DONE]".to_string());
            }
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk_bytes));
    }
}

enum LineOutcome {
    Continue,
    Done(Result<(), String>),
}

/// Per-line SSE dispatch — shared between the prefix replay and the live
/// stream so a frame straddling the boundary is handled identically.
async fn dispatch_line(
    line: &str,
    tx: &mpsc::Sender<String>,
    acc: &mut SseAccumulator,
    has_text: &mut bool,
    has_tool_calls: &mut bool,
    message_item_emitted: &mut bool,
) -> LineOutcome {
    if line.is_empty() || line.starts_with(':') {
        return LineOutcome::Continue;
    }

    let Some(data) = line.strip_prefix("data:").map(|d| d.trim()) else {
        return LineOutcome::Continue;
    };

    if data == "[DONE]" {
        return LineOutcome::Done(finalize(tx.clone(), acc, *has_text, *has_tool_calls).await);
    }

    acc.log_event(data);

    let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data) else {
        return LineOutcome::Continue;
    };

    if let Some(ref u) = chunk.usage {
        acc.usage = Some(normalize_usage(u));
    }

    let Some(choices) = &chunk.choices else {
        return LineOutcome::Continue;
    };

    process_choices(
        choices,
        tx,
        acc,
        has_text,
        has_tool_calls,
        message_item_emitted,
    )
    .await;

    LineOutcome::Continue
}

async fn process_choices(
    choices: &[crate::protocol::chat_completions::ChunkChoice],
    tx: &mpsc::Sender<String>,
    acc: &mut SseAccumulator,
    has_text: &mut bool,
    has_tool_calls: &mut bool,
    message_item_emitted: &mut bool,
) {
    for choice in choices {
        let Some(delta) = &choice.delta else { continue };

        // ── Text content (with <think> tag splitting) ──
        if let Some(ref content) = delta.content {
            if !content.is_empty() {
                // Split <think> tags — MiniMax embeds thinking in content
                let (text, thinking) =
                    crate::transform::responses_to_chat::split_think_tags(content);
                if let Some(ref tk) = thinking {
                    acc.reasoning_content.push_str(tk);
                }
                if !text.is_empty() {
                    if !*message_item_emitted {
                        send(tx, &ev::output_item_added_message(&acc.msg_item_id, 0)).await;
                        send(tx, &ev::content_part_added(&acc.msg_item_id, 0, 0)).await;
                        acc.text_content_started = true;
                        *message_item_emitted = true;
                    }
                    *has_text = true;
                    acc.full_text.push_str(&text);
                    send(tx, &ev::output_text_delta(&acc.msg_item_id, 0, 0, &text)).await;
                }
            }
        }

        // ── Reasoning content ──
        if let Some(ref rc) = delta.reasoning_content {
            if !rc.is_empty() {
                if !*message_item_emitted {
                    send(tx, &ev::output_item_added_message(&acc.msg_item_id, 0)).await;
                    *message_item_emitted = true;
                }
                // Inject "**Thinking**\n\n" header so Codex TUI shows reasoning
                if acc.reasoning_content.is_empty() {
                    acc.reasoning_content.push_str("**Thinking**\n\n");
                }
                acc.reasoning_content.push_str(rc);
                stream_reasoning_delta(tx, acc, rc).await;
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
                        stream_reasoning_delta(tx, acc, text).await;
                    }
                }
            }
        }

        // ── Web-search annotations / citations ──
        // MiMo emits these on the first streaming chunk; OpenAI's
        // search-preview models emit per chunk. Forward each as an
        // `output_text.annotation.added` event so the client can
        // surface citations in real time, and accumulate them for the
        // final output_item.done message.
        if let Some(ref anns) = delta.annotations {
            for ann in anns {
                let annotation_index = acc.annotations.len();
                send(
                    tx,
                    &ev::output_text_annotation_added(
                        &acc.msg_item_id,
                        0,
                        0,
                        annotation_index,
                        ann,
                    ),
                )
                .await;
                acc.annotations.push(ann.clone());
            }
        }

        // ── Legacy delta.function_call → synthetic tool_call ──
        if let Some(ref fc) = delta.function_call {
            *has_tool_calls = true;
            let idx = 0usize;
            if !acc.tool_calls.contains_key(&idx) {
                acc.tool_calls.insert(
                    idx,
                    AccumulatedToolCall {
                        id: format!("call_legacy_{}", acc.response_id.replace("resp_", "")),
                        name: String::new(),
                        arguments: String::new(),
                        emitted_added: false,
                        last_args_len: 0,
                    },
                );
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
                send(tx, &ev::function_call_added(&item_id, 1, &tc.id, &tc.name)).await;
                tc.emitted_added = true;
            }
            if tc.emitted_added && tc.arguments.len() > tc.last_args_len {
                let delta_args = &tc.arguments[tc.last_args_len..];
                let item_id = format!("fc_{}", tc.id);
                send(
                    tx,
                    &ev::function_call_arguments_delta(&item_id, 1, delta_args),
                )
                .await;
                tc.last_args_len = tc.arguments.len();
            }
        }

        // ── Tool calls (streaming deltas) ──
        if let Some(ref tcs) = delta.tool_calls {
            *has_tool_calls = true;
            for tc_delta in tcs {
                let idx = tc_delta.index.unwrap_or(0) as usize;

                if !acc.tool_calls.contains_key(&idx) {
                    acc.tool_calls.insert(
                        idx,
                        AccumulatedToolCall {
                            id: String::new(),
                            name: String::new(),
                            arguments: String::new(),
                            emitted_added: false,
                            last_args_len: 0,
                        },
                    );
                }

                let tc = acc.tool_calls.get_mut(&idx).unwrap();

                if let Some(ref id) = tc_delta.id {
                    if tc.id.is_empty() {
                        tc.id = clamp_call_id(id);
                    }
                }

                if let Some(ref func) = tc_delta.function {
                    if let Some(ref name) = func.name {
                        tc.name.push_str(name);
                    }
                    if let Some(ref args) = func.arguments {
                        tc.arguments.push_str(args);
                    }
                }

                if tc.id.is_empty() {
                    tc.id = format!("call_{}_{}", acc.response_id.replace("resp_", ""), idx);
                }

                if !tc.emitted_added && !tc.name.is_empty() {
                    let item_id = format!("fc_{}", tc.id);
                    let oi = acc.next_output_index;
                    acc.next_output_index += 1;
                    send(tx, &ev::function_call_added(&item_id, oi, &tc.id, &tc.name)).await;
                    tc.emitted_added = true;
                }

                if tc.emitted_added && tc.arguments.len() > tc.last_args_len {
                    let delta_args = &tc.arguments[tc.last_args_len..];
                    let item_id = format!("fc_{}", tc.id);
                    send(
                        tx,
                        &ev::function_call_arguments_delta(&item_id, idx + 1, delta_args),
                    )
                    .await;
                    tc.last_args_len = tc.arguments.len();
                }
            }
        }
    }
}

async fn finalize(
    tx: mpsc::Sender<String>,
    acc: &SseAccumulator,
    has_text: bool,
    has_tool_calls: bool,
) -> Result<(), String> {
    let rc = if acc.reasoning_content.is_empty() { None } else { Some(acc.reasoning_content.as_str()) };

    // Store reasoning for future requests (in-memory LRU, process-local —
    // survives within the same process for the same conversation thread).
    if !acc.reasoning_content.is_empty() {
        let tc_ids: Vec<String> = acc.tool_calls.values().map(|tc| tc.id.clone()).collect();
        reasoning_store::store(&acc.full_text, &acc.reasoning_content, &tc_ids);
    }

    // Pin reasoning into a dedicated `reasoning` output_item with
    // `encrypted_content`. Codex round-trips this verbatim, so the trace
    // survives process restarts and multi-turn tool calls preserve their
    // reasoning_content (required by MiMo / DeepSeek thinking-mode upstream).
    if !acc.reasoning_content.is_empty() {
        // Reuse the oi reserved during streaming if delta events were emitted;
        // otherwise (non-streaming reasoning paths, providers that surface
        // reasoning only in the final chunk) allocate fresh — keeps
        // back-compat with consumers that don't subscribe to delta events.
        let oi = acc.reasoning_output_index.unwrap_or(acc.next_output_index);
        if acc.reasoning_output_index.is_some() {
            // Streamed: close out the summary text before the item.done.
            send(&tx, &ev::reasoning_summary_text_done(&acc.reasoning_item_id, oi, 0, &acc.reasoning_content)).await;
        }
        send(&tx, &ev::output_item_done_reasoning(&acc.reasoning_item_id, oi, &acc.reasoning_content)).await;
    }

    // Close text content if any
    if has_text {
        send(&tx, &ev::output_text_done(&acc.msg_item_id, 0, 0, &acc.full_text)).await;
        send(&tx, &ev::content_part_done(&acc.msg_item_id, 0, 0, &acc.full_text)).await;
        send(&tx, &ev::output_item_done_message_with_annotations(
            &acc.msg_item_id, 0, &acc.full_text, rc, &acc.annotations,
        )).await;
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

/// 发送一段 reasoning 增量。首次调用会顺手把 reasoning 的 output_item
/// 占位事件先打出去（output_item.added），后续仅发 delta。output_index
/// 在首次时从 `acc.next_output_index` 抢占——这样和 tool_calls 共用
/// 同一个递增空间，不会冲突。
async fn stream_reasoning_delta(tx: &mpsc::Sender<String>, acc: &mut SseAccumulator, delta: &str) {
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
        // log_event should cap at MAX_EVENTS_LOG_SIZE, never exceed it (plus 1 for newline)
        assert!(acc.events_size <= MAX_EVENTS_LOG_SIZE + 1);
        assert_eq!(acc.events_log.len(), MAX_EVENTS_LOG_SIZE + 1); // truncated content + newline
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

    #[tokio::test]
    async fn stream_reasoning_delta_first_chunk_emits_added_and_delta() {
        let mut acc = SseAccumulator::new("resp_xyz".to_string(), "deepseek".to_string());
        let (tx, mut rx) = mpsc::channel::<String>(8);
        stream_reasoning_delta(&tx, &mut acc, "Hello").await;
        drop(tx);

        let first = rx.recv().await.expect("expected added event");
        let second = rx.recv().await.expect("expected delta event");
        assert!(first.contains("response.output_item.added"), "got: {first}");
        assert!(first.contains("\"type\":\"reasoning\""), "got: {first}");
        assert!(first.contains("rs_xyz"), "got: {first}");
        assert!(second.contains("response.reasoning_summary_text.delta"), "got: {second}");
        assert!(second.contains("\"delta\":\"Hello\""), "got: {second}");
        assert!(rx.recv().await.is_none(), "only added + delta on first chunk");
        assert_eq!(acc.reasoning_output_index, Some(1));
        // next_output_index advanced so tool calls won't reuse the reasoning slot.
        assert_eq!(acc.next_output_index, 2);
    }

    #[tokio::test]
    async fn stream_reasoning_delta_subsequent_chunks_emit_delta_only() {
        let mut acc = SseAccumulator::new("resp_xyz".to_string(), "deepseek".to_string());
        let (tx, mut rx) = mpsc::channel::<String>(8);
        stream_reasoning_delta(&tx, &mut acc, "A").await;
        // Drain the first two events (added + delta).
        let _ = rx.recv().await;
        let _ = rx.recv().await;
        stream_reasoning_delta(&tx, &mut acc, "B").await;
        drop(tx);

        let second_delta = rx.recv().await.expect("second delta");
        assert!(second_delta.contains("response.reasoning_summary_text.delta"));
        assert!(second_delta.contains("\"delta\":\"B\""));
        assert!(rx.recv().await.is_none(), "no second added event");
        assert_eq!(acc.next_output_index, 2, "index reserved only once");
    }

    #[tokio::test]
    async fn stream_reasoning_delta_empty_is_noop() {
        let mut acc = SseAccumulator::new("resp_xyz".to_string(), "deepseek".to_string());
        let (tx, mut rx) = mpsc::channel::<String>(8);
        stream_reasoning_delta(&tx, &mut acc, "").await;
        drop(tx);

        assert!(rx.recv().await.is_none(), "empty delta must not emit events");
        assert!(acc.reasoning_output_index.is_none());
    }
}
