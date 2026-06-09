use axum::extract::State as AxumState;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Json, Response};
use serde_json::json;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::errors::AppError;
use crate::providers::adapter::{self, ProviderConfig};

use super::shared::{
    anthropic_request_has_images, detect_client_from_ua, lock_db, log_request_error,
    log_request_error_full, log_request_success, native_model_override, refine_struct_body,
    sanitize_body, trace_with_degradation_events, validate_auth, GatewayError,
};
use super::GatewayState;

/// 检测 Claude Code 的自动压缩(/compact)请求。压缩要求模型只回
/// 一段文本摘要、不调工具;命中后给上游关思考 + 去工具,避免 MiMo 的 thinking block /
/// tool_call 污染摘要。这两个串是 Claude Code 压缩 prompt 专属,不会误判普通请求。
fn is_claude_code_compaction(body: &str) -> bool {
    body.contains("You are a helpful AI assistant tasked with summarizing conversations")
        || body.contains("CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.")
}

// ── POST /v1/messages (Anthropic Messages API) ─────────────────

pub async fn handle_messages(
    headers: HeaderMap,
    AxumState(state): AxumState<GatewayState>,
    body: bytes::Bytes,
) -> Result<Response, GatewayError> {
    validate_auth(&headers)?;
    let start = Instant::now();
    let request_id = format!(
        "req_{}",
        &uuid::Uuid::new_v4().to_string().replace('-', "")[..12]
    );
    let client_type = detect_client_from_ua(&headers, "Claude Code");

    let body = crate::gateway::body_decode::decode(&headers, body).map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1/messages",
            &request_id,
            "",
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;

    let requested_model = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(str::to_string));

    // Select provider — try anthropic_messages protocol first, then openai_responses as fallback
    let mut selection = crate::gateway::provider_selector::select_for_failover(
        &state.db,
        "anthropic_messages",
        requested_model.as_deref(),
        None,
    )
    .or_else(|_| {
        crate::gateway::provider_selector::select_for_failover(
            &state.db,
            "openai_responses",
            requested_model.as_deref(),
            None,
        )
    })
    .map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1/messages",
            &request_id,
            &sanitize_body(&body),
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;

    // 带图请求跳过显式不支持 vision 的 provider(与 /v1/responses 对齐)。
    // messages 入口无 failover 循环、直接用 selection.provider,故在此就近用统一排序
    // 选出支持 vision 的候选替换掉选中的 provider/model。
    if anthropic_request_has_images(&body) {
        let is_failover = selection.mode == "failover" && selection.candidates.len() > 1;
        let order = crate::gateway::failover::build_attempt_order(
            &selection.candidates,
            &selection.provider.id,
            is_failover,
            true,
            None,
        );
        if let Some(top) = order.first() {
            if top.provider_id != selection.provider.id {
                let replacement = lock_db(&state.db)
                    .and_then(|conn| crate::storage::providers::get_by_id(&conn, &top.provider_id).ok());
                if let Some(provider) = replacement {
                    selection.model = top.model.clone();
                    selection.provider = provider;
                }
            }
        }
    }

    let config = ProviderConfig::from_provider(&selection.provider).map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1/messages",
            &request_id,
            &sanitize_body(&body),
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;

    let raw = sanitize_body(&body);

    // If provider has anthropic_base_url, pass-through directly (no conversion)
    if config.has_anthropic_url() {
        {
            let target = config.anthropic_messages_url();
            let model_override = native_model_override(
                &selection.provider,
                requested_model.as_deref(),
                Some(&selection.model),
            );
            return crate::gateway::pass_through::handle_anthropic(
                &state.http_client,
                &state.db,
                &config,
                &target,
                &body,
                model_override.as_deref(),
                &request_id,
                start,
                &client_type,
                Some(&headers),
            )
            .await
            .map_err(|e| {
                log_request_error(
                    &state.db,
                    &client_type,
                    "/v1/messages",
                    &request_id,
                    &raw,
                    None,
                    &e,
                    start.elapsed().as_millis() as i64,
                );
                GatewayError(e)
            });
        }
    }

    // No anthropic endpoint — fall back to Messages → Chat Completions transform
    let msg_req: crate::protocol::anthropic_messages::MessagesRequest = serde_json::from_str(&body)
        .map_err(|e| {
            let err = AppError::new(crate::errors::codes::MESSAGES_PARSE_ERROR, format!("Failed to parse: {e}"));
            log_request_error(
                &state.db,
                &client_type,
                "/v1/messages",
                &request_id,
                &raw,
                None,
                &err,
                start.elapsed().as_millis() as i64,
            );
            err
        })?;

    let model = selection.model.clone();
    let messages = crate::protocol::anthropic_messages::to_chat_messages(&msg_req);
    // Anthropic 工具形态 {name, description, input_schema} —— 没有顶层 type，
    // 必须走 anthropic_messages::tools_to_chat，否则 transform::tool_calls::convert_tools
    // 会把整组工具丢弃。
    let tools: Option<Vec<serde_json::Value>> = msg_req
        .tools
        .as_ref()
        .map(|t| crate::protocol::anthropic_messages::tools_to_chat(t, config.is_deepseek()))
        .filter(|t| !t.is_empty());
    // tool_choice 也得翻译：Anthropic {type:"tool",name:"X"} 与 Chat
    // {type:"function",function:{name:"X"}} 不通用；{type:"any"} → "required"。
    let tool_choice = msg_req
        .tool_choice
        .as_ref()
        .map(crate::protocol::anthropic_messages::tool_choice_to_chat);
    // thinking.budget_tokens → reasoning_effort 字符串。Chat 没有真正的 budget 字段，
    // 桶化映射是最接近的等价表达（与 Responses→Anthropic 方向对称）。
    let reasoning_effort = msg_req
        .thinking
        .as_ref()
        .and_then(crate::protocol::anthropic_messages::thinking_to_reasoning_effort);
    let want_stream = msg_req.stream.unwrap_or(false);

    // Claude Code 自动压缩请求:命中则下面关思考 + 去工具,只回干净的摘要文本。
    let is_cc_compaction = is_claude_code_compaction(&body);

    let mut chat_req = crate::protocol::chat_completions::ChatCompletionsRequest {
        model: model.clone(),
        messages,
        tools,
        tool_choice,
        stream: want_stream,
        temperature: msg_req.temperature,
        top_p: msg_req.top_p,
        max_tokens: msg_req.max_tokens,
        max_completion_tokens: msg_req.max_tokens, // 同步透传新字段（C 修复）
        thinking: None,
        // include_usage 必加：默认 Chat stream 不带 usage，client 看 token 都是 0；
        // 加上后终块带完整 usage，message_delta 能正确报 output_tokens。
        stream_options: if want_stream {
            Some(json!({"include_usage": true}))
        } else {
            None
        },
        response_format: None,
        reasoning_effort,
        seed: None,
        stop: None,
        frequency_penalty: None,
        presence_penalty: None,
        parallel_tool_calls: None,
        diagnostic_events: Vec::new(),
    };

    // 压缩请求:关思考 + 去工具,保证上游只回一段干净的摘要文本。
    if is_cc_compaction {
        chat_req.thinking = Some(json!({"type": "disabled"}));
        chat_req.reasoning_effort = None;
        chat_req.tools = None;
        chat_req.tool_choice = None;
    }

    let _refiner_log = refine_struct_body(&state.db, &selection.provider, &mut chat_req);
    let mut converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();

    if want_stream {
        // 真流式：边收上游 Chat SSE chunk 边转 Anthropic 事件、立即转发给 client。
        // 首字延迟 = 上游首字延迟（1-3s 级别），不是上游完整耗时。
        return handle_anthropic_fallback_stream(
            state,
            config,
            chat_req,
            model,
            request_id,
            raw,
            converted_json,
            start,
            client_type,
        )
        .await;
    }

    let result = adapter::send_non_stream(&state.http_client, &config, &mut chat_req).await;
    match result {
        Ok(upstream_json) => {
            converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
            let response =
                crate::protocol::anthropic_messages::from_chat_response(&upstream_json, &model);
            let latency = start.elapsed().as_millis() as i64;
            let (in_tok, out_tok) = crate::gateway::usage::extract_chat(&upstream_json);
            let (cache_w, cache_r) = upstream_json
                .get("usage")
                .map(crate::storage::request_logs::extract_cache_tokens)
                .unwrap_or((None, None));
            let trace = trace_with_degradation_events(
                json!({"mode": "transform", "protocol": "anthropic_messages", "stream": false}),
                &chat_req.diagnostic_events,
            );
            log_request_success(
                &state.db,
                &client_type,
                "/v1/messages",
                &request_id,
                &raw,
                &converted_json,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&response).unwrap_or_default(),
                None,
                &config.name,
                &model,
                200,
                latency,
                Some(&trace),
                crate::gateway::usage::TokenUsage {
                    input: in_tok,
                    output: out_tok,
                    cache_write: cache_w,
                    cache_read: cache_r,
                },
            );
            Ok(Json(response).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/messages",
                &request_id,
                &raw,
                &converted_json,
                &config.name,
                &model,
                &err,
                502,
                latency,
            );
            Err(GatewayError(err))
        }
    }
}

/// Client 用 /v1/messages stream:true，但 provider 没有 anthropic_base_url
/// （只支持 OpenAI Chat Completions）—— 用 ChatToAnthropicStream 增量转换器
/// 把上游 Chat SSE 流逐 chunk 翻译成 Anthropic 事件，**真流式**转发给 client。
async fn handle_anthropic_fallback_stream(
    state: GatewayState,
    config: ProviderConfig,
    mut chat_req: crate::protocol::chat_completions::ChatCompletionsRequest,
    model: String,
    request_id: String,
    raw_request: String,
    mut converted_request: String,
    start: Instant,
    client_type: String,
) -> Result<Response, GatewayError> {
    use futures::StreamExt;

    let upstream = adapter::send_stream(&state.http_client, &config, &mut chat_req)
        .await
        .map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/messages",
                &request_id,
                &raw_request,
                &converted_request,
                &config.name,
                &model,
                &e,
                502,
                latency,
            );
            GatewayError(e)
        })?;
    converted_request = serde_json::to_string_pretty(&chat_req).unwrap_or_default();

    // 用 sse_bootstrap 检查上游首批字节——HTTP 200 + 错误帧的情况能被识别并
    // 转成正常错误回给 client 的 SDK，而不是糊弄它走假流式。
    let boot = crate::gateway::sse_bootstrap::bootstrap_detect(upstream)
        .await
        .map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/messages",
                &request_id,
                &raw_request,
                &converted_request,
                &config.name,
                &model,
                &e,
                502,
                latency,
            );
            GatewayError(e)
        })?;

    let (tx, rx) = mpsc::channel::<String>(512);
    let db = state.db.clone();
    let provider_name = config.name.clone();
    let req_id = request_id.clone();
    let raw_req = raw_request.clone();
    let conv_req = converted_request.clone();
    let model_clone = model.clone();
    let client_type_owned = client_type.clone();
    let diagnostic_events = chat_req.diagnostic_events.clone();

    tokio::spawn(async move {
        use crate::transform::chat_to_anthropic_stream::ChatToAnthropicStream;

        let mut converter = ChatToAnthropicStream::new(model_clone.clone());
        let mut buffer = String::from_utf8_lossy(&boot.prefix).into_owned();
        buffer = buffer.replace("\r\n", "\n");
        let mut stream = boot.stream;
        let mut bootstrap_replayed = false;
        let mut total_text = String::new();
        let mut final_usage_json: Option<serde_json::Value> = None;

        // 解析并 emit 一个 SSE frame。注意要在每个 frame 之间检查 client 是否
        // 还在监听（tx.send Err = receiver drop = client 断开），避免上游浪费。
        async fn handle_frame(
            converter: &mut ChatToAnthropicStream,
            tx: &mpsc::Sender<String>,
            data_str: &str,
            total_text: &mut String,
            final_usage: &mut Option<serde_json::Value>,
        ) -> bool {
            if data_str == "[DONE]" {
                return true; // continue
            }
            let chunk: crate::protocol::chat_completions::ChatCompletionChunk =
                match serde_json::from_str(data_str) {
                    Ok(c) => c,
                    Err(_) => return true,
                };
            // 顺手把可观测信号采集起来（落日志用）
            if let Some(u) = &chunk.usage {
                *final_usage = Some(u.clone());
            }
            if let Some(choices) = &chunk.choices {
                for c in choices {
                    if let Some(d) = &c.delta {
                        if let Some(t) = &d.content {
                            total_text.push_str(t);
                        }
                    }
                }
            }
            for ev in converter.process_chunk(&chunk) {
                if tx.send(ev).await.is_err() {
                    return false;
                }
            }
            true
        }

        loop {
            // 先把 buffer 里完整的 SSE frame 全部处理掉
            while let Some(frame_end) = buffer.find("\n\n") {
                let frame = buffer[..frame_end].to_string();
                buffer = buffer[frame_end + 2..].to_string();
                // 单 frame 内可能多行；只关心 data: 行
                for line in frame.lines() {
                    let trimmed = line.trim_end_matches('\r');
                    if let Some(data) = trimmed.strip_prefix("data:").map(str::trim) {
                        if !handle_frame(
                            &mut converter,
                            &tx,
                            data,
                            &mut total_text,
                            &mut final_usage_json,
                        )
                        .await
                        {
                            return; // client 断开
                        }
                    }
                }
            }

            // 拉更多字节。reqwest 配了 read_timeout(60s)，单次 read 60s 没字节
            // 就会返 timeout error；describe_stream_error 会识别并产出中文文案。
            match stream.next().await {
                Some(Ok(bytes)) => {
                    buffer.push_str(&String::from_utf8_lossy(&bytes));
                    buffer = buffer.replace("\r\n", "\n");
                    bootstrap_replayed = true;
                }
                None => break,
                Some(Err(e)) => {
                    let msg = crate::gateway::sse_bootstrap::describe_stream_error(&e);
                    let payload = format!(
                        "event: error\ndata: {}\n\n",
                        json!({"type": "error", "error": {"type": "upstream_stream_idle", "message": msg}})
                    );
                    let _ = tx.send(payload).await;
                    break;
                }
            }
        }

        // 关流前的收尾事件
        for ev in converter.finalize() {
            if tx.send(ev).await.is_err() {
                break;
            }
        }

        let _ = bootstrap_replayed; // 仅用于潜在 debug，无副作用

        let latency = start.elapsed().as_millis() as i64;
        let (in_tok, out_tok) = final_usage_json
            .as_ref()
            .map(|u| {
                let i = u
                    .get("prompt_tokens")
                    .or_else(|| u.get("input_tokens"))
                    .and_then(|v| v.as_i64());
                let o = u
                    .get("completion_tokens")
                    .or_else(|| u.get("output_tokens"))
                    .and_then(|v| v.as_i64());
                (i, o)
            })
            .unwrap_or((None, None));
        let (cache_w, cache_r) = final_usage_json
            .as_ref()
            .map(crate::storage::request_logs::extract_cache_tokens)
            .unwrap_or((None, None));
        let trace = trace_with_degradation_events(
            json!({"mode": "transform", "protocol": "anthropic_messages", "stream": true}),
            &diagnostic_events,
        );
        log_request_success(
            &db,
            &client_type_owned,
            "/v1/messages",
            &req_id,
            &raw_req,
            &conv_req,
            &final_usage_json
                .map(|u| serde_json::to_string_pretty(&u).unwrap_or_default())
                .unwrap_or_default(),
            &total_text.chars().take(10_000).collect::<String>(),
            None,
            &provider_name,
            &model_clone,
            200,
            latency,
            Some(&trace),
            crate::gateway::usage::TokenUsage {
                input: in_tok,
                output: out_tok,
                cache_write: cache_w,
                cache_read: cache_r,
            },
        );
    });

    let stream = ReceiverStream::new(rx);
    let body = axum::body::Body::from_stream(tokio_stream::StreamExt::map(stream, |s| {
        Ok::<_, std::convert::Infallible>(s)
    }));
    Ok(axum::response::Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
        .header(axum::http::header::CACHE_CONTROL, "no-cache")
        .body(body)
        .unwrap())
}


#[cfg(test)]
mod tests {
    use super::is_claude_code_compaction;

    #[test]
    fn detects_summarizing_system_prompt() {
        let body = r#"{"system":"You are a helpful AI assistant tasked with summarizing conversations.","messages":[]}"#;
        assert!(is_claude_code_compaction(body));
    }

    #[test]
    fn detects_text_only_no_tools_marker() {
        let body = r#"{"messages":[{"role":"user","content":"... CRITICAL: Respond with TEXT ONLY. Do NOT call any tools. ..."}]}"#;
        assert!(is_claude_code_compaction(body));
    }

    #[test]
    fn normal_request_not_flagged() {
        let body = r#"{"system":"You are a coding agent.","messages":[{"role":"user","content":"修个 bug"}]}"#;
        assert!(!is_claude_code_compaction(body));
    }
}
