use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use serde_json::{json, Value};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::errors::AppError;
use crate::gateway::sse::SseAccumulator;
use crate::gateway::sse_anthropic::AnthropicSseAccumulator;
use crate::gateway::sse_gemini::GeminiSseAccumulator;
use crate::protocol::chat_completions::{ChatCompletionResponse, ChatMessage};
use crate::protocol::openai_responses::ResponsesRequest;
use crate::providers::adapter::{self, ProviderConfig};
use crate::transform::{responses_to_anthropic, responses_to_chat, responses_to_gemini};

use super::shared::{
    detect_client_from_ua, lock_db, log_request_error, log_request_error_full,
    log_request_success, native_model_override, refine_struct_body, refine_value_body,
    request_contains_images, sanitize_body, trace_with_degradation_events, truncate_str,
    validate_auth, GatewayError,
};
use super::GatewayState;

// ── POST /v1/responses ─────────────────────────────────────────

pub async fn handle_responses(
    headers: HeaderMap,
    AxumState(state): AxumState<GatewayState>,
    body: bytes::Bytes,
) -> Result<Response, GatewayError> {
    validate_auth(&headers)?;
    let start = Instant::now();
    let request_id = format!(
        "req_{}",
        uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string()
    );
    let client_type = detect_client_from_ua(&headers, "Codex");

    // Decompress if needed — Codex.app with `requires_openai_auth = true`
    // gzip-compresses the request body to match the production OpenAI flow.
    let body = crate::gateway::body_decode::decode(&headers, body).map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1/responses",
            &request_id,
            "",
            None,
            &e,
            start.elapsed().as_millis() as i64,
        );
        GatewayError(e)
    })?;

    // 1. Parse request
    let req: ResponsesRequest = serde_json::from_str(&body).map_err(|e| {
        let err = AppError::new(
            crate::errors::codes::RESPONSES_PARSE_ERROR,
            format!("Failed to parse request: {e}"),
        );
        // Log the error
        log_request_error(
            &state.db,
            &client_type,
            "/v1/responses",
            &request_id,
            &sanitize_body(&body),
            None,
            &err,
            start.elapsed().as_millis() as i64,
        );
        err
    })?;

    // 2. Select provider via route profile (with failover candidates)
    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db,
        "openai_responses",
        req.model.as_deref(),
        Some(&req),
    )
    .map_err(|e| {
        log_request_error(
            &state.db,
            &client_type,
            "/v1/responses",
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

    // Derive a stable session-affinity key. Used at two points: candidate
    // reordering (prefer the provider that hit the upstream prompt cache last
    // time) and post-response recording (write affinity when cached_tokens>0).
    let session_id = crate::gateway::session_affinity::derive_from_responses(&req);

    // Detect if request contains images (for vision-aware routing)
    let request_has_images = request_contains_images(&req);

    // 主 provider 优先 + failover 候选 + vision 过滤 + 会话亲和,统一由 failover 模块构建。
    let attempt_order = crate::gateway::failover::build_attempt_order(
        &candidates,
        &selection.provider.id,
        is_failover,
        request_has_images,
        session_id.as_deref(),
    );

    let mut last_error: Option<AppError> = None;
    let mut attempts_trace: Vec<serde_json::Value> = Vec::new();

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
                attempts_trace.push(json!({"provider": &candidate.provider_name, "error": e.message, "attempt": attempt_idx + 1}));
                last_error = Some(e);
                continue;
            }
        };

        let model = candidate.model.clone();

        let result = if config.has_responses_url() {
            // Pass-through: provider has explicit Responses API endpoint
            let target_url = config.responses_url();
            let model_override =
                native_model_override(&provider, req.model.as_deref(), Some(&model));
            crate::gateway::pass_through::handle(
                &state.http_client,
                &state.db,
                &config,
                &target_url,
                "/v1/responses",
                "openai_responses",
                &body,
                model_override.as_deref(),
                &request_id,
                start,
                &client_type,
                Some(&headers),
            )
            .await
            .map_err(|e| GatewayError(e))
        } else if config.is_anthropic() {
            // Claude Messages API conversion (only for Anthropic-type providers)
            // auto_cache_control: default true unless provider explicitly set false
            let auto_cache = provider.auto_cache_control.unwrap_or(true);
            let mut anthropic_body = match responses_to_anthropic::convert(&req, &model, auto_cache)
            {
                Ok(b) => b,
                Err(e) => {
                    attempts_trace.push(json!({"provider": &candidate.provider_name, "error": e.message, "attempt": attempt_idx + 1}));
                    last_error = Some(e);
                    break;
                }
            };
            let _refiner_log = refine_value_body(&state.db, &provider, &mut anthropic_body);
            let converted_json = serde_json::to_string_pretty(&anthropic_body).unwrap_or_default();
            let is_stream = req.stream.unwrap_or(false);
            if is_stream {
                handle_anthropic_stream_response(
                    state.clone(),
                    config.clone(),
                    anthropic_body,
                    request_id.clone(),
                    raw_body.clone(),
                    converted_json,
                    model.clone(),
                    start,
                    client_type.clone(),
                    session_id.clone(),
                    candidate.provider_id.clone(),
                )
                .await
            } else {
                handle_anthropic_non_stream_response(
                    state.clone(),
                    config.clone(),
                    anthropic_body,
                    request_id.clone(),
                    raw_body.clone(),
                    converted_json,
                    model.clone(),
                    start,
                    client_type.clone(),
                    session_id.clone(),
                    candidate.provider_id.clone(),
                )
                .await
            }
        } else if config.is_gemini() {
            // Gemini API conversion
            let mut gemini_body = match responses_to_gemini::convert(&req, &model) {
                Ok(b) => b,
                Err(e) => {
                    attempts_trace.push(json!({"provider": &candidate.provider_name, "error": e.message, "attempt": attempt_idx + 1}));
                    last_error = Some(e);
                    break;
                }
            };
            let _refiner_log = refine_value_body(&state.db, &provider, &mut gemini_body);
            let converted_json = serde_json::to_string_pretty(&gemini_body).unwrap_or_default();
            let is_stream = req.stream.unwrap_or(false);
            if is_stream {
                handle_gemini_stream_response(
                    state.clone(),
                    config.clone(),
                    gemini_body,
                    request_id.clone(),
                    raw_body.clone(),
                    converted_json,
                    model.clone(),
                    start,
                    client_type.clone(),
                    session_id.clone(),
                    candidate.provider_id.clone(),
                )
                .await
            } else {
                handle_gemini_non_stream_response(
                    state.clone(),
                    config.clone(),
                    gemini_body,
                    request_id.clone(),
                    raw_body.clone(),
                    converted_json,
                    model.clone(),
                    start,
                    client_type.clone(),
                    session_id.clone(),
                    candidate.provider_id.clone(),
                )
                .await
            }
        } else {
            // Chat Completions path (default: transform Responses → Chat Completions)
            let provider_transform = crate::transform::providers::for_config(&config);
            // Pull the per-model capability matrix from the underlying provider
            // (re-fetch since ProviderConfig doesn't carry it). Empty map → fall back
            // to legacy "always emit web_search for MiMo" behavior.
            let matrix = {
                let conn = state
                    .db
                    .get()
                    .map_err(|_| GatewayError(AppError::internal("DB lock")))?;
                crate::storage::providers::get_by_id(&conn, &candidate.provider_id)
                    .ok()
                    .and_then(|p| p.model_capabilities)
                    .and_then(|s| {
                        serde_json::from_str::<std::collections::HashMap<String, Vec<String>>>(&s)
                            .ok()
                    })
                    .unwrap_or_default()
            };
            let mut chat_req = match responses_to_chat::convert_with_provider_matrix(
                &req,
                &model,
                provider_transform.as_ref(),
                &matrix,
            ) {
                Ok(r) => r,
                Err(e) => {
                    attempts_trace.push(json!({"provider": &candidate.provider_name, "error": e.message, "attempt": attempt_idx + 1}));
                    last_error = Some(e);
                    break;
                }
            };
            let _refiner_log = refine_struct_body(&state.db, &provider, &mut chat_req);
            let converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
            let is_stream = chat_req.stream;
            if is_stream {
                handle_stream_response(
                    state.clone(),
                    config.clone(),
                    chat_req,
                    request_id.clone(),
                    raw_body.clone(),
                    converted_json,
                    model.clone(),
                    start,
                    client_type.clone(),
                    session_id.clone(),
                    candidate.provider_id.clone(),
                )
                .await
            } else {
                handle_non_stream_response(
                    state.clone(),
                    config.clone(),
                    chat_req,
                    request_id.clone(),
                    raw_body.clone(),
                    converted_json,
                    model.clone(),
                    start,
                    client_type.clone(),
                    session_id.clone(),
                    candidate.provider_id.clone(),
                )
                .await
            }
        };

        match result {
            Ok(response) => {
                // Success — mark provider healthy
                if let Some(conn) = lock_db(&state.db) {
                    let _ = crate::storage::provider_runtime_status::mark_success(
                        &conn,
                        &candidate.provider_id,
                    );
                }
                return Ok(response);
            }
            Err(GatewayError(err)) => {
                // 从 err.message 提取 "Provider returned HTTP {status}" 里的状态码。
                // 之前是从 err.detail（上游 body）扫"HTTP "字串——但 detail 是上游
                // 原始响应（可能是 HTML / SSE 帧 / JSON），不保证含 "HTTP "。adapter.rs
                // 里 message 才是 canonical 的 "Provider returned HTTP 500 ..."，从这里
                // 提取永远靠谱。修这个 bug 后，HTML 错误页等"detail 里没 HTTP 串"的
                // 场景能正确识别状态码，进而触发 5xx failover。
                let status_code = match err.code.as_str() {
                    "UPSTREAM_NON_STREAM_ERROR" | "UPSTREAM_STREAM_ERROR" => {
                        err.message.find("HTTP ").and_then(|i| {
                            err.message[i + 5..]
                                .split_whitespace()
                                .next()?
                                .parse::<u16>()
                                .ok()
                        })
                    }
                    "PROVIDER_REQUEST_FAILED" => Some(502),
                    _ => None,
                };

                attempts_trace.push(json!({
                    "provider": &candidate.provider_name, "attempt": attempt_idx + 1,
                    "error": &err.message, "status": status_code,
                }));

                // Mark failure + cooldown
                if let Some(conn) = lock_db(&state.db) {
                    let _ = crate::storage::provider_runtime_status::mark_failure(
                        &conn,
                        &candidate.provider_id,
                        &err.code,
                        &err.message,
                        candidate.cooldown_seconds,
                    );
                }

                // Check if we should failover
                if is_failover && attempt_idx < attempt_order.len() - 1 {
                    let should = crate::gateway::provider_selector::should_failover(
                        status_code,
                        &err.message,
                        candidate,
                    );
                    if should {
                        last_error = Some(err);
                        continue; // Try next provider
                    }
                }

                // Not retryable or last attempt
                return Err(GatewayError(err));
            }
        }
    }

    // All attempts exhausted
    Err(GatewayError(last_error.unwrap_or_else(|| {
        AppError::new(crate::errors::codes::FAILOVER_EXHAUSTED, "All providers failed")
    })))
}

async fn handle_non_stream_response(
    state: GatewayState,
    config: ProviderConfig,
    mut chat_req: crate::protocol::chat_completions::ChatCompletionsRequest,
    request_id: String,
    raw_request: String,
    mut converted_request: String,
    model: String,
    start: Instant,
    client_type: String,
    session_id: Option<String>,
    provider_id: String,
) -> Result<Response, GatewayError> {
    let result = adapter::send_non_stream(&state.http_client, &config, &mut chat_req).await;

    match result {
        Ok(upstream_json) => {
            converted_request = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
            let resp_id = format!("resp_{}", &request_id[4..]);
            let tool_call_resolution =
                crate::transform::tool_calls::build_tool_call_resolution_map(&raw_request);

            // Parse upstream response
            let chat_resp: ChatCompletionResponse = serde_json::from_value(upstream_json.clone())
                .unwrap_or(ChatCompletionResponse {
                    id: None,
                    choices: None,
                    usage: None,
                });

            // Convert to Responses format
            let mut output = Vec::new();
            let mut tool_calls_json = String::new();

            if let Some(choices) = &chat_resp.choices {
                if choices.is_empty() {
                    // Empty choices — emit a placeholder message so Codex doesn't hang
                    let msg_id = format!("msg_{}", &resp_id.replace("resp_", ""));
                    output.push(json!({
                        "id": msg_id, "type": "message", "status": "completed",
                        "role": "assistant", "content": [{"type": "output_text", "text": ""}]
                    }));
                }
                for choice in choices {
                    if let Some(msg) = &choice.message {
                        let text_content = msg.content.clone().unwrap_or_default();

                        // Store reasoning_content for future multi-turn requests
                        if let Some(ref rc) = msg.reasoning_content {
                            if !rc.is_empty() {
                                let tc_ids: Vec<String> = msg
                                    .tool_calls
                                    .as_ref()
                                    .map(|tcs| tcs.iter().map(|tc| tc.id.clone()).collect())
                                    .unwrap_or_default();
                                crate::transform::reasoning_store::store(
                                    &text_content,
                                    rc,
                                    &tc_ids,
                                );
                            }
                        }

                        // Text content
                        if !text_content.is_empty() {
                            let msg_id = format!("msg_{}", &resp_id.replace("resp_", ""));
                            // Pull web-search annotations from the raw upstream message
                            // (ChatCompletionResponse struct doesn't model them; the
                            // shape is provider-defined and we pass through verbatim).
                            let annotations = upstream_json
                                .get("choices")
                                .and_then(|c| c.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|c| c.get("message"))
                                .and_then(|m| m.get("annotations"))
                                .and_then(|a| a.as_array())
                                .map(|anns| {
                                    crate::protocol::responses_events::normalize_annotations(anns)
                                })
                                .unwrap_or_default();
                            let mut item = json!({
                                "id": msg_id,
                                "type": "message",
                                "status": "completed",
                                "role": "assistant",
                                "content": [{
                                    "type": "output_text",
                                    "text": &text_content,
                                    "annotations": annotations,
                                }]
                            });
                            if let Some(ref rc) = msg.reasoning_content {
                                if !rc.is_empty() {
                                    item["reasoning_content"] = json!(rc);
                                }
                            }
                            output.push(item);
                        }

                        // Tool calls
                        if let Some(ref tcs) = msg.tool_calls {
                            // #5 修复：非流式响应路径也对 arguments 做 JSON 合法性
                            // salvage（与 sse.rs 流式路径对称）。上游偶尔在非流式
                            // 模式下回半截 JSON args（finish_reason="length" 或自身
                            // 截断），原样塞给客户端 → 下轮 history 带病。
                            let finish = choice.finish_reason.as_deref();
                            for tc in tcs {
                                let safe_args =
                                    crate::transform::tool_calls::salvage_tool_arguments(
                                        &tc.function.arguments,
                                        &tc.function.name,
                                        &tc.id,
                                        finish,
                                    );
                                let mut item = responses_tool_call_item_from_chat_name(
                                    &format!("fc_{}", tc.id),
                                    &tc.id,
                                    &tc.function.name,
                                    &safe_args,
                                    &tool_call_resolution,
                                );
                                if let Some(ref rc) = msg.reasoning_content {
                                    if !rc.is_empty() {
                                        item["reasoning_content"] = json!(rc);
                                    }
                                }
                                output.push(item);
                            }
                            tool_calls_json = serde_json::to_string(tcs).unwrap_or_default();
                        }
                    }
                }
            }

            let responses_resp = json!({
                "id": resp_id,
                "object": "response",
                "created_at": chrono::Utc::now().timestamp(),
                "status": "completed",
                "model": model,
                "output": output
            });
            let latency = start.elapsed().as_millis() as i64;

            // Store session for previous_response_id support
            {
                let mut asst_msgs = Vec::new();
                if let Some(choices) = &chat_resp.choices {
                    for choice in choices {
                        if let Some(msg) = &choice.message {
                            asst_msgs.push(ChatMessage {
                                role: "assistant".to_string(),
                                content: msg
                                    .content
                                    .as_ref()
                                    .map(|c| serde_json::Value::String(c.clone())),
                                reasoning_content: msg.reasoning_content.clone(),
                                tool_calls: msg.tool_calls.clone(),
                                tool_call_id: None,
                                name: None,
                            });
                        }
                    }
                }
                crate::gateway::session_store::store_turn(
                    &resp_id,
                    chat_req.messages.clone(),
                    asst_msgs,
                    chat_resp
                        .choices
                        .as_ref()
                        .and_then(|c| c.first())
                        .and_then(|c| c.message.as_ref())
                        .and_then(|m| m.reasoning_content.clone()),
                );
            }

            // Extract token usage from upstream
            let (in_tok, out_tok) = crate::gateway::usage::extract_chat(&upstream_json);
            let (cache_w, cache_r) = chat_resp
                .usage
                .as_ref()
                .map(|u| crate::storage::request_logs::extract_cache_tokens(u))
                .unwrap_or((None, None));

            // Record session affinity if the upstream reported cache hits.
            // Skipped silently when no session_id (short prompts) or usage is absent.
            if let Some(ref sid) = session_id {
                if let Some(usage) = chat_resp.usage.as_ref() {
                    crate::gateway::session_affinity::record_if_cache_hit(sid, &provider_id, usage);
                }
            }

            // Log success
            let trace = trace_with_degradation_events(
                json!({ "response_id": &resp_id, "stream": false }),
                &chat_req.diagnostic_events,
            );
            log_request_success(
                &state.db,
                &client_type,
                "/v1/responses",
                &request_id,
                &raw_request,
                &converted_request,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&responses_resp).unwrap_or_default(),
                if tool_calls_json.is_empty() {
                    None
                } else {
                    Some(&tool_calls_json)
                },
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

            Ok(Json(responses_resp).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            let status = if err.code == "PROVIDER_API_KEY_MISSING" {
                401
            } else if err.code.starts_with("UPSTREAM") {
                502
            } else {
                500
            };

            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/responses",
                &request_id,
                &raw_request,
                &converted_request,
                &config.name,
                &model,
                &err,
                status,
                latency,
            );

            Err(GatewayError(err))
        }
    }
}

fn responses_tool_call_item_from_chat_name(
    item_id: &str,
    call_id: &str,
    chat_name: &str,
    arguments: &str,
    resolution: &crate::transform::tool_calls::ToolCallResolutionMap,
) -> Value {
    match crate::transform::tool_calls::resolve_tool_call_response_kind(chat_name, resolution) {
        crate::transform::tool_calls::ToolCallResponseKind::Function { name, namespace } => {
            let mut item = json!({
                "id": item_id,
                "type": "function_call",
                "status": "completed",
                "call_id": call_id,
                "name": name,
                "arguments": arguments,
            });
            if let Some(ns) = namespace {
                item["namespace"] = json!(ns);
            }
            item
        }
        crate::transform::tool_calls::ToolCallResponseKind::Custom { name } => {
            let input = crate::transform::tool_calls::custom_tool_input_from_arguments(arguments);
            json!({
                "id": item_id,
                "type": "custom_tool_call",
                "status": "completed",
                "call_id": call_id,
                "name": name,
                "input": input,
            })
        }
        crate::transform::tool_calls::ToolCallResponseKind::ToolSearch => {
            let arguments =
                crate::transform::tool_calls::tool_search_arguments_from_arguments(arguments);
            json!({
                "type": "tool_search_call",
                "status": "completed",
                "call_id": call_id,
                "execution": "client",
                "arguments": arguments,
            })
        }
    }
}

async fn handle_stream_response(
    state: GatewayState,
    config: ProviderConfig,
    mut chat_req: crate::protocol::chat_completions::ChatCompletionsRequest,
    request_id: String,
    raw_request: String,
    mut converted_request: String,
    model: String,
    start: Instant,
    client_type: String,
    session_id: Option<String>,
    provider_id: String,
) -> Result<Response, GatewayError> {
    let mut degraded_bootstrap_web_search = false;
    let upstream_resp = loop {
        let upstream_resp = adapter::send_stream(&state.http_client, &config, &mut chat_req).await;
        match upstream_resp {
            Ok(response) => {
                // Bootstrap-validate the upstream stream: read the leading window
                // before any byte reaches the client so HTTP-200-with-error-frame
                // failures become a clean Err. MiMo can report paid web_search
                // plugin failures here, so degrade before opening the client stream.
                match crate::gateway::sse_bootstrap::bootstrap_detect(response).await {
                    Ok(boot) => break Ok(boot),
                    Err(e)
                        if !degraded_bootstrap_web_search
                            && adapter::is_mimo_web_search_disabled_error(&e)
                            && adapter::strip_mimo_web_search_tool(&mut chat_req) =>
                    {
                        adapter::remember_mimo_web_search_disabled(&config);
                        let degraded_model = chat_req.model.clone();
                        chat_req.diagnostic_events.push(
                            crate::transform::degradation::web_search_degraded_event(
                                &config.provider_type,
                                Some(degraded_model.as_str()),
                                "stream_bootstrap_web_search_disabled",
                            ),
                        );
                        converted_request =
                            serde_json::to_string_pretty(&chat_req).unwrap_or_default();
                        degraded_bootstrap_web_search = true;
                        tracing::warn!(
                            provider = %config.name,
                            "MiMo stream reported Web Search Plugin disabled in bootstrap; stripped web_search and retrying once"
                        );
                        continue;
                    }
                    Err(e) => break Err(e),
                }
            }
            Err(err) => break Err(err),
        }
    };

    match upstream_resp {
        Ok(boot) => {
            converted_request = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
            let resp_id = format!("resp_{}", &request_id[4..]);
            let (tx, rx) = mpsc::channel::<String>(256);

            let db = state.db.clone();
            let provider_name = config.name.clone();
            let model_clone = model.clone();
            let req_id = request_id.clone();
            let raw_req = raw_request.clone();
            let conv_req = converted_request.clone();
            let sent_messages = chat_req.messages.clone();
            let diagnostic_events = chat_req.diagnostic_events.clone();
            let sa_session = session_id.clone();
            let sa_provider = provider_id.clone();

            // Spawn task to process upstream SSE and send converted events
            tokio::spawn(async move {
                let mut acc = SseAccumulator::new(resp_id, model_clone.clone());
                acc.tool_call_resolution =
                    crate::transform::tool_calls::build_tool_call_resolution_map(&raw_req);

                let result = crate::gateway::sse::process_upstream_stream_inner(
                    boot,
                    tx.clone(),
                    &mut acc,
                    true,
                    true,
                )
                .await;

                // Record session affinity when the upstream confirmed a cache
                // hit (acc.usage was normalized to the Responses-shape during
                // stream processing — input_tokens_details.cached_tokens etc.).
                if let (Some(ref sid), Some(usage)) = (sa_session.as_ref(), acc.usage.as_ref()) {
                    crate::gateway::session_affinity::record_if_cache_hit(sid, &sa_provider, usage);
                }

                let latency = start.elapsed().as_millis() as i64;
                let tc_list = acc.tool_calls_list();
                let tool_calls_json = if tc_list.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&tc_list.iter().map(|tc| {
                        json!({"id": tc.id, "name": tc.name, "arguments": tc.arguments})
                    }).collect::<Vec<_>>()).unwrap_or_default())
                };

                match result {
                    Ok(()) => {
                        // Store session for previous_response_id
                        {
                            type CM = crate::protocol::chat_completions::ChatMessage;
                            type TC = crate::protocol::chat_completions::ToolCall;
                            type TCF = crate::protocol::chat_completions::ToolCallFunction;
                            let mut asst_msgs: Vec<CM> = vec![];
                            let rc_opt = if acc.reasoning_content.is_empty() {
                                None
                            } else {
                                Some(acc.reasoning_content.clone())
                            };
                            let tcs_opt = if tc_list.is_empty() {
                                None
                            } else {
                                Some(
                                    tc_list
                                        .iter()
                                        .map(|tc| TC {
                                            id: tc.id.clone(),
                                            call_type: "function".to_string(),
                                            function: TCF {
                                                name: tc.name.clone(),
                                                arguments: tc.arguments.clone(),
                                            },
                                        })
                                        .collect(),
                                )
                            };
                            asst_msgs.push(CM {
                                role: "assistant".to_string(),
                                content: if acc.full_text.is_empty() {
                                    None
                                } else {
                                    Some(serde_json::Value::String(acc.full_text.clone()))
                                },
                                reasoning_content: rc_opt.clone(),
                                tool_calls: tcs_opt,
                                tool_call_id: None,
                                name: None,
                            });
                            let rc = if acc.reasoning_content.is_empty() {
                                None
                            } else {
                                Some(acc.reasoning_content.clone())
                            };
                            crate::gateway::session_store::store_turn(
                                &acc.response_id,
                                sent_messages,
                                asst_msgs,
                                rc,
                            );
                        }

                        // Bug #9 修复：trace 加 finish_reason / reasoning_tokens /
                        // truncated 字段，让 `agentgate logs` 能直接看出截断原因
                        // （而不是猜是 max_tokens 还是 AgentGate 自己挂了）。
                        let reasoning_tokens = acc
                            .usage
                            .as_ref()
                            .and_then(|u| u.get("output_tokens_details"))
                            .and_then(|d| d.get("reasoning_tokens"))
                            .and_then(|v| v.as_i64());
                        let truncated = matches!(
                            acc.finish_reason.as_deref(),
                            Some("length") | Some("max_tokens")
                        );
                        let trace = trace_with_degradation_events(
                            serde_json::json!({
                                "response_id": &acc.response_id,
                                "stream": true,
                                "text_len": acc.full_text.len(),
                                "tool_calls_count": tc_list.len(),
                                "reasoning_len": acc.reasoning_content.len(),
                                "finish_reason": acc.finish_reason.as_deref(),
                                "reasoning_tokens": reasoning_tokens,
                                "truncated": truncated,
                            }),
                            &diagnostic_events,
                        );
                        // Extract tokens from SSE usage
                        let (in_tok, out_tok) = acc
                            .usage
                            .as_ref()
                            .map(|u| {
                                (
                                    u.get("input_tokens").and_then(|v| v.as_i64()),
                                    u.get("output_tokens").and_then(|v| v.as_i64()),
                                )
                            })
                            .unwrap_or((None, None));
                        let (cache_w, cache_r) = acc
                            .usage
                            .as_ref()
                            .map(crate::storage::request_logs::extract_cache_tokens)
                            .unwrap_or((None, None));

                        log_request_success(
                            &db,
                            &client_type,
                            "/v1/responses",
                            &req_id,
                            &raw_req,
                            &conv_req,
                            "",
                            &truncate_str(&acc.full_text, 10000),
                            tool_calls_json.as_deref(),
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
                    Err(err_msg) => {
                        let err = AppError::new(crate::errors::codes::UPSTREAM_STREAM_ERROR, &err_msg);
                        log_request_error_full(
                            &db,
                            &client_type,
                            "/v1/responses",
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

            // Return SSE stream response
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
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            let status = if err.code.starts_with("UPSTREAM") {
                502
            } else {
                500
            };
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/responses",
                &request_id,
                &raw_request,
                &converted_request,
                &config.name,
                &model,
                &err,
                status,
                latency,
            );
            Err(GatewayError(err))
        }
    }
}

// ── Anthropic (Claude Messages API) handlers ──────────────────

async fn handle_anthropic_non_stream_response(
    state: GatewayState,
    config: ProviderConfig,
    body: serde_json::Value,
    request_id: String,
    raw_request: String,
    converted_request: String,
    model: String,
    start: Instant,
    client_type: String,
    session_id: Option<String>,
    provider_id: String,
) -> Result<Response, GatewayError> {
    let result = adapter::send_anthropic_non_stream(&state.http_client, &config, &body).await;

    match result {
        Ok(upstream_json) => {
            let resp_id = format!("resp_{}", &request_id[4..]);

            // Parse Claude response: {content: [...], stop_reason, usage}
            let mut output = Vec::new();
            let tool_calls_json = String::new();

            if let Some(content) = upstream_json.get("content").and_then(|c| c.as_array()) {
                let msg_id = format!("msg_{}", &resp_id.replace("resp_", ""));
                let mut text_parts = Vec::new();

                for block in content {
                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match block_type {
                        "text" => {
                            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                text_parts.push(text.to_string());
                            }
                        }
                        "tool_use" => {
                            let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let empty_input = json!({});
                            let input = block.get("input").unwrap_or(&empty_input);
                            let arguments =
                                serde_json::to_string(input).unwrap_or("{}".to_string());
                            output.push(json!({
                                "id": format!("fc_{id}"),
                                "type": "function_call",
                                "status": "completed",
                                "call_id": id,
                                "name": name,
                                "arguments": arguments
                            }));
                        }
                        _ => {}
                    }
                }

                if !text_parts.is_empty() {
                    let full_text = text_parts.join("");
                    output.insert(
                        0,
                        json!({
                            "id": msg_id,
                            "type": "message",
                            "status": "completed",
                            "role": "assistant",
                            "content": [{"type": "output_text", "text": full_text}]
                        }),
                    );
                }
            }

            let responses_resp = json!({
                "id": resp_id,
                "object": "response",
                "created_at": chrono::Utc::now().timestamp(),
                "status": "completed",
                "model": model,
                "output": output
            });
            let latency = start.elapsed().as_millis() as i64;
            let (in_tok, out_tok) = crate::gateway::usage::extract_anthropic(&upstream_json);
            let (cache_w, cache_r) = upstream_json
                .get("usage")
                .map(crate::storage::request_logs::extract_cache_tokens)
                .unwrap_or((None, None));

            // Record session affinity on Anthropic cache_read_input_tokens hit.
            if let Some(ref sid) = session_id {
                if let Some(usage) = upstream_json.get("usage") {
                    crate::gateway::session_affinity::record_if_cache_hit(sid, &provider_id, usage);
                }
            }

            let trace =
                json!({"response_id": &resp_id, "stream": false, "protocol": "anthropic_messages"})
                    .to_string();
            log_request_success(
                &state.db,
                &client_type,
                "/v1/responses",
                &request_id,
                &raw_request,
                &converted_request,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&responses_resp).unwrap_or_default(),
                if tool_calls_json.is_empty() {
                    None
                } else {
                    Some(&tool_calls_json)
                },
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

            Ok(Json(responses_resp).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/responses",
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

async fn handle_anthropic_stream_response(
    state: GatewayState,
    config: ProviderConfig,
    body: serde_json::Value,
    request_id: String,
    raw_request: String,
    converted_request: String,
    model: String,
    start: Instant,
    client_type: String,
    session_id: Option<String>,
    provider_id: String,
) -> Result<Response, GatewayError> {
    let upstream_resp = adapter::send_anthropic_stream(&state.http_client, &config, &body).await;

    match upstream_resp {
        Ok(response) => {
            // Bootstrap-validate before committing to streaming so HTTP-200-
            // with-error-frame failures (Anthropic overload / rate-limit
            // events) become a clean Err that triggers failover.
            let boot = match crate::gateway::sse_bootstrap::bootstrap_detect(response).await {
                Ok(b) => b,
                Err(e) => return Err(GatewayError(e)),
            };

            let resp_id = format!("resp_{}", &request_id[4..]);
            let (tx, rx) = mpsc::channel::<String>(256);

            let db = state.db.clone();
            let provider_name = config.name.clone();
            let model_clone = model.clone();
            let req_id = request_id.clone();
            let raw_req = raw_request.clone();
            let conv_req = converted_request.clone();
            let sa_session = session_id.clone();
            let sa_provider = provider_id.clone();

            tokio::spawn(async move {
                let mut acc = AnthropicSseAccumulator::new(resp_id, model_clone.clone());
                let result =
                    crate::gateway::sse_anthropic::process_anthropic_stream(boot, tx, &mut acc)
                        .await;

                let latency = start.elapsed().as_millis() as i64;
                let tc_list = acc.tool_calls_list();

                if let (Some(ref sid), Some(usage)) = (sa_session.as_ref(), acc.usage.as_ref()) {
                    crate::gateway::session_affinity::record_if_cache_hit(sid, &sa_provider, usage);
                }

                match result {
                    Ok(()) => {
                        // Bug #9 修复：anthropic stream 路径也加观测字段保持一致。
                        // stop_reason 是 Anthropic 自己术语（end_turn/max_tokens/...）。
                        let truncated = matches!(
                            acc.stop_reason.as_deref(),
                            Some("max_tokens") | Some("length")
                        );
                        let trace = json!({
                            "response_id": &acc.response_id, "stream": true, "protocol": "anthropic_messages",
                            "text_len": acc.full_text.len(), "tool_calls_count": tc_list.len(),
                            "reasoning_len": acc.reasoning_content.len(),
                            "stop_reason": acc.stop_reason.as_deref(),
                            "truncated": truncated,
                        }).to_string();
                        let (in_tok, out_tok) = acc
                            .usage
                            .as_ref()
                            .map(|u| {
                                (
                                    u.get("input_tokens").and_then(|v| v.as_i64()),
                                    u.get("output_tokens").and_then(|v| v.as_i64()),
                                )
                            })
                            .unwrap_or((None, None));
                        let (cache_w, cache_r) = acc
                            .usage
                            .as_ref()
                            .map(crate::storage::request_logs::extract_cache_tokens)
                            .unwrap_or((None, None));

                        log_request_success(
                            &db,
                            &client_type,
                            "/v1/responses",
                            &req_id,
                            &raw_req,
                            &conv_req,
                            "",
                            &truncate_str(&acc.full_text, 10000),
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
                    Err(err_msg) => {
                        let err = AppError::new(crate::errors::codes::UPSTREAM_STREAM_ERROR, &err_msg);
                        log_request_error_full(
                            &db,
                            &client_type,
                            "/v1/responses",
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
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/responses",
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

// ── Gemini API handlers ──────────────────────────────────────

async fn handle_gemini_non_stream_response(
    state: GatewayState,
    config: ProviderConfig,
    body: serde_json::Value,
    request_id: String,
    raw_request: String,
    converted_request: String,
    model: String,
    start: Instant,
    client_type: String,
    session_id: Option<String>,
    provider_id: String,
) -> Result<Response, GatewayError> {
    // Gemini's usage object doesn't currently expose a prompt-cache hit
    // counter, so the affinity record below is effectively a no-op today;
    // the params are wired for parity and forward-compat (if Gemini API
    // adds it later, we'll start writing affinity without further plumbing).
    let _ = (&session_id, &provider_id);
    let result = adapter::send_gemini_non_stream(&state.http_client, &config, &body, &model).await;

    match result {
        Ok(upstream_json) => {
            let resp_id = format!("resp_{}", &request_id[4..]);
            let mut output = Vec::new();

            // Parse Gemini response: candidates[0].content.parts[]
            if let Some(candidate) = upstream_json
                .get("candidates")
                .and_then(|c| c.as_array())
                .and_then(|a| a.first())
            {
                let msg_id = format!("msg_{}", &resp_id.replace("resp_", ""));
                let mut text_parts = Vec::new();

                if let Some(parts) = candidate
                    .get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array())
                {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(text.to_string());
                        }
                        if let Some(fc) = part.get("functionCall") {
                            let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let args = fc
                                .get("args")
                                .map(|a| a.to_string())
                                .unwrap_or("{}".to_string());
                            let call_id = format!("call_gemini_{}", output.len());
                            output.push(json!({
                                "id": format!("fc_{call_id}"),
                                "type": "function_call",
                                "status": "completed",
                                "call_id": call_id,
                                "name": name,
                                "arguments": args
                            }));
                        }
                    }
                }

                if !text_parts.is_empty() {
                    let full_text = text_parts.join("");
                    output.insert(
                        0,
                        json!({
                            "id": msg_id,
                            "type": "message",
                            "status": "completed",
                            "role": "assistant",
                            "content": [{"type": "output_text", "text": full_text}]
                        }),
                    );
                }
            }

            let responses_resp = json!({
                "id": resp_id,
                "object": "response",
                "created_at": chrono::Utc::now().timestamp(),
                "status": "completed",
                "model": model,
                "output": output
            });
            let latency = start.elapsed().as_millis() as i64;
            let (in_tok, out_tok) = crate::gateway::usage::extract_gemini(&upstream_json);
            if let Some(ref sid) = session_id {
                if let Some(usage) = upstream_json.get("usageMetadata") {
                    crate::gateway::session_affinity::record_if_cache_hit(sid, &provider_id, usage);
                }
            }
            let trace =
                json!({"response_id": &resp_id, "stream": false, "protocol": "gemini"}).to_string();
            log_request_success(
                &state.db,
                &client_type,
                "/v1/responses",
                &request_id,
                &raw_request,
                &converted_request,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&responses_resp).unwrap_or_default(),
                None,
                &config.name,
                &model,
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
            Ok(Json(responses_resp).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/responses",
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

async fn handle_gemini_stream_response(
    state: GatewayState,
    config: ProviderConfig,
    body: serde_json::Value,
    request_id: String,
    raw_request: String,
    converted_request: String,
    model: String,
    start: Instant,
    client_type: String,
    session_id: Option<String>,
    provider_id: String,
) -> Result<Response, GatewayError> {
    // See note in `handle_gemini_non_stream_response` — params wired for parity.
    let _ = (&session_id, &provider_id);
    let upstream_resp =
        adapter::send_gemini_stream(&state.http_client, &config, &body, &model).await;

    match upstream_resp {
        Ok(response) => {
            // Bootstrap-validate the stream before committing to forwarding.
            // Gemini occasionally returns 200 then immediately emits an error
            // JSON in the first SSE frame (e.g. quota / safety blocks); the
            // scan catches those and routes them through the standard
            // failover path instead of letting the client see a broken stream.
            let boot = match crate::gateway::sse_bootstrap::bootstrap_detect(response).await {
                Ok(b) => b,
                Err(e) => return Err(GatewayError(e)),
            };

            let resp_id = format!("resp_{}", &request_id[4..]);
            let (tx, rx) = mpsc::channel::<String>(256);

            let db = state.db.clone();
            let provider_name = config.name.clone();
            let model_clone = model.clone();
            let req_id = request_id.clone();
            let raw_req = raw_request.clone();
            let conv_req = converted_request.clone();

            tokio::spawn(async move {
                let mut acc = GeminiSseAccumulator::new(resp_id, model_clone.clone());
                let result =
                    crate::gateway::sse_gemini::process_gemini_stream(boot, tx, &mut acc).await;

                let latency = start.elapsed().as_millis() as i64;
                match result {
                    Ok(()) => {
                        let trace = json!({
                            "response_id": &acc.response_id, "stream": true, "protocol": "gemini",
                            "text_len": acc.full_text.len(), "tool_calls_count": acc.tool_calls.len(),
                        }).to_string();
                        let (in_tok, out_tok) = acc
                            .usage
                            .as_ref()
                            .map(|u| {
                                (
                                    u.get("input_tokens").and_then(|v| v.as_i64()),
                                    u.get("output_tokens").and_then(|v| v.as_i64()),
                                )
                            })
                            .unwrap_or((None, None));
                        log_request_success(
                            &db,
                            &client_type,
                            "/v1/responses",
                            &req_id,
                            &raw_req,
                            &conv_req,
                            "",
                            &truncate_str(&acc.full_text, 10000),
                            None,
                            &provider_name,
                            &model_clone,
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
                    }
                    Err(err_msg) => {
                        let err = AppError::new(crate::errors::codes::UPSTREAM_STREAM_ERROR, &err_msg);
                        log_request_error_full(
                            &db,
                            &client_type,
                            "/v1/responses",
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
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(
                &state.db,
                &client_type,
                "/v1/responses",
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

