use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{json, Value};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::errors::AppError;
use crate::providers::adapter::{self, ProviderConfig};
use crate::transform::gemini_to_chat;

use super::shared::{
    detect_client_from_ua, log_request_error, log_request_error_full, log_request_success,
    native_model_override, refine_struct_body, sanitize_body, trace_with_degradation_events,
    validate_auth, GatewayError,
};
use super::GatewayState;

// ── POST /v1beta/models/:model:generateContent (Gemini CLI input) ──

pub async fn handle_gemini_generate(
    headers: HeaderMap,
    axum::extract::Path(model_path): axum::extract::Path<String>,
    AxumState(state): AxumState<GatewayState>,
    body: bytes::Bytes,
) -> Result<Response, GatewayError> {
    validate_auth(&headers)?;

    // Gemini 路径形如 "gemini-2.5-flash:generateContent" / ":streamGenerateContent"
    // / ":countTokens"。axum 无法在 router 层按 action 分发，handler 入口分流。
    if model_path.ends_with(":countTokens") {
        let body = crate::gateway::body_decode::decode(&headers, body).map_err(GatewayError)?;
        let v: Value = serde_json::from_str(&body).map_err(|e| {
            GatewayError(AppError::new(
                crate::errors::codes::COUNT_TOKENS_PARSE_ERROR,
                format!("Failed to parse: {e}"),
            ))
        })?;
        let mut chars: usize = 0;
        if let Some(contents) = v.get("contents").and_then(|c| c.as_array()) {
            for c in contents {
                if let Some(parts) = c.get("parts").and_then(|p| p.as_array()) {
                    for part in parts {
                        if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                            chars += t.chars().count();
                        }
                    }
                }
            }
        }
        let estimate = ((chars as f64) / 4.0).ceil() as i64;
        return Ok(Json(json!({"totalTokens": estimate})).into_response());
    }

    let start = Instant::now();
    let request_id = format!(
        "req_{}",
        uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string()
    );
    let client_type = detect_client_from_ua(&headers, "Gemini CLI");

    let body = crate::gateway::body_decode::decode(&headers, body).map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1beta/generateContent",
            &request_id,
            "",
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;

    // Extract model name from path (e.g. "gemini-2.5-flash" from "gemini-2.5-flash:generateContent")
    let model_name = model_path
        .split(':')
        .next()
        .unwrap_or(&model_path)
        .to_string();
    let is_stream = model_path.contains("streamGenerateContent");

    let gemini_body: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        let err = AppError::new(
            crate::errors::codes::GEMINI_PARSE_ERROR,
            format!("Failed to parse Gemini request: {e}"),
        );
        log_request_error(
            &state.db,
            &client_type,
            "/v1beta/generateContent",
            &request_id,
            &sanitize_body(&body),
            None,
            &err,
            start.elapsed().as_millis() as i64,
        );
        err
    })?;

    // Select provider (use openai_responses route profile since Gemini CLI is a coding agent)
    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db,
        "openai_responses",
        Some(&model_name),
        None,
    )
    .or_else(|_| {
        crate::gateway::provider_selector::select_for_failover(
            &state.db,
            "openai_chat_completions",
            None,
            None,
        )
    })
    .map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1beta/generateContent",
            &request_id,
            &sanitize_body(&body),
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;

    let config = ProviderConfig::from_provider(&selection.provider).map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1beta/generateContent",
            &request_id,
            &sanitize_body(&body),
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;

    let resolved_model = selection.model.clone();
    let raw_body = sanitize_body(&body);

    // Gemini → Gemini passthrough：选中的 provider 是 google_gemini 原生上游，
    // 直接调上游 generateContent / streamGenerateContent，body 原样转发回 client。
    // 不绕 Chat 转换，避免丢 thinking / grounding / safetySettings 这些 Gemini-only 字段。
    if config.is_gemini() {
        // Override body 里的 model（如有 mapping）
        let model_override = native_model_override(
            &selection.provider,
            Some(&model_name),
            Some(&resolved_model),
        );
        let final_model = model_override.unwrap_or(resolved_model.clone());

        if is_stream {
            let upstream_resp = adapter::send_gemini_stream(
                &state.http_client,
                &config,
                &gemini_body,
                &final_model,
            )
            .await
            .map_err(|e| {
                log_request_error_full(
                    &state.db,
                    &client_type,
                    "/v1beta/generateContent",
                    &request_id,
                    &raw_body,
                    "",
                    &config.name,
                    &final_model,
                    &e,
                    502,
                    start.elapsed().as_millis() as i64,
                );
                GatewayError(e)
            })?;
            let boot = crate::gateway::sse_bootstrap::bootstrap_detect(upstream_resp)
                .await
                .map_err(|e| {
                    log_request_error_full(
                        &state.db,
                        &client_type,
                        "/v1beta/generateContent",
                        &request_id,
                        &raw_body,
                        "",
                        &config.name,
                        &final_model,
                        &e,
                        502,
                        start.elapsed().as_millis() as i64,
                    );
                    GatewayError(e)
                })?;
            let (tx, rx) = mpsc::channel::<String>(256);
            let db = state.db.clone();
            let provider_name = config.name.clone();
            let req_id = request_id.clone();
            let raw_req = raw_body.clone();
            let model_clone = final_model.clone();
            let client_type_owned = client_type.clone();
            tokio::spawn(async move {
                use futures::StreamExt;
                let mut utf8_pending: Vec<u8> = Vec::new();
                let mut prefix_text = String::new();
                crate::gateway::stream_utf8::append_utf8_safe(
                    &mut prefix_text,
                    &mut utf8_pending,
                    &boot.prefix,
                );
                if !prefix_text.is_empty() {
                    let _ = tx.send(prefix_text).await;
                }
                let mut stream = boot.stream;
                // 区分两种提前结束：客户端断开(tx.send 失败)是正常的，请求其实成功了；
                // 上游流 Err 才是真失败，必须记成非 2xx，否则日志层假成功、污染成本/健康统计。
                let mut stream_err: Option<String> = None;
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(b) => {
                            let mut text = String::new();
                            crate::gateway::stream_utf8::append_utf8_safe(
                                &mut text,
                                &mut utf8_pending,
                                &b,
                            );
                            if tx.send(text).await.is_err() {
                                break; // 客户端断开，正常结束
                            }
                        }
                        Err(e) => {
                            stream_err = Some(e.to_string());
                            break;
                        }
                    }
                }
                let latency = start.elapsed().as_millis() as i64;
                let trace =
                    json!({"mode": "native_pass_through", "protocol": "gemini", "stream": true})
                        .to_string();
                if let Some(msg) = stream_err {
                    let err = AppError::new(
                        crate::errors::codes::UPSTREAM_STREAM_ERROR,
                        format!("Gemini 上游流中断: {msg}"),
                    );
                    log_request_error(
                        &db,
                        &client_type_owned,
                        "/v1beta/generateContent",
                        &req_id,
                        &raw_req,
                        None,
                        &err,
                        latency,
                    );
                } else {
                    log_request_success(
                        &db,
                        &client_type_owned,
                        "/v1beta/generateContent",
                        &req_id,
                        &raw_req,
                        "",
                        "",
                        "",
                        None,
                        &provider_name,
                        &model_clone,
                        200,
                        latency,
                        Some(&trace),
                        crate::gateway::usage::TokenUsage::default(),
                    );
                }
            });
            let stream = ReceiverStream::new(rx);
            let body = Body::from_stream(tokio_stream::StreamExt::map(stream, |s| {
                Ok::<_, std::convert::Infallible>(s)
            }));
            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(body)
                .unwrap());
        } else {
            let result = adapter::send_gemini_non_stream(
                &state.http_client,
                &config,
                &gemini_body,
                &final_model,
            )
            .await;
            match result {
                Ok(upstream_json) => {
                    let latency = start.elapsed().as_millis() as i64;
                    let (in_tok, out_tok) = crate::gateway::usage::extract_gemini(&upstream_json);
                    let trace = json!({"mode": "native_pass_through", "protocol": "gemini", "stream": false}).to_string();
                    log_request_success(
                        &state.db,
                        &client_type,
                        "/v1beta/generateContent",
                        &request_id,
                        &raw_body,
                        "",
                        &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                        &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                        None,
                        &config.name,
                        &final_model,
                        200,
                        latency,
                        Some(&trace),
                        crate::gateway::usage::TokenUsage {
                            input: in_tok,
                            output: out_tok,
                            cache_write: None,
                            cache_read: None,
                        },
                    );
                    return Ok(Json(upstream_json).into_response());
                }
                Err(err) => {
                    let latency = start.elapsed().as_millis() as i64;
                    log_request_error_full(
                        &state.db,
                        &client_type,
                        "/v1beta/generateContent",
                        &request_id,
                        &raw_body,
                        "",
                        &config.name,
                        &final_model,
                        &err,
                        502,
                        latency,
                    );
                    return Err(GatewayError(err));
                }
            }
        }
    }

    // Convert Gemini → Chat Completions
    let mut chat_req = gemini_to_chat::convert(&gemini_body, &resolved_model).map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1beta/generateContent",
            &request_id,
            &raw_body,
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;
    chat_req.stream = is_stream;
    if !is_stream {
        chat_req.stream_options = None;
    }

    let _refiner_log = refine_struct_body(&state.db, &selection.provider, &mut chat_req);
    let mut converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();

    if is_stream {
        // Stream: Chat Completions SSE → convert each chunk to Gemini SSE format
        let upstream_resp = adapter::send_stream(&state.http_client, &config, &mut chat_req)
            .await
            .map_err(|e| {
                let latency = start.elapsed().as_millis() as i64;
                log_request_error_full(
                    &state.db,
                    &client_type,
                    "/v1beta/generateContent",
                    &request_id,
                    &raw_body,
                    &converted_json,
                    &config.name,
                    &resolved_model,
                    &e,
                    502,
                    latency,
                );
                GatewayError(e)
            })?;
        converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();

        // Bootstrap-validate the upstream Chat Completions stream before
        // committing to forwarding the converted Gemini SSE back to the client.
        let boot = crate::gateway::sse_bootstrap::bootstrap_detect(upstream_resp)
            .await
            .map_err(|e| {
                let latency = start.elapsed().as_millis() as i64;
                log_request_error_full(
                    &state.db,
                    &client_type,
                    "/v1beta/generateContent",
                    &request_id,
                    &raw_body,
                    &converted_json,
                    &config.name,
                    &resolved_model,
                    &e,
                    502,
                    latency,
                );
                GatewayError(e)
            })?;

        let (tx, rx) = mpsc::channel::<String>(256);
        let db = state.db.clone();
        let provider_name = config.name.clone();
        let model_clone = resolved_model.clone();
        let req_id = request_id.clone();
        let raw_req = raw_body.clone();
        let conv_req = converted_json.clone();
        let diagnostic_events = chat_req.diagnostic_events.clone();

        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = boot.stream;
            let mut utf8_pending: Vec<u8> = Vec::new();
            let mut buffer = String::new();
            crate::gateway::stream_utf8::append_utf8_safe(
                &mut buffer,
                &mut utf8_pending,
                &boot.prefix,
            );
            buffer = buffer.replace("\r\n", "\n");
            let mut full_text = String::new();
            let mut bootstrap_replayed = false;

            loop {
                if bootstrap_replayed {
                    let chunk = match stream.next().await {
                        Some(Ok(b)) => b,
                        Some(Err(e)) => {
                            let msg = crate::gateway::sse_bootstrap::describe_stream_error(&e);
                            let payload = format!(
                                "data: {}\n\n",
                                json!({"error": {"code": 500, "status": "INTERNAL", "message": msg}})
                            );
                            let _ = tx.send(payload).await;
                            break;
                        }
                        None => break,
                    };
                    crate::gateway::stream_utf8::append_utf8_safe(
                        &mut buffer,
                        &mut utf8_pending,
                        &chunk,
                    );
                    buffer = buffer.replace("\r\n", "\n");
                }
                bootstrap_replayed = true;

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim_end_matches('\r').to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }
                    let Some(data) = line.strip_prefix("data:").map(|d| d.trim()) else {
                        continue;
                    };
                    if data == "[DONE]" {
                        break;
                    }

                    if let Ok(chunk_json) = serde_json::from_str::<Value>(data) {
                        // Accumulate text
                        if let Some(delta) = chunk_json
                            .get("choices")
                            .and_then(|c| c.as_array())
                            .and_then(|a| a.first())
                            .and_then(|c| c.get("delta"))
                        {
                            if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                                full_text.push_str(text);
                            }
                        }

                        if let Some(gemini_sse) = gemini_to_chat::chunk_to_gemini(&chunk_json) {
                            let _ = tx.send(gemini_sse).await;
                        }
                    }
                }
            }

            let latency = start.elapsed().as_millis() as i64;
            let trace = trace_with_degradation_events(
                json!({"response_id": &req_id, "stream": true, "protocol": "gemini_input"}),
                &diagnostic_events,
            );
            log_request_success(
                &db,
                &client_type,
                "/v1beta/generateContent",
                &req_id,
                &raw_req,
                &conv_req,
                "",
                &full_text[..full_text.len().min(10000)],
                None,
                &provider_name,
                &model_clone,
                200,
                latency,
                Some(&trace),
                crate::gateway::usage::TokenUsage::default(),
            );
        });

        let stream = ReceiverStream::new(rx);
        let body = Body::from_stream(tokio_stream::StreamExt::map(stream, |s| {
            Ok::<_, std::convert::Infallible>(s)
        }));

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(body)
            .unwrap())
    } else {
        // Non-stream
        chat_req.stream = false;
        let result = adapter::send_non_stream(&state.http_client, &config, &mut chat_req).await;

        match result {
            Ok(upstream_json) => {
                converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
                let gemini_resp =
                    gemini_to_chat::response_to_gemini(&upstream_json, &resolved_model);
                let latency = start.elapsed().as_millis() as i64;
                let (in_tok, out_tok) = crate::gateway::usage::extract_chat(&upstream_json);
                let trace = trace_with_degradation_events(
                    json!({"protocol": "gemini_input"}),
                    &chat_req.diagnostic_events,
                );
                log_request_success(
                    &state.db,
                    &client_type,
                    "/v1beta/generateContent",
                    &request_id,
                    &raw_body,
                    &converted_json,
                    &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                    &serde_json::to_string_pretty(&gemini_resp).unwrap_or_default(),
                    None,
                    &config.name,
                    &resolved_model,
                    200,
                    latency,
                    Some(&trace),
                    crate::gateway::usage::TokenUsage {
                        input: in_tok,
                        output: out_tok,
                        cache_write: None,
                        cache_read: None,
                    },
                );
                Ok(Json(gemini_resp).into_response())
            }
            Err(err) => {
                let latency = start.elapsed().as_millis() as i64;
                log_request_error_full(
                    &state.db,
                    &client_type,
                    "/v1beta/generateContent",
                    &request_id,
                    &raw_body,
                    &converted_json,
                    &config.name,
                    &resolved_model,
                    &err,
                    502,
                    latency,
                );
                Err(GatewayError(err))
            }
        }
    }
}
