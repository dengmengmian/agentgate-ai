//! Codex remote compaction v2 网关实现。
//!
//! ## 背景
//!
//! Codex 在长对话超过模型 context window 时会触发"remote compaction":
//! 把当前 history 发到上游(`POST /v1/responses` + 特殊 header / 旧版还会带
//! `/compact` sub-path),让 OpenAI 自家的 `gpt-5.5-openai-compact` 模型产出一条
//! 紧凑的"history summary" output item,Codex 拿到后**把那个 item 直接塞回**下次
//! request 的 input,作为 history 的占位项。Item 的 `encrypted_content` 是
//! OpenAI 加密 token,Codex 不解析,只是上下文回环用。
//!
//! 这套机制依赖 OpenAI 专属模型 + 加密 token,**MiMo / DeepSeek 等上游接不上**——
//! 转过去要么 503(无此模型),要么不知道 v2 协议直接 fail。
//!
//! ## 我们的方案
//!
//! 网关在请求阶段就拦截:
//! 1. **探嗅**(`is_codex_v2_compaction`):header `x-codex-beta-features` 含
//!    `remote_compaction_v2`,或 `x-codex-turn-metadata.request_kind == "compaction"`,
//!    或 URL 含 `/compact`(旧 Codex 兼容)
//! 2. **本地 summary**:借 `auto_compact::summarize_chunk` 用同 provider 跑一次
//!    非流式 chat completion(已经验证过的 chain),拿到 summary 文本
//! 3. **编码**:`AGENTGATE_COMPACT_V1:<base64(summary)>` 塞 `encrypted_content`,
//!    magic prefix 让我们下次能识别还原(避免跟真 OpenAI 加密 token 混淆)
//! 4. **SSE 输出**:`response.output_item.done` (item type=compaction) + `response.completed`
//!
//! 下一轮 Codex 把 `{"type":"compaction","encrypted_content":"AGENTGATE_COMPACT_V1:..."}`
//! 塞回 input 数组,`transform::responses_to_chat::convert_input_array` 识别 prefix,
//! 解码出 summary,转成一条 user message 注入 history。
//!
//! ## 跟 `auto_compact` 的关系
//!
//! `auto_compact` 是**网关侧自驱动**——任何 client 发的请求超阈值就摘要 middle 段。
//! `codex_compact` 是**响应 Codex 自身的 compact 指令**——只在 Codex 触发 v2 协议
//! 时才介入,流程跟 Codex 期望的 SSE shape 严格对齐。两者共享 summarizer 调用链。

use std::time::Instant;

use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};

use crate::errors::AppError;
use crate::gateway::auto_compact;
use crate::gateway::routes::shared::GatewayError;
use crate::protocol::chat_completions::ChatMessage;
use crate::protocol::openai_responses::ResponsesRequest;
use crate::providers::adapter::ProviderConfig;

/// `encrypted_content` 的 magic prefix。AgentGate 自己塞进去的内容必须以这个开头,
/// 否则当不可识别的 token 透传(给原生 OpenAI 直连场景留余地)。
const ENCRYPTED_PREFIX: &str = "AGENTGATE_COMPACT_V1:";

/// 探嗅一个 POST /v1/responses 是不是 Codex remote compaction v2 请求。
///
/// **默认关闭**(env `AGENTGATE_CODEX_COMPACT=1` 才打开),因为现实里发现:
/// Codex 端 SSE parser 对 `response.completed.usage` 的字段要求比 fixture
/// 看上去严格,我们返回的 SSE 即使带 usage 也会被丢弃,造成 Codex stream 不
/// 完成、用户输入 "继续" 没反应,**连带普通对话都被阻断**(每个 turn 前都
/// 先尝试 compaction)。
///
/// 关掉后 compact 请求走原 chat 转换路径,上游(MiMo 等)会因不识别专属模型
/// 返 503,Codex CLI 收到 503 后会向用户报错(而不是静默 hang),用户可以
/// `/clear` 或显式手动 compact 恢复。
///
/// 修好 SSE 真兼容后把 env 默认改回 on,或换成 gateway_settings 字段读 DB。
///
/// 规则(任一为真且 env 启用):
/// - URL path 含 `/compact`(旧 Codex sub-path)
/// - header `x-codex-beta-features` 含 `remote_compaction_v2`
/// - header `x-codex-turn-metadata` 是 JSON 含 `"request_kind":"compaction"`
pub fn is_codex_v2_compaction(headers: &HeaderMap, uri_path: &str) -> bool {
    if !is_enabled() {
        return false;
    }
    if uri_path.contains("/compact") {
        return true;
    }
    if let Some(v) = headers
        .get("x-codex-beta-features")
        .and_then(|h| h.to_str().ok())
    {
        if v.split(',')
            .any(|s| s.trim().eq_ignore_ascii_case("remote_compaction_v2"))
        {
            return true;
        }
    }
    if let Some(v) = headers
        .get("x-codex-turn-metadata")
        .and_then(|h| h.to_str().ok())
    {
        if let Ok(j) = serde_json::from_str::<Value>(v) {
            if j.get("request_kind").and_then(|x| x.as_str()) == Some("compaction") {
                return true;
            }
        }
    }
    false
}

fn is_enabled() -> bool {
    std::env::var("AGENTGATE_CODEX_COMPACT")
        .ok()
        .map(|v| matches!(v.trim(), "1" | "true" | "on" | "yes"))
        .unwrap_or(false)
}

/// 把 summary 文本编码成 `encrypted_content`。
pub fn encode_summary(summary: &str) -> String {
    format!("{ENCRYPTED_PREFIX}{}", B64.encode(summary.as_bytes()))
}

/// 反向:从 `encrypted_content` 拿回 summary。`None` 表示不是 AgentGate 生成的
/// 内容(可能是真 OpenAI 加密 token,这种就让上层透传)。
pub fn decode_summary(encrypted_content: &str) -> Option<String> {
    let payload = encrypted_content.strip_prefix(ENCRYPTED_PREFIX)?;
    let bytes = B64.decode(payload.trim()).ok()?;
    String::from_utf8(bytes).ok()
}

/// 从 ResponsesRequest 的 `input`(Value)抽出"可读 history"渲染成 ChatMessage 列表,
/// 供 summarizer 用。规则跟 Codex `is_retained_for_remote_compaction_v2` 一致:
/// 只保留 role 是 user / developer / system 的 message。
fn extract_chat_messages(req: &ResponsesRequest) -> Vec<ChatMessage> {
    let mut msgs: Vec<ChatMessage> = Vec::new();

    // instructions 字段当作 system message
    if let Some(s) = &req.instructions {
        if !s.is_empty() {
            msgs.push(ChatMessage {
                role: "system".into(),
                content: Some(Value::String(s.clone())),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
    }

    match &req.input {
        Value::String(s) => {
            msgs.push(ChatMessage {
                role: "user".into(),
                content: Some(Value::String(s.clone())),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        Value::Array(items) => {
            for it in items {
                let item_type = it.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if item_type == "message" {
                    let role = it
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or("user")
                        .to_string();
                    if !matches!(role.as_str(), "user" | "developer" | "system" | "assistant") {
                        continue;
                    }
                    let text = extract_message_text(it.get("content"));
                    if text.is_empty() {
                        continue;
                    }
                    msgs.push(ChatMessage {
                        role,
                        content: Some(Value::String(text)),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    });
                } else if item_type == "compaction" || item_type == "compaction_summary" {
                    // 之前一轮 AgentGate 生成的 compaction item 又被塞回来了——
                    // 解出原 summary 当 user message,让上下文链不断。
                    if let Some(enc) = it.get("encrypted_content").and_then(|v| v.as_str()) {
                        if let Some(summary) = decode_summary(enc) {
                            msgs.push(ChatMessage {
                                role: "user".into(),
                                content: Some(Value::String(format!(
                                    "[Prior compacted history]\n\n{summary}"
                                ))),
                                reasoning_content: None,
                                tool_calls: None,
                                tool_call_id: None,
                                name: None,
                            });
                        }
                    }
                }
                // function_call / function_call_output / reasoning 等不参与 summary
            }
        }
        _ => {}
    }

    msgs
}

/// content 既支持 string(简单形式)也支持 ContentItem 数组(Responses API)。
/// 把能取出来的文本拼起来。
fn extract_message_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => {
            let mut out = String::new();
            for p in parts {
                let pt = p.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if matches!(pt, "text" | "input_text" | "output_text") {
                    if let Some(t) = p.get("text").and_then(|v| v.as_str()) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(t);
                    }
                } else if pt == "input_image" || pt == "image_url" {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str("[image]");
                }
            }
            out
        }
        _ => String::new(),
    }
}

/// Codex v2 compaction 的 SSE 响应。Codex CLI 的 SSE parser 至少要这套:
///
/// 1. `response.created`:让 stream state 进入 active
/// 2. `response.output_item.done`:带 type=compaction 的 item(实际负载)
/// 3. `response.completed`:必须带 `usage` 字段(`input_tokens/output_tokens/total_tokens`
///    + `*_details: null`),否则 Codex 端 `RemoteCompactionV2Output.token_usage`
///    解析失败、stream 卡住,用户输入 "继续" 没反应就是这种状态
///
/// 参考 codex fixture `tests/common/responses.rs::sse_completed / ev_completed`。
pub(crate) fn build_compaction_sse(
    response_id: &str,
    summary: &str,
    input_tokens: i64,
    output_tokens: i64,
) -> String {
    let encrypted = encode_summary(summary);
    let created_event = json!({
        "type": "response.created",
        "response": { "id": response_id }
    });
    let item_event = json!({
        "type": "response.output_item.done",
        "item": {
            "type": "compaction",
            "encrypted_content": encrypted,
        }
    });
    let completed_event = json!({
        "type": "response.completed",
        "response": {
            "id": response_id,
            "usage": {
                "input_tokens": input_tokens,
                "input_tokens_details": null,
                "output_tokens": output_tokens,
                "output_tokens_details": null,
                "total_tokens": input_tokens + output_tokens,
            }
        }
    });
    format!(
        "event: response.created\n\
         data: {created_event}\n\n\
         event: response.output_item.done\n\
         data: {item_event}\n\n\
         event: response.completed\n\
         data: {completed_event}\n\n",
    )
}

/// 主入口:把 ResponsesRequest 当作 Codex v2 compact 处理,本地做 summary
/// 后返回 SSE。
///
/// 失败时返回 `GatewayError`,上层日志记录 + 让 Codex 看到具体原因。
pub async fn handle_codex_compaction(
    http_client: &reqwest::Client,
    config: &ProviderConfig,
    req: &ResponsesRequest,
    request_id: &str,
    start: Instant,
) -> Result<Response, GatewayError> {
    let history = extract_chat_messages(req);
    if history.is_empty() {
        return Err(GatewayError(AppError::new(
            crate::errors::codes::RESPONSES_PARSE_ERROR,
            "Codex compaction 请求 input 里没有可用的 history messages",
        )));
    }

    // model:Codex 会发 gpt-5.5-openai-compact 之类专属名,我们走当前 provider
    // 的 default_model 才有意义。
    let model = if config.default_model.is_empty() {
        req.model.clone().unwrap_or_else(|| "default".to_string())
    } else {
        config.default_model.clone()
    };

    let transcript = auto_compact::render_transcript(&history);
    if transcript.trim().is_empty() {
        return Err(GatewayError(AppError::new(
            crate::errors::codes::RESPONSES_PARSE_ERROR,
            "Codex compaction transcript 为空",
        )));
    }

    let summary = match auto_compact::summarize_chunk(http_client, config, &model, &transcript).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                provider = %config.name,
                error = %e.message,
                "codex_compact: summary 调用失败"
            );
            return Err(GatewayError(e));
        }
    };

    // 字符数估算 token,Codex 端只看数值是否合理,不验证精确性。
    let input_tokens = (transcript.len() as i64) / 4;
    let output_tokens = (summary.len() as i64) / 4;
    let sse_body = build_compaction_sse(request_id, &summary, input_tokens, output_tokens);
    tracing::info!(
        provider = %config.name,
        request_id = %request_id,
        summary_len = summary.len(),
        history_messages = history.len(),
        latency_ms = start.elapsed().as_millis() as i64,
        "codex_compact: 返回伪 v2 compaction 响应"
    );

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from(sse_body))
        .map_err(|e| {
            GatewayError(AppError::internal(format!(
                "构造 SSE 响应失败: {e}"
            )))
        })?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn h(name: &str, value: &str) -> HeaderMap {
        let mut hm = HeaderMap::new();
        hm.insert(name.parse::<axum::http::HeaderName>().unwrap(),
                  HeaderValue::from_str(value).unwrap());
        hm
    }

    struct EnvGuard;
    impl EnvGuard {
        fn on() -> Self {
            std::env::set_var("AGENTGATE_CODEX_COMPACT", "1");
            EnvGuard
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            std::env::remove_var("AGENTGATE_CODEX_COMPACT");
        }
    }

    #[test]
    fn detect_via_beta_features_header() {
        let _g = EnvGuard::on();
        let hm = h("x-codex-beta-features", "alpha,remote_compaction_v2,beta");
        assert!(is_codex_v2_compaction(&hm, "/v1/responses"));
    }

    #[test]
    fn detect_via_turn_metadata_header() {
        let _g = EnvGuard::on();
        let hm = h(
            "x-codex-turn-metadata",
            r#"{"request_kind":"compaction","other":"x"}"#,
        );
        assert!(is_codex_v2_compaction(&hm, "/v1/responses"));
    }

    #[test]
    fn detect_via_legacy_url_path() {
        let _g = EnvGuard::on();
        let hm = HeaderMap::new();
        assert!(is_codex_v2_compaction(&hm, "/v1/responses/compact"));
    }

    #[test]
    fn not_detected_for_normal_request() {
        let _g = EnvGuard::on();
        let hm = h("x-codex-beta-features", "other_feature");
        assert!(!is_codex_v2_compaction(&hm, "/v1/responses"));
    }

    #[test]
    fn off_by_default_so_all_paths_bypass() {
        // 不设 env:任何 header / URL 组合都不应触发,保留原 chat 路径
        std::env::remove_var("AGENTGATE_CODEX_COMPACT");
        let hm = h("x-codex-beta-features", "remote_compaction_v2");
        assert!(!is_codex_v2_compaction(&hm, "/v1/responses"));
        assert!(!is_codex_v2_compaction(&HeaderMap::new(), "/v1/responses/compact"));
    }

    #[test]
    fn encode_decode_round_trip() {
        let summary = "用户讨论了 Final Cut Pro 调色工作流,选择走 ProRes 4K 交付。";
        let encrypted = encode_summary(summary);
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX));
        let decoded = decode_summary(&encrypted).expect("应能解码");
        assert_eq!(decoded, summary);
    }

    #[test]
    fn decode_returns_none_for_unknown_format() {
        assert!(decode_summary("real_openai_encrypted_token_xyz").is_none());
        assert!(decode_summary("").is_none());
        assert!(decode_summary("AGENTGATE_COMPACT_V1:not-base64!!").is_none());
    }

    #[test]
    fn extract_history_from_message_array() {
        let req = ResponsesRequest {
            instructions: Some("rules".into()),
            input: serde_json::json!([
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "hi"}]
                },
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "hello"}]
                },
                // reasoning / function_call 不参与
                {"type": "reasoning", "id": "r1", "summary": []},
                {"type": "function_call", "name": "f", "arguments": "{}", "call_id": "c1"}
            ]),
            ..Default::default()
        };
        let msgs = extract_chat_messages(&req);
        assert_eq!(msgs.len(), 3); // system(instructions) + user + assistant
        assert_eq!(msgs[0].role, "system");
        assert_eq!(msgs[1].role, "user");
        assert_eq!(msgs[2].role, "assistant");
    }

    #[test]
    fn extract_history_restores_prior_compaction() {
        let prior_summary = "上轮 summary 的内容";
        let req = ResponsesRequest {
            input: serde_json::json!([
                {
                    "type": "compaction",
                    "encrypted_content": encode_summary(prior_summary)
                },
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "继续问"}]
                }
            ]),
            ..Default::default()
        };
        let msgs = extract_chat_messages(&req);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        let restored = msgs[0]
            .content
            .as_ref()
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(restored.contains(prior_summary));
        assert!(restored.contains("Prior compacted history"));
    }

    /// 镜像 codex-rs/codex-api/src/sse/responses.rs 的 deserialize 路径,
    /// 把我们生成的 SSE bytes 跑过同一个 parser,断言每条 event 都被识别
    /// 为有意义的 ResponseEvent variant。任何 None / parse error 都意味着
    /// Codex CLI 也会 silently drop 这条 event,导致 stream 卡住。
    mod codex_parser_compat {
        use serde::Deserialize;
        use serde_json::Value;

        #[derive(Debug, Deserialize)]
        pub(super) struct ResponsesStreamEvent {
            #[serde(rename = "type")]
            pub(super) kind: String,
            pub(super) response: Option<Value>,
            pub(super) item: Option<Value>,
        }

        #[derive(Debug, Deserialize)]
        pub(super) struct ResponseCompleted {
            pub(super) id: String,
            #[serde(default)]
            pub(super) usage: Option<ResponseCompletedUsage>,
        }

        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        pub(super) struct ResponseCompletedUsage {
            pub(super) input_tokens: i64,
            pub(super) input_tokens_details: Option<Value>,
            pub(super) output_tokens: i64,
            pub(super) output_tokens_details: Option<Value>,
            pub(super) total_tokens: i64,
        }

        /// codex `protocol::models::ResponseItem` 的子集——只列我们关心的
        /// variant + Other 兜底。tag/snake_case 等 serde 行为完全跟原版一致。
        #[derive(Debug, Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        #[allow(dead_code)]
        pub(super) enum ResponseItem {
            Message {
                role: String,
            },
            Reasoning {
                #[serde(default)]
                summary: Vec<Value>,
            },
            #[serde(alias = "compaction_summary")]
            Compaction {
                encrypted_content: String,
            },
            CompactionTrigger,
            ContextCompaction {
                #[serde(default)]
                encrypted_content: Option<String>,
            },
            #[serde(other)]
            Other,
        }

        /// 按 SSE wire 格式拆 events。eventsource 协议:`event: <kind>\ndata: <json>\n\n`。
        pub(super) fn split_events(sse: &str) -> Vec<(String, String)> {
            let mut out = Vec::new();
            for chunk in sse.split("\n\n") {
                let mut kind = None;
                let mut data = None;
                for line in chunk.lines() {
                    if let Some(rest) = line.strip_prefix("event: ") {
                        kind = Some(rest.to_string());
                    } else if let Some(rest) = line.strip_prefix("data: ") {
                        data = Some(rest.to_string());
                    }
                }
                if let (Some(k), Some(d)) = (kind, data) {
                    out.push((k, d));
                }
            }
            out
        }
    }

    #[test]
    fn sse_parses_through_mirror_of_codex_parser() {
        use codex_parser_compat::*;

        let sse = build_compaction_sse("resp-test", "this is a summary", 123, 45);
        let events = split_events(&sse);
        assert_eq!(events.len(), 3, "应解出 3 个 event,实际 {}", events.len());

        // 1. response.created — 必须含 response 字段
        let (k0, d0) = &events[0];
        assert_eq!(k0, "response.created");
        let e0: ResponsesStreamEvent =
            serde_json::from_str(d0).expect("response.created data parse 失败");
        assert!(e0.response.is_some(), "created 必须含 response 字段");

        // 2. response.output_item.done — item 必须能 deserialize 成 Compaction variant
        let (k1, d1) = &events[1];
        assert_eq!(k1, "response.output_item.done");
        let e1: ResponsesStreamEvent =
            serde_json::from_str(d1).expect("output_item.done data parse 失败");
        let item_val = e1.item.expect("output_item.done 必须含 item 字段");
        let parsed_item: ResponseItem = serde_json::from_value(item_val.clone())
            .expect(&format!("item 解析 ResponseItem 失败,raw={item_val}"));
        match parsed_item {
            ResponseItem::Compaction { encrypted_content } => {
                assert!(encrypted_content.starts_with(ENCRYPTED_PREFIX));
            }
            other => panic!("item 应是 Compaction variant,实际 {other:?}"),
        }

        // 3. response.completed — usage 字段必须能 deserialize 成 ResponseCompletedUsage
        let (k2, d2) = &events[2];
        assert_eq!(k2, "response.completed");
        let e2: ResponsesStreamEvent =
            serde_json::from_str(d2).expect("response.completed data parse 失败");
        let resp_val = e2.response.expect("completed 必须含 response 字段");
        let completed: ResponseCompleted = serde_json::from_value(resp_val.clone())
            .expect(&format!("ResponseCompleted parse 失败,raw={resp_val}"));
        assert_eq!(completed.id, "resp-test");
        let usage = completed.usage.expect("completed.usage 必须存在");
        assert_eq!(usage.input_tokens, 123);
        assert_eq!(usage.output_tokens, 45);
        assert_eq!(usage.total_tokens, 168);
    }

    #[test]
    fn sse_includes_required_events() {
        let sse = build_compaction_sse("resp-123", "test summary", 100, 50);
        // 3 个事件按顺序出现
        let created_idx = sse.find("event: response.created").expect("created");
        let item_idx = sse.find("event: response.output_item.done").expect("item");
        let done_idx = sse.find("event: response.completed").expect("completed");
        assert!(created_idx < item_idx && item_idx < done_idx);
        assert!(sse.contains("\"type\":\"compaction\""));
        assert!(sse.contains("\"encrypted_content\":\""));
        assert!(sse.contains("\"id\":\"resp-123\""));
        // usage 必填,token_usage parser 会用
        assert!(sse.contains("\"input_tokens\":100"));
        assert!(sse.contains("\"output_tokens\":50"));
        assert!(sse.contains("\"total_tokens\":150"));
        assert!(sse.contains("\"input_tokens_details\":null"));
        // SSE 事件必须用 \n\n 分隔
        assert_eq!(sse.matches("\n\n").count(), 3);
    }
}
