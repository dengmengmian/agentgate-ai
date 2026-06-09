use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{json, Value};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::errors::AppError;
use crate::models::provider::Provider;
use crate::providers::adapter::{self, ProviderConfig};

use super::shared::{
    chat_request_has_images, detect_client_from_ua, lock_db, log_request_error,
    log_request_error_full, log_request_success, native_model_override, refine_value_body,
    sanitize_body, truncate_str, validate_auth, GatewayError,
};
use super::GatewayState;

// ── POST /v1/chat/completions ──────────────────────────────────

pub async fn handle_chat_completions(
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
    let client_type = detect_client_from_ua(&headers, "Generic");

    let body = crate::gateway::body_decode::decode(&headers, body).map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1/chat/completions",
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

    // Provider 选取：先按 openai_chat_completions 路由 profile 选；选不到再
    // fallback 到 anthropic_messages —— 让只配了 Anthropic 端点的 provider
    // 也能服务 Chat 客户端，下面 anthropic 分支负责协议转换。
    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db,
        "openai_chat_completions",
        requested_model.as_deref(),
        None,
    )
    .or_else(|_| {
        crate::gateway::provider_selector::select_for_failover(
            &state.db,
            "anthropic_messages",
            requested_model.as_deref(),
            None,
        )
    })
    .map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1/chat/completions",
            &request_id,
            &sanitize_body(&body),
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;

    let is_failover = selection.mode == "failover" && selection.candidates.len() > 1;
    let candidates = selection.candidates.clone();
    let raw_body = sanitize_body(&body);

    // 带图请求跳过显式不支持 vision 的 provider(与 /v1/responses 对齐)。
    // 会话亲和暂不在 chat 入口启用(保持既有行为)。
    let request_has_images = chat_request_has_images(&body);
    let attempt_order = crate::gateway::failover::build_attempt_order(
        &candidates,
        &selection.provider.id,
        is_failover,
        request_has_images,
        None,
    );

    let mut last_error: Option<AppError> = None;

    for (attempt_idx, candidate) in attempt_order.iter().enumerate() {
        let provider = {
            let conn = state
                .db
                .get()
                .map_err(|_| GatewayError(AppError::internal("DB lock")))?;
            match crate::storage::providers::get_by_id(&conn, &candidate.provider_id) {
                Ok(p) => p,
                Err(_) => continue,
            }
        };

        let config = match ProviderConfig::from_provider(&provider) {
            Ok(c) => c,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };

        // Chat → Anthropic 转换分支：provider 是 anthropic 且配了 anthropic_base_url。
        // 走 client_chat_to_anthropic_handle 转换请求体、调上游 Anthropic、再把响应/SSE
        // 翻译成 Chat 形态发回。
        if config.is_anthropic() && config.has_anthropic_url() {
            let model_override = native_model_override(
                &provider,
                requested_model.as_deref(),
                Some(&candidate.model),
            );
            let model = model_override.unwrap_or_else(|| candidate.model.clone());
            let result = client_chat_to_anthropic_handle(
                state.clone(),
                config.clone(),
                provider.clone(),
                &body,
                model.clone(),
                request_id.clone(),
                raw_body.clone(),
                start,
                client_type.clone(),
            )
            .await;

            match result {
                Ok(response) => {
                    if let Some(conn) = lock_db(&state.db) {
                        let _ = crate::storage::provider_runtime_status::mark_success(
                            &conn,
                            &candidate.provider_id,
                        );
                    }
                    return Ok(response);
                }
                Err(err) => {
                    if let Some(conn) = lock_db(&state.db) {
                        let _ = crate::storage::provider_runtime_status::mark_failure(
                            &conn,
                            &candidate.provider_id,
                            &err.0.code,
                            &err.0.message,
                            candidate.cooldown_seconds,
                        );
                    }
                    if is_failover && attempt_idx < attempt_order.len() - 1 {
                        if crate::gateway::provider_selector::should_failover(
                            Some(502),
                            &err.0.message,
                            candidate,
                        ) {
                            last_error = Some(err.0);
                            continue;
                        }
                    }
                    return Err(err);
                }
            }
        }

        let decision = match crate::gateway::route_decision::decide(
            "/v1/chat/completions",
            &provider.protocol,
            &config.base_url,
        ) {
            Ok(d) => d,
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        };

        if decision.mode != crate::gateway::route_decision::RouteMode::PassThrough {
            last_error = Some(AppError::new(
                crate::errors::codes::PROTOCOL_TRANSFORM_NOT_SUPPORTED,
                "Not a pass-through provider",
            ));
            continue;
        }

        let model_override = native_model_override(
            &provider,
            requested_model.as_deref(),
            Some(&candidate.model),
        );
        let result = crate::gateway::pass_through::handle(
            &state.http_client,
            &state.db,
            &config,
            &decision.target_url,
            "/v1/chat/completions",
            "openai_chat_completions",
            &body,
            model_override.as_deref(),
            &request_id,
            start,
            &client_type,
            Some(&headers),
        )
        .await;

        match result {
            Ok(response) => {
                if let Some(conn) = lock_db(&state.db) {
                    let _ = crate::storage::provider_runtime_status::mark_success(
                        &conn,
                        &candidate.provider_id,
                    );
                }
                return Ok(response);
            }
            Err(err) => {
                if let Some(conn) = lock_db(&state.db) {
                    let _ = crate::storage::provider_runtime_status::mark_failure(
                        &conn,
                        &candidate.provider_id,
                        &err.code,
                        &err.message,
                        candidate.cooldown_seconds,
                    );
                }
                if is_failover && attempt_idx < attempt_order.len() - 1 {
                    if crate::gateway::provider_selector::should_failover(
                        Some(502),
                        &err.message,
                        candidate,
                    ) {
                        last_error = Some(err);
                        continue;
                    }
                }
                return Err(GatewayError(err));
            }
        }
    }

    Err(GatewayError(last_error.unwrap_or_else(|| {
        AppError::new(
            crate::errors::codes::FAILOVER_EXHAUSTED,
            "All providers failed",
        )
    })))
}

// ── Chat 客户端 + Anthropic provider 协议转换 ──────────────────
//
// Client 发 /v1/chat/completions 但 provider 只有 anthropic_base_url（没 Chat
// 端点）时走这条路径：
// 1. Chat 请求体 → Anthropic Messages 请求体（`chat_to_anthropic::convert`）
// 2. send_anthropic_stream / non_stream 调上游 Anthropic
// 3. 上游 Anthropic 响应 → Chat 响应：
//    - 非流式：`anthropic_to_chat::convert` 一次性转
//    - 流式：`AnthropicToChatStream` 增量转译 SSE 帧
async fn client_chat_to_anthropic_handle(
    state: GatewayState,
    config: ProviderConfig,
    provider: Provider,
    body: &str,
    model: String,
    request_id: String,
    raw_request: String,
    start: Instant,
    client_type: String,
) -> Result<Response, GatewayError> {
    use crate::protocol::chat_completions::ChatCompletionsRequest;

    // 1. 解析 Chat 请求
    let mut chat_req: ChatCompletionsRequest = serde_json::from_str(body).map_err(|e| {
        let err = AppError::new(
            crate::errors::codes::CHAT_PARSE_ERROR,
            format!("Failed to parse chat request: {e}"),
        );
        log_request_error(
            &state.db,
            &client_type,
            "/v1/chat/completions",
            &request_id,
            &raw_request,
            None,
            &err,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(err)
    })?;
    // model_mapping 覆盖
    chat_req.model = model.clone();

    let want_stream = chat_req.stream;

    // 2. Chat → Anthropic 转换
    let mut anthropic_body =
        crate::transform::chat_to_anthropic::convert(&chat_req).map_err(|e| {
            log_request_error(
                &state.db,
                &client_type,
                "/v1/chat/completions",
                &request_id,
                &raw_request,
                None,
                &e,
                start.elapsed().as_millis() as i64,
            );
            GatewayError(e)
        })?;
    // 3. 网关精炼层：开关全关时是 no-op，开了才会按 quirks 改写 outbound body
    let _refiner_log = refine_value_body(&state.db, &provider, &mut anthropic_body);
    let converted_request = serde_json::to_string_pretty(&anthropic_body).unwrap_or_default();

    if want_stream {
        return client_chat_to_anthropic_stream(
            state,
            config,
            anthropic_body,
            model,
            request_id,
            raw_request,
            converted_request,
            start,
            client_type,
        )
        .await;
    }

    // 3. 非流式：发 Anthropic non-stream 请求
    let result =
        adapter::send_anthropic_non_stream(&state.http_client, &config, &anthropic_body).await;
    match result {
        Ok(upstream_json) => {
            let chat_resp = crate::transform::anthropic_to_chat::convert(&upstream_json, &model);
            let latency = start.elapsed().as_millis() as i64;
            // usage 同时含 Anthropic + OpenAI 两形态字段，extract_cache_tokens 都识别
            let (in_tok, out_tok) = (
                chat_resp
                    .get("usage")
                    .and_then(|u| u.get("prompt_tokens"))
                    .and_then(|v| v.as_i64()),
                chat_resp
                    .get("usage")
                    .and_then(|u| u.get("completion_tokens"))
                    .and_then(|v| v.as_i64()),
            );
            let (cache_w, cache_r) = chat_resp
                .get("usage")
                .map(crate::storage::request_logs::extract_cache_tokens)
                .unwrap_or((None, None));
            let trace =
                json!({"mode": "transform", "protocol": "chat_to_anthropic", "stream": false})
                    .to_string();
            log_request_success(
                &state.db,
                &client_type,
                "/v1/chat/completions",
                &request_id,
                &raw_request,
                &converted_request,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&chat_resp).unwrap_or_default(),
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
            Ok(Json(chat_resp).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/chat/completions",
                &request_id,
                &raw_request,
                &converted_request,
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

/// 流式：上游 Anthropic SSE → 增量翻译 → 给客户端发 Chat SSE。
/// 与 handle_anthropic_fallback_stream 镜像对称。
async fn client_chat_to_anthropic_stream(
    state: GatewayState,
    config: ProviderConfig,
    anthropic_body: Value,
    model: String,
    request_id: String,
    raw_request: String,
    converted_request: String,
    start: Instant,
    client_type: String,
) -> Result<Response, GatewayError> {
    use futures::StreamExt;

    let upstream = adapter::send_anthropic_stream(&state.http_client, &config, &anthropic_body)
        .await
        .map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/chat/completions",
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

    // Bootstrap 校验：HTTP 200 + 错误帧的情况能被识别并报错触发 failover
    let boot = crate::gateway::sse_bootstrap::bootstrap_detect(upstream)
        .await
        .map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/chat/completions",
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
    let model_clone = model.clone();
    let req_id = request_id.clone();
    let raw_req = raw_request.clone();
    let conv_req = converted_request.clone();
    let client_type_owned = client_type.clone();

    tokio::spawn(async move {
        use crate::transform::anthropic_to_chat_stream::AnthropicToChatStream;

        let mut converter = AnthropicToChatStream::new(model_clone.clone());
        let mut buffer = String::from_utf8_lossy(&boot.prefix).into_owned();
        buffer = buffer.replace("\r\n", "\n");
        let mut stream = boot.stream;
        let mut bootstrap_replayed = false;
        let mut total_text = String::new();
        let mut final_usage_json: Option<Value> = None;
        let mut stream_err: Option<String> = None;

        loop {
            if bootstrap_replayed {
                let chunk = match stream.next().await {
                    Some(Ok(b)) => b,
                    Some(Err(e)) => {
                        stream_err = Some(crate::gateway::sse_bootstrap::describe_stream_error(&e));
                        break;
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

                let mut event_type = String::new();
                let mut data_str = String::new();
                for line in frame.lines() {
                    if let Some(et) = line.strip_prefix("event:").map(|s| s.trim()) {
                        event_type = et.to_string();
                    } else if let Some(d) = line.strip_prefix("data:").map(|s| s.trim()) {
                        if !data_str.is_empty() {
                            data_str.push('\n');
                        }
                        data_str.push_str(d);
                    }
                }

                if event_type.is_empty() || data_str.is_empty() {
                    continue;
                }

                let data: Value = match serde_json::from_str(&data_str) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                // 累计文本用于 log 日志摘要
                if event_type == "content_block_delta" {
                    if let Some(delta) = data.get("delta") {
                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                            total_text.push_str(text);
                        }
                    }
                }
                // 收尾 usage：message_delta 携带 output_tokens；message_start 携带 input_tokens
                if event_type == "message_delta" {
                    if let Some(u) = data.get("usage") {
                        final_usage_json.get_or_insert_with(|| json!({}));
                        if let Some(obj) = final_usage_json.as_mut().and_then(|u| u.as_object_mut())
                        {
                            if let Some(ot) = u.get("output_tokens") {
                                obj.insert("output_tokens".into(), ot.clone());
                            }
                        }
                    }
                }
                if event_type == "message_start" {
                    if let Some(u) = data.get("message").and_then(|m| m.get("usage")) {
                        final_usage_json.get_or_insert_with(|| json!({}));
                        if let Some(obj) = final_usage_json.as_mut().and_then(|u| u.as_object_mut())
                        {
                            if let Some(it) = u.get("input_tokens") {
                                obj.insert("input_tokens".into(), it.clone());
                            }
                            if let Some(c) = u.get("cache_read_input_tokens") {
                                obj.insert("cache_read_input_tokens".into(), c.clone());
                            }
                            if let Some(c) = u.get("cache_creation_input_tokens") {
                                obj.insert("cache_creation_input_tokens".into(), c.clone());
                            }
                        }
                    }
                }

                let chat_chunks = converter.process_event(&event_type, &data);
                for chunk in chat_chunks {
                    if tx.send(chunk).await.is_err() {
                        // client 断开
                        return;
                    }
                }
            }
        }

        // 收尾
        let final_chunks = converter.finalize();
        for chunk in final_chunks {
            if tx.send(chunk).await.is_err() {
                return;
            }
        }

        let latency = start.elapsed().as_millis() as i64;
        match stream_err {
            None => {
                let trace = json!({
                    "mode": "transform", "protocol": "chat_to_anthropic", "stream": true,
                    "text_len": total_text.len(),
                })
                .to_string();
                let (in_tok, out_tok) = final_usage_json
                    .as_ref()
                    .map(|u| {
                        (
                            u.get("input_tokens").and_then(|v| v.as_i64()),
                            u.get("output_tokens").and_then(|v| v.as_i64()),
                        )
                    })
                    .unwrap_or((None, None));
                let (cache_w, cache_r) = final_usage_json
                    .as_ref()
                    .map(crate::storage::request_logs::extract_cache_tokens)
                    .unwrap_or((None, None));
                log_request_success(
                    &db,
                    &client_type_owned,
                    "/v1/chat/completions",
                    &req_id,
                    &raw_req,
                    &conv_req,
                    "",
                    &truncate_str(&total_text, 10000),
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
            }
            Some(msg) => {
                let err = AppError::new(crate::errors::codes::UPSTREAM_STREAM_ERROR, &msg);
                log_request_error_full(
                    &db,
                    &client_type_owned,
                    "/v1/chat/completions",
                    &req_id,
                    &raw_req,
                    &conv_req,
                    &provider_name,
                    &model_clone,
                    &err,
                    502,
                    latency,
                );
            }
        }
    });

    let stream = ReceiverStream::new(rx);
    let body = Body::from_stream(tokio_stream::StreamExt::map(stream, |s| {
        Ok::<_, std::convert::Infallible>(s)
    }));

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .header(header::CONNECTION, "keep-alive")
        .body(body)
        .unwrap())
}
