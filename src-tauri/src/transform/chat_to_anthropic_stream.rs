//! Chat Completions SSE → Anthropic Messages SSE 增量转换器。
//!
//! 场景：client 走 /v1/messages（Anthropic 协议）但 provider 只支持 OpenAI
//! Chat Completions（没配 anthropic_base_url），且 client 要 `stream:true`。
//!
//! 之前的 fallback 是"非流式上游 + 合成 SSE"（详见 [`crate::protocol::
//! anthropic_messages::synthesize_sse_events`]），首字延迟 = 上游完整生成
//! 耗时。这个模块实现**真流式**：边收上游 chat chunk 边转换成 Anthropic
//! SSE 事件发给 client，首字延迟 = 上游首字延迟。
//!
//! 状态机要点：
//! - 第一个 chunk 到达时 emit `message_start`（input_tokens 留 0，最后 usage
//!   到达时由 message_delta 携带 output_tokens）
//! - 文本 delta → 必要时 emit `content_block_start{type:text}`、再 emit
//!   `content_block_delta{type:text_delta}`
//! - reasoning_content delta（DeepSeek-thinking / MiMo / o1 风格）→ 同上但
//!   block type 是 thinking、delta type 是 thinking_delta
//! - tool_calls delta：每个 upstream tool_call_index 映射到独立的 Anthropic
//!   content_block index；first delta 携带 id + name 触发
//!   `content_block_start{type:tool_use}`，后续 arguments 增量 emit
//!   `content_block_delta{type:input_json_delta, partial_json}`
//! - finish_reason 到达 → 关闭所有 open 的 content_block、emit `message_delta`
//!   带 stop_reason 映射、emit `message_stop`

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::protocol::chat_completions::ChatCompletionChunk;

/// 一个已经打开（emit 过 content_block_start、未关闭）的 Anthropic content block。
/// 块种类靠它在哪个字段持有来区分（text_block / thinking_block / tool_blocks），
/// 不另开 enum。
struct OpenBlock {
    /// Anthropic content 数组里的 index——与 chat tool_call_index 不同体系。
    anthropic_idx: usize,
}

pub struct ChatToAnthropicStream {
    message_id: String,
    model: String,
    /// 是否已经 emit message_start——第一个 upstream chunk 到达时触发一次。
    started: bool,
    /// 当前打开的 text block（如果有）。Chat 流的 text content 全部 collapse
    /// 进同一个 Anthropic text content_block。
    text_block: Option<OpenBlock>,
    /// 当前打开的 thinking block（如果有）。
    thinking_block: Option<OpenBlock>,
    /// 上游 tool_call_index → 我方 OpenBlock 的映射。Anthropic content block
    /// 序号必须连续、按打开顺序递增；upstream tool_call_index 也是连续的但
    /// 单独编号体系，需要这层映射。
    tool_blocks: HashMap<i64, OpenBlock>,
    /// 下一个可分配的 Anthropic content_block index。
    next_anthropic_idx: usize,
    /// 终块带的 finish_reason；用来映射 Anthropic stop_reason。
    finish_reason: Option<String>,
    /// 上游最终 usage（含 include_usage 时）。
    final_usage: Option<Value>,
    /// 是否已经 emit message_stop——防御幂等性。
    stopped: bool,
}

impl ChatToAnthropicStream {
    pub fn new(model: impl Into<String>) -> Self {
        // 生成稳定的 message id，与 from_chat_response 同形态（"msg_" + uuid 前缀）。
        let message_id = format!(
            "msg_{}",
            &uuid::Uuid::new_v4().to_string().replace('-', "")[..12]
        );
        Self {
            message_id,
            model: model.into(),
            started: false,
            text_block: None,
            thinking_block: None,
            tool_blocks: HashMap::new(),
            next_anthropic_idx: 0,
            finish_reason: None,
            final_usage: None,
            stopped: false,
        }
    }

    /// 消费一个解析好的 Chat Completions chunk，返回这一步应该立即写给 client
    /// 的 Anthropic SSE 事件列表。
    pub fn process_chunk(&mut self, chunk: &ChatCompletionChunk) -> Vec<String> {
        let mut events: Vec<String> = Vec::new();

        // 第一次拿到 chunk 时 emit message_start。input_tokens 用 chunk 里的
        // usage（若 include_usage 在首块就送）或者 0；output_tokens 起步 1。
        if !self.started {
            self.started = true;
            let input_tokens = chunk
                .usage
                .as_ref()
                .and_then(|u| u.get("prompt_tokens").or_else(|| u.get("input_tokens")))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            events.push(sse_event(
                "message_start",
                json!({
                    "type": "message_start",
                    "message": {
                        "id": self.message_id,
                        "type": "message",
                        "role": "assistant",
                        "model": self.model,
                        "content": [],
                        "stop_reason": Value::Null,
                        "stop_sequence": Value::Null,
                        "usage": {
                            "input_tokens": input_tokens,
                            "output_tokens": 1
                        }
                    }
                }),
            ));
        }

        // 终块（OpenAI 习惯）携带 usage 但 choices 为空或没 delta。先取下来。
        if let Some(u) = &chunk.usage {
            self.final_usage = Some(u.clone());
        }

        let Some(choices) = &chunk.choices else {
            return events;
        };

        for choice in choices {
            // finish_reason 通常只在终块。先存起来；真正 emit message_delta 是
            // 在 finalize 时统一做（避免某些上游分多次发 finish_reason 时重复 emit）。
            if let Some(fr) = &choice.finish_reason {
                if !fr.is_empty() {
                    self.finish_reason = Some(fr.clone());
                }
            }

            let Some(delta) = &choice.delta else { continue };

            // reasoning_content → thinking block。Anthropic 顺序约束：thinking
            // 必须在 text/tool_use 之前。如果 text 已经打开了 reasoning 才来，
            // 仍然按到达顺序 emit——客户端容忍乱序、且这只是 fallback 路径。
            if let Some(rc) = &delta.reasoning_content {
                if !rc.is_empty() {
                    events.extend(self.ensure_thinking_block());
                    if let Some(b) = &self.thinking_block {
                        events.push(sse_event(
                            "content_block_delta",
                            json!({
                                "type": "content_block_delta",
                                "index": b.anthropic_idx,
                                "delta": {"type": "thinking_delta", "thinking": rc}
                            }),
                        ));
                    }
                }
            }
            // reasoning_details 数组（o3/o4 native 风格）—— 取 text 字段拼到 thinking。
            if let Some(details) = &delta.reasoning_details {
                for d in details {
                    if let Some(text) = d.get("text").and_then(|t| t.as_str()) {
                        if !text.is_empty() {
                            events.extend(self.ensure_thinking_block());
                            if let Some(b) = &self.thinking_block {
                                events.push(sse_event(
                                    "content_block_delta",
                                    json!({
                                        "type": "content_block_delta",
                                        "index": b.anthropic_idx,
                                        "delta": {"type": "thinking_delta", "thinking": text}
                                    }),
                                ));
                            }
                        }
                    }
                }
            }

            // text content → text block
            if let Some(text) = &delta.content {
                if !text.is_empty() {
                    events.extend(self.ensure_text_block());
                    if let Some(b) = &self.text_block {
                        events.push(sse_event(
                            "content_block_delta",
                            json!({
                                "type": "content_block_delta",
                                "index": b.anthropic_idx,
                                "delta": {"type": "text_delta", "text": text}
                            }),
                        ));
                    }
                }
            }

            // tool_calls deltas
            if let Some(tcs) = &delta.tool_calls {
                for tc in tcs {
                    let tc_idx = tc.index.unwrap_or(0);
                    let func = tc.function.as_ref();
                    let name = func.and_then(|f| f.name.as_deref()).unwrap_or("");
                    let id = tc.id.as_deref().unwrap_or("");

                    // 第一次见这个 tc_idx 时打开 content_block。OpenAI 习惯：
                    // 首个 delta 携带 id + function.name + arguments=""；后续
                    // delta 仅携带 arguments 增量。
                    let need_open = !self.tool_blocks.contains_key(&tc_idx);
                    if need_open {
                        let anthropic_idx = self.alloc_idx();
                        let sanitized_id = crate::transform::tool_calls::sanitize_call_id(id);
                        let sanitized_name = crate::transform::tool_calls::sanitize_tool_name(name);
                        events.push(sse_event(
                            "content_block_start",
                            json!({
                                "type": "content_block_start",
                                "index": anthropic_idx,
                                "content_block": {
                                    "type": "tool_use",
                                    "id": sanitized_id.as_ref(),
                                    "name": sanitized_name.as_ref(),
                                    "input": {}
                                }
                            }),
                        ));
                        self.tool_blocks.insert(
                            tc_idx,
                            OpenBlock { anthropic_idx },
                        );
                    }

                    // arguments 增量 → input_json_delta
                    if let Some(args) = func.and_then(|f| f.arguments.as_deref()) {
                        if !args.is_empty() {
                            let anthropic_idx = self.tool_blocks[&tc_idx].anthropic_idx;
                            events.push(sse_event(
                                "content_block_delta",
                                json!({
                                    "type": "content_block_delta",
                                    "index": anthropic_idx,
                                    "delta": {"type": "input_json_delta", "partial_json": args}
                                }),
                            ));
                        }
                    }
                }
            }
        }

        events
    }

    /// 在上游流结束（[DONE] / connection close）时调用，emit 收尾事件：
    /// 关闭所有 open 的 content_block、message_delta（stop_reason + usage）、
    /// message_stop。重复调用是 no-op（stopped 标记守护）。
    pub fn finalize(&mut self) -> Vec<String> {
        if self.stopped {
            return Vec::new();
        }
        self.stopped = true;

        let mut events: Vec<String> = Vec::new();

        // 没收到任何 chunk 就直接 finalize（上游 0 字节就关）：补 message_start。
        if !self.started {
            self.started = true;
            events.push(sse_event(
                "message_start",
                json!({
                    "type": "message_start",
                    "message": {
                        "id": self.message_id,
                        "type": "message",
                        "role": "assistant",
                        "model": self.model,
                        "content": [],
                        "stop_reason": Value::Null,
                        "stop_sequence": Value::Null,
                        "usage": {"input_tokens": 0, "output_tokens": 1}
                    }
                }),
            ));
        }

        // 关闭所有 open block（顺序：thinking → text → tool_uses）
        if let Some(b) = self.thinking_block.take() {
            events.push(sse_event(
                "content_block_stop",
                json!({"type": "content_block_stop", "index": b.anthropic_idx}),
            ));
        }
        if let Some(b) = self.text_block.take() {
            events.push(sse_event(
                "content_block_stop",
                json!({"type": "content_block_stop", "index": b.anthropic_idx}),
            ));
        }
        // tool_blocks 按 anthropic_idx 升序关闭，行为可预测
        let mut tool_blocks: Vec<OpenBlock> = self.tool_blocks.drain().map(|(_, b)| b).collect();
        tool_blocks.sort_by_key(|b| b.anthropic_idx);
        for b in tool_blocks {
            events.push(sse_event(
                "content_block_stop",
                json!({"type": "content_block_stop", "index": b.anthropic_idx}),
            ));
        }

        // message_delta：stop_reason 映射 + 终 usage 的 output_tokens
        let stop_reason = map_finish_reason(self.finish_reason.as_deref());
        let output_tokens = self
            .final_usage
            .as_ref()
            .and_then(|u| u.get("completion_tokens").or_else(|| u.get("output_tokens")))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        events.push(sse_event(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop_reason, "stop_sequence": Value::Null},
                "usage": {"output_tokens": output_tokens}
            }),
        ));

        // message_stop
        events.push(sse_event(
            "message_stop",
            json!({"type": "message_stop"}),
        ));

        events
    }

    fn alloc_idx(&mut self) -> usize {
        let i = self.next_anthropic_idx;
        self.next_anthropic_idx += 1;
        i
    }

    fn ensure_text_block(&mut self) -> Vec<String> {
        if self.text_block.is_some() {
            return Vec::new();
        }
        let idx = self.alloc_idx();
        self.text_block = Some(OpenBlock { anthropic_idx: idx });
        vec![sse_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": idx,
                "content_block": {"type": "text", "text": ""}
            }),
        )]
    }

    fn ensure_thinking_block(&mut self) -> Vec<String> {
        if self.thinking_block.is_some() {
            return Vec::new();
        }
        let idx = self.alloc_idx();
        self.thinking_block = Some(OpenBlock { anthropic_idx: idx });
        vec![sse_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": idx,
                "content_block": {"type": "thinking", "thinking": ""}
            }),
        )]
    }
}

fn sse_event(event_type: &str, data: Value) -> String {
    format!("event: {event_type}\ndata: {data}\n\n")
}

/// Chat finish_reason → Anthropic stop_reason 映射。与
/// [`crate::protocol::anthropic_messages::from_chat_response`] 保持一致。
fn map_finish_reason(fr: Option<&str>) -> &'static str {
    match fr {
        Some("length") => "max_tokens",
        Some("tool_calls") | Some("function_call") => "tool_use",
        Some("content_filter") => "refusal",
        Some("stop") => "end_turn",
        _ => "end_turn",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> ChatCompletionChunk {
        serde_json::from_str(s).expect("test chunk parses")
    }

    #[test]
    fn emits_message_start_on_first_chunk() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let chunk = parse(r#"{"id":"x","choices":[{"index":0,"delta":{"role":"assistant","content":""}}]}"#);
        let events = s.process_chunk(&chunk);
        assert!(events[0].contains("event: message_start"));
        assert!(events[0].contains("\"output_tokens\":1"));
    }

    #[test]
    fn text_content_emits_text_block_lifecycle() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"role":"assistant","content":""}}]}"#));
        let events = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"content":"Hello"}}]}"#));
        let joined = events.join("");
        assert!(joined.contains("event: content_block_start"));
        assert!(joined.contains("\"type\":\"text\""));
        assert!(joined.contains("event: content_block_delta"));
        assert!(joined.contains("\"text_delta\""));
        assert!(joined.contains("\"text\":\"Hello\""));
    }

    #[test]
    fn subsequent_text_chunks_only_emit_delta() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"content":"a"}}]}"#));
        let events = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"content":"b"}}]}"#));
        // 仅 1 个 delta，不再 emit content_block_start
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("content_block_delta"));
        assert!(events[0].contains("\"text\":\"b\""));
    }

    #[test]
    fn reasoning_content_emits_thinking_block() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"role":"assistant"}}]}"#));
        let events = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"reasoning_content":"think..."}}]}"#));
        let joined = events.join("");
        assert!(joined.contains("\"type\":\"thinking\""));
        assert!(joined.contains("\"thinking_delta\""));
        assert!(joined.contains("\"thinking\":\"think...\""));
    }

    #[test]
    fn tool_call_first_delta_opens_block_with_id_and_name() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"role":"assistant"}}]}"#));
        let events = s.process_chunk(&parse(
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_x","function":{"name":"search","arguments":""}}]}}]}"#,
        ));
        let joined = events.join("");
        assert!(joined.contains("content_block_start"));
        assert!(joined.contains("\"type\":\"tool_use\""));
        assert!(joined.contains("\"id\":\"call_x\""));
        assert!(joined.contains("\"name\":\"search\""));
    }

    #[test]
    fn tool_call_argument_deltas_emit_input_json_delta() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"role":"assistant"}}]}"#));
        let _ = s.process_chunk(&parse(
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_x","function":{"name":"search","arguments":""}}]}}]}"#,
        ));
        let events = s.process_chunk(&parse(
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"q"}}]}}]}"#,
        ));
        let joined = events.join("");
        assert!(joined.contains("\"input_json_delta\""));
        // arguments 增量原样塞 partial_json（注意是 Chat 流自己分块的形态）
        assert!(joined.contains("\"partial_json\":\"{\\\"q\""));
    }

    #[test]
    fn finalize_closes_open_blocks_and_emits_terminal_events() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"role":"assistant","content":"hi"}}]}"#));
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"completion_tokens":5}}"#));
        let events = s.finalize();
        let joined = events.join("");
        assert!(joined.contains("content_block_stop"));
        assert!(joined.contains("event: message_delta"));
        assert!(joined.contains("\"stop_reason\":\"end_turn\""));
        assert!(joined.contains("\"output_tokens\":5"));
        assert!(joined.contains("event: message_stop"));
    }

    #[test]
    fn finish_reason_length_maps_to_max_tokens() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"content":"trunc"}}]}"#));
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{},"finish_reason":"length"}]}"#));
        let events = s.finalize();
        let joined = events.join("");
        assert!(joined.contains("\"stop_reason\":\"max_tokens\""));
    }

    #[test]
    fn parallel_tool_calls_get_distinct_anthropic_indices() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"role":"assistant"}}]}"#));
        let events1 = s.process_chunk(&parse(
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"a","arguments":""}}]}}]}"#,
        ));
        let events2 = s.process_chunk(&parse(
            r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"id":"c2","function":{"name":"b","arguments":""}}]}}]}"#,
        ));
        let joined1 = events1.join("");
        let joined2 = events2.join("");
        // 两个 tool_call_index → 两个独立 Anthropic block_start
        assert!(joined1.contains("\"index\":0"));
        assert!(joined2.contains("\"index\":1"));
        assert!(joined2.contains("\"id\":\"c2\""));
    }

    #[test]
    fn finalize_is_idempotent() {
        let mut s = ChatToAnthropicStream::new("claude-3");
        let _ = s.process_chunk(&parse(r#"{"choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":"stop"}]}"#));
        let first = s.finalize();
        let second = s.finalize();
        assert!(!first.is_empty());
        assert!(second.is_empty(), "second finalize must be no-op");
    }

    #[test]
    fn finalize_without_chunks_still_emits_message_start_and_stop() {
        // 罕见但要防御：上游一字节都没送就关闭，要 emit 合法的 Anthropic 流。
        let mut s = ChatToAnthropicStream::new("claude-3");
        let events = s.finalize();
        let joined = events.join("");
        assert!(joined.contains("message_start"));
        assert!(joined.contains("message_stop"));
    }

    #[test]
    fn message_id_format_matches_anthropic_convention() {
        let s = ChatToAnthropicStream::new("claude-3");
        assert!(s.message_id.starts_with("msg_"));
        assert_eq!(s.message_id.len(), "msg_".len() + 12);
    }
}
