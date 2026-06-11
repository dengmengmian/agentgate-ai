use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio::sync::mpsc;

use crate::protocol::responses_events as ev;

const MAX_EVENTS_LOG_SIZE: usize = 1_000_000;

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
    /// 按到达顺序累积的 thinking 块——含签名 / redacted data，给下一轮
    /// 多轮 thinking-mode 工具调用回放签名链用。
    pub thinking_blocks: Vec<crate::transform::thinking_blocks::ThinkingBlock>,
    /// Anthropic message_delta 携带的 `stop_reason`（end_turn / max_tokens /
    /// stop_sequence / tool_use / refusal），映射到 Responses status 用。
    pub stop_reason: Option<String>,
}

impl AnthropicSseAccumulator {
    pub fn new(response_id: String, model: String) -> Self {
        let msg_item_id = format!("msg_{}", response_id.replace("resp_", ""));
        let reasoning_item_id = format!("rs_{}", response_id.replace("resp_", ""));
        Self {
            response_id,
            model,
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
            thinking_blocks: Vec::new(),
            stop_reason: None,
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
    let mut utf8_pending: Vec<u8> = Vec::new();
    let mut buffer = String::new();
    crate::gateway::stream_utf8::append_utf8_safe(&mut buffer, &mut utf8_pending, &boot.prefix);
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
                    send(
                        &tx,
                        &ev::response_failed(&acc.response_id, &acc.model, &err_msg),
                    )
                    .await;
                    return Err(err_msg);
                }
                None => break,
            };
            crate::gateway::stream_utf8::append_utf8_safe(&mut buffer, &mut utf8_pending, &chunk);
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
                            // message_start 携带完整初始 usage，包括 cache 字段。
                            // 这些值在流过程中不再变（cache_creation/read 是请求级
                            // 概念），message_delta 只补 output_tokens 增量。
                            let mut usage = json!({
                                "input_tokens": u.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
                                "output_tokens": 0
                            });
                            if let Some(cc) = u
                                .get("cache_creation_input_tokens")
                                .and_then(|v| v.as_i64())
                            {
                                usage["cache_creation_input_tokens"] = json!(cc);
                            }
                            if let Some(cr) =
                                u.get("cache_read_input_tokens").and_then(|v| v.as_i64())
                            {
                                usage["cache_read_input_tokens"] = json!(cr);
                            }
                            acc.usage = Some(usage);
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
                                send(&tx, &ev::output_item_added_message(&acc.msg_item_id, oi))
                                    .await;
                                send(&tx, &ev::content_part_added(&acc.msg_item_id, oi, 0)).await;
                                acc.text_item_emitted = true;
                            }
                        }
                        "tool_use" => {
                            let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let clamped_id =
                                crate::transform::tool_calls::sanitize_call_id(id).into_owned();
                            let oi = acc.next_output_index;
                            acc.next_output_index += 1;
                            let item_id = format!("fc_{}", clamped_id);
                            acc.tool_calls.insert(
                                index,
                                AccumulatedToolCall {
                                    id: clamped_id.clone(),
                                    name: name.to_string(),
                                    arguments: String::new(),
                                    last_args_len: 0,
                                    output_index: oi,
                                },
                            );
                            send(
                                &tx,
                                &ev::function_call_added(&item_id, oi, &clamped_id, name),
                            )
                            .await;
                        }
                        "thinking" => {
                            // 新 thinking 块开始——预留位置，文本和签名分别由
                            // thinking_delta / signature_delta 填进来。
                            acc.thinking_blocks.push(
                                crate::transform::thinking_blocks::ThinkingBlock::thinking(""),
                            );
                        }
                        "redacted_thinking" => {
                            // 安全系统加密的 thinking——data 字段直接给到，
                            // 没有 delta 流式过程，必须原样保留以支持下一轮签名链。
                            let data = block.get("data").and_then(|d| d.as_str()).unwrap_or("");
                            acc.thinking_blocks.push(
                                crate::transform::thinking_blocks::ThinkingBlock::redacted(data),
                            );
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
                                    send(&tx, &ev::output_text_delta(&acc.msg_item_id, 0, 0, text))
                                        .await;
                                }
                            }
                        }
                        "input_json_delta" => {
                            if let Some(partial) =
                                delta.get("partial_json").and_then(|p| p.as_str())
                            {
                                if let Some(tc) = acc.tool_calls.get_mut(&index) {
                                    tc.arguments.push_str(partial);
                                    let delta_args = &tc.arguments[tc.last_args_len..];
                                    let item_id = format!("fc_{}", tc.id);
                                    send(
                                        &tx,
                                        &ev::function_call_arguments_delta(
                                            &item_id,
                                            tc.output_index,
                                            delta_args,
                                        ),
                                    )
                                    .await;
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
                                // 同步累积到当前 thinking 块（最新 push 的那个）。
                                if let Some(last) = acc.thinking_blocks.last_mut() {
                                    if last.kind == "thinking" {
                                        last.text.push_str(thinking);
                                    }
                                }
                                stream_reasoning_delta(&tx, acc, thinking).await;
                            }
                        }
                        "signature_delta" => {
                            // Anthropic 把签名分块发，但实测大多数情况一次到位。
                            // 不管多少块，append 到当前块的 signature 字段即可。
                            if let Some(sig) = delta.get("signature").and_then(|s| s.as_str()) {
                                if let Some(last) = acc.thinking_blocks.last_mut() {
                                    if last.kind == "thinking" {
                                        last.signature.push_str(sig);
                                    }
                                }
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
                        let out_tokens =
                            u.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
                        if let Some(ref mut existing) = acc.usage {
                            existing["output_tokens"] = json!(out_tokens);
                        } else {
                            acc.usage =
                                Some(json!({"input_tokens": 0, "output_tokens": out_tokens}));
                        }
                    }
                    // 捕获 stop_reason 给 finalize 映射成 Responses status。
                    if let Some(sr) = data
                        .get("delta")
                        .and_then(|d| d.get("stop_reason"))
                        .and_then(|s| s.as_str())
                    {
                        if !sr.is_empty() {
                            acc.stop_reason = Some(sr.to_string());
                        }
                    }
                }
                "message_stop" => {
                    // Finalize
                    finalize(acc, &tx).await;
                }
                "error" => {
                    let err_msg = data
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("Unknown Claude API error");
                    let full_err = format!("Claude API error: {err_msg}");
                    send(
                        &tx,
                        &ev::response_failed(&acc.response_id, &acc.model, &full_err),
                    )
                    .await;
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
        send(
            &tx,
            &ev::response_failed(&acc.response_id, &acc.model, "Stream ended unexpectedly"),
        )
        .await;
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
            send(
                tx,
                &ev::output_item_added_reasoning(&acc.reasoning_item_id, oi),
            )
            .await;
            oi
        }
    };
    send(
        tx,
        &ev::reasoning_summary_text_delta(&acc.reasoning_item_id, oi, 0, delta),
    )
    .await;
}

async fn finalize(acc: &mut AnthropicSseAccumulator, tx: &mpsc::Sender<String>) {
    // Store reasoning for multi-turn
    if !acc.reasoning_content.is_empty() {
        let tc_ids: Vec<String> = acc.tool_calls.values().map(|tc| tc.id.clone()).collect();
        crate::transform::reasoning_store::store(&acc.full_text, &acc.reasoning_content, &tc_ids);
    }

    // Close streamed reasoning summary, if any. Emitted before text/tool dones
    // so the order matches OpenAI Responses canonical event ordering. 把累积
    // 的 thinking 签名块编码进 encrypted_content——下一轮 client 回传时能
    // 还原签名链，否则 Anthropic 会因为 sig-chain 校验失败 400。
    if let Some(oi) = acc.reasoning_output_index {
        send(
            tx,
            &ev::reasoning_summary_text_done(&acc.reasoning_item_id, oi, 0, &acc.reasoning_content),
        )
        .await;
        send(
            tx,
            &ev::output_item_done_reasoning_with_blocks(
                &acc.reasoning_item_id,
                oi,
                &acc.reasoning_content,
                &acc.thinking_blocks,
            ),
        )
        .await;
    }

    // Text done events
    if acc.text_item_emitted {
        send(
            tx,
            &ev::output_text_done(&acc.msg_item_id, 0, 0, &acc.full_text),
        )
        .await;
        send(
            tx,
            &ev::content_part_done(&acc.msg_item_id, 0, 0, &acc.full_text),
        )
        .await;
        let rc = if acc.reasoning_content.is_empty() {
            None
        } else {
            Some(acc.reasoning_content.as_str())
        };
        send(
            tx,
            &ev::output_item_done_message(&acc.msg_item_id, 0, &acc.full_text, rc),
        )
        .await;
    }

    // Tool call done events
    for tc in acc.tool_calls.values() {
        let item_id = format!("fc_{}", tc.id);
        let rc = if acc.reasoning_content.is_empty() {
            None
        } else {
            Some(acc.reasoning_content.as_str())
        };
        send(
            tx,
            &ev::function_call_arguments_done(&item_id, tc.output_index, &tc.arguments),
        )
        .await;
        send(
            tx,
            &ev::function_call_done(
                &item_id,
                tc.output_index,
                &tc.id,
                &tc.name,
                &tc.arguments,
                rc,
            ),
        )
        .await;
    }

    // response.completed —— 顺手把 Anthropic stop_reason 映射进 status。
    // output 暂传空数组（Anthropic 路径暂未累积 output_items；与 sse.rs 同步
    // 改造是独立工作）。
    send(
        tx,
        &ev::response_completed_with_stop_reason(
            &acc.response_id,
            &acc.model,
            acc.usage.as_ref(),
            acc.stop_reason.as_deref(),
            &[],
        ),
    )
    .await;
}

async fn send(tx: &mpsc::Sender<String>, event: &str) {
    let _ = tx.send(event.to_string()).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    // call_id sanitize 行为已在 transform::tool_calls::tests 全面覆盖；
    // 这里曾有 clamp_call_id 包装函数的局部测试，函数已删除。

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
