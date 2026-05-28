use axum::body::Body;
use axum::extract::State as AxumState;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use axum::http::HeaderMap;

use crate::errors::AppError;
use crate::models::provider::Provider;
use crate::protocol::openai_responses::ResponsesRequest;
use crate::protocol::chat_completions::{ChatCompletionResponse, ChatMessage};
use crate::providers::adapter::{self, ProviderConfig};
use crate::transform::{responses_to_chat, responses_to_anthropic, responses_to_gemini, gemini_to_chat};
use crate::gateway::sse::SseAccumulator;
use crate::gateway::sse_anthropic::AnthropicSseAccumulator;
use crate::gateway::sse_gemini::GeminiSseAccumulator;
use crate::security::local_token;

/// Shared state for the gateway HTTP server.
#[derive(Clone)]
pub struct GatewayState {
    pub db: Arc<Mutex<Connection>>,
    pub http_client: reqwest::Client,
    pub active_requests: Arc<AtomicU64>,
}

// ── GET /health ────────────────────────────────────────────────

pub async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "app": "AgentGate",
        "gateway": "running",
        "version": "0.1.0"
    }))
}

// ── GET /v1/models ─────────────────────────────────────────────

pub async fn list_models(headers: HeaderMap, AxumState(state): AxumState<GatewayState>) -> Result<Json<Value>, GatewayError> {
    validate_auth(&headers)?;
    let provider = get_active_provider(&state.db)?;

    let mut models = vec![json!({
        "id": provider.default_model,
        "object": "model",
        "created": 0,
        "owned_by": "agentgate"
    })];

    if let Some(ref rm) = provider.reasoning_model {
        if !rm.is_empty() {
            models.push(json!({
                "id": rm,
                "object": "model",
                "created": 0,
                "owned_by": "agentgate"
            }));
        }
    }

    Ok(Json(json!({
        "object": "list",
        "data": models
    })))
}

// ── POST /v1/responses ─────────────────────────────────────────

pub async fn handle_responses(
    headers: HeaderMap,
    AxumState(state): AxumState<GatewayState>,
    body: bytes::Bytes,
) -> Result<Response, GatewayError> {
    validate_auth(&headers)?;
    let start = Instant::now();
    let request_id = format!("req_{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string());
    let client_type = detect_client_from_ua(&headers, "Codex");

    // Decompress if needed — Codex.app with `requires_openai_auth = true`
    // gzip-compresses the request body to match the production OpenAI flow.
    let body = crate::gateway::body_decode::decode(&headers, body).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/responses", &request_id, "", None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    // 1. Parse request
    let req: ResponsesRequest = serde_json::from_str(&body).map_err(|e| {
        let err = AppError::new("RESPONSES_PARSE_ERROR", format!("Failed to parse request: {e}"));
        // Log the error
        log_request_error(&state.db, &client_type, "/v1/responses", &request_id, &sanitize_body(&body), None, &err, start.elapsed().as_millis() as i64);
        err
    })?;

    // 2. Select provider via route profile (with failover candidates)
    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db, "openai_responses", req.model.as_deref(), Some(&req),
    ).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/responses", &request_id, &sanitize_body(&body), None, &e, start.elapsed().as_millis() as i64);
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

    // Build ordered list: selected provider first, then remaining candidates
    // Skip providers that don't support vision when request has images
    let mut attempt_order: Vec<&crate::gateway::provider_selector::ProviderCandidate> = Vec::new();
    // Primary
    if let Some(primary) = candidates.iter().find(|c| c.provider_id == selection.provider.id) {
        if !request_has_images || primary.supports_vision != Some(false) {
            attempt_order.push(primary);
        }
    }
    // Remaining (for failover)
    if is_failover {
        for c in &candidates {
            if c.provider_id != selection.provider.id && !c.in_cooldown {
                if request_has_images && c.supports_vision == Some(false) {
                    continue; // Skip providers that explicitly don't support vision
                }
                attempt_order.push(c);
            }
        }
    }
    // If all candidates were skipped (all lack vision), fall back to original order
    if attempt_order.is_empty() {
        if let Some(primary) = candidates.iter().find(|c| c.provider_id == selection.provider.id) {
            attempt_order.push(primary);
        }
        if is_failover {
            for c in &candidates {
                if c.provider_id != selection.provider.id && !c.in_cooldown {
                    attempt_order.push(c);
                }
            }
        }
    }

    // Session affinity: if the previous turn of this conversation hit the
    // upstream prompt cache on a specific provider, move that provider to
    // the front of attempt_order. Skip when the affinity provider isn't in
    // the candidate set or is in cooldown — affinity is a hint, not a pin.
    if let Some(ref sid) = session_id {
        if let Some(entry) = crate::gateway::session_affinity::lookup(sid) {
            if let Some(pos) = attempt_order
                .iter()
                .position(|c| c.provider_id == entry.provider_id && !c.in_cooldown)
            {
                if pos > 0 {
                    let preferred = attempt_order.remove(pos);
                    attempt_order.insert(0, preferred);
                }
            }
        }
    }

    let mut last_error: Option<AppError> = None;
    let mut attempts_trace: Vec<serde_json::Value> = Vec::new();

    for (attempt_idx, candidate) in attempt_order.iter().enumerate() {
        let provider = {
            let conn = state.db.lock().map_err(|_| GatewayError(AppError::internal("DB lock")))?;
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
            crate::gateway::pass_through::handle(
                &state.http_client, &state.db, &config, &target_url, &body, &request_id, start, &client_type,
            ).await.map_err(|e| GatewayError(e))
        } else if config.is_anthropic() {
            // Claude Messages API conversion (only for Anthropic-type providers)
            // auto_cache_control: default true unless provider explicitly set false
            let auto_cache = provider.auto_cache_control.unwrap_or(true);
            let anthropic_body = match responses_to_anthropic::convert(&req, &model, auto_cache) {
                Ok(b) => b,
                Err(e) => {
                    attempts_trace.push(json!({"provider": &candidate.provider_name, "error": e.message, "attempt": attempt_idx + 1}));
                    last_error = Some(e);
                    break;
                }
            };
            let converted_json = serde_json::to_string_pretty(&anthropic_body).unwrap_or_default();
            let is_stream = req.stream.unwrap_or(false);
            if is_stream {
                handle_anthropic_stream_response(state.clone(), config.clone(), anthropic_body, request_id.clone(), raw_body.clone(), converted_json, model.clone(), start, client_type.clone(), session_id.clone(), candidate.provider_id.clone()).await
            } else {
                handle_anthropic_non_stream_response(state.clone(), config.clone(), anthropic_body, request_id.clone(), raw_body.clone(), converted_json, model.clone(), start, client_type.clone(), session_id.clone(), candidate.provider_id.clone()).await
            }
        } else if config.is_gemini() {
            // Gemini API conversion
            let gemini_body = match responses_to_gemini::convert(&req, &model) {
                Ok(b) => b,
                Err(e) => {
                    attempts_trace.push(json!({"provider": &candidate.provider_name, "error": e.message, "attempt": attempt_idx + 1}));
                    last_error = Some(e);
                    break;
                }
            };
            let converted_json = serde_json::to_string_pretty(&gemini_body).unwrap_or_default();
            let is_stream = req.stream.unwrap_or(false);
            if is_stream {
                handle_gemini_stream_response(state.clone(), config.clone(), gemini_body, request_id.clone(), raw_body.clone(), converted_json, model.clone(), start, client_type.clone(), session_id.clone(), candidate.provider_id.clone()).await
            } else {
                handle_gemini_non_stream_response(state.clone(), config.clone(), gemini_body, request_id.clone(), raw_body.clone(), converted_json, model.clone(), start, client_type.clone(), session_id.clone(), candidate.provider_id.clone()).await
            }
        } else {
            // Chat Completions path (default: transform Responses → Chat Completions)
            let provider_transform = crate::transform::providers::for_config(&config);
            // Pull the per-model capability matrix from the underlying provider
            // (re-fetch since ProviderConfig doesn't carry it). Empty map → fall back
            // to legacy "always emit web_search for MiMo" behavior.
            let matrix = {
                let conn = state.db.lock().map_err(|_| GatewayError(AppError::internal("DB lock")))?;
                crate::storage::providers::get_by_id(&conn, &candidate.provider_id)
                    .ok()
                    .and_then(|p| p.model_capabilities)
                    .and_then(|s| serde_json::from_str::<std::collections::HashMap<String, Vec<String>>>(&s).ok())
                    .unwrap_or_default()
            };
            let chat_req = match responses_to_chat::convert_with_provider_matrix(&req, &model, provider_transform.as_ref(), &matrix) {
                Ok(r) => r,
                Err(e) => {
                    attempts_trace.push(json!({"provider": &candidate.provider_name, "error": e.message, "attempt": attempt_idx + 1}));
                    last_error = Some(e);
                    break;
                }
            };
            let converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
            let is_stream = chat_req.stream;
            if is_stream {
                handle_stream_response(state.clone(), config.clone(), chat_req, request_id.clone(), raw_body.clone(), converted_json, model.clone(), start, client_type.clone(), session_id.clone(), candidate.provider_id.clone()).await
            } else {
                handle_non_stream_response(state.clone(), config.clone(), chat_req, request_id.clone(), raw_body.clone(), converted_json, model.clone(), start, client_type.clone(), session_id.clone(), candidate.provider_id.clone()).await
            }
        };

        match result {
            Ok(response) => {
                // Success — mark provider healthy
                if let Some(conn) = lock_db(&state.db) {
                    let _ = crate::storage::provider_runtime_status::mark_success(&conn, &candidate.provider_id);
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
                            err.message[i + 5..].split_whitespace().next()?.parse::<u16>().ok()
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
                        &conn, &candidate.provider_id, &err.code, &err.message, candidate.cooldown_seconds,
                    );
                }

                // Check if we should failover
                if is_failover && attempt_idx < attempt_order.len() - 1 {
                    let should = crate::gateway::provider_selector::should_failover(
                        status_code, &err.message, candidate,
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
    Err(GatewayError(last_error.unwrap_or_else(|| AppError::new("FAILOVER_EXHAUSTED", "All providers failed"))))
}

async fn handle_non_stream_response(
    state: GatewayState,
    config: ProviderConfig,
    chat_req: crate::protocol::chat_completions::ChatCompletionsRequest,
    request_id: String,
    raw_request: String,
    converted_request: String,
    model: String,
    start: Instant,
    client_type: String,
    session_id: Option<String>,
    provider_id: String,
) -> Result<Response, GatewayError> {
    let result = adapter::send_non_stream(&state.http_client, &config, &chat_req).await;

    match result {
        Ok(upstream_json) => {
            let resp_id = format!("resp_{}", &request_id[4..]);

            // Parse upstream response
            let chat_resp: ChatCompletionResponse = serde_json::from_value(upstream_json.clone())
                .unwrap_or(ChatCompletionResponse { id: None, choices: None, usage: None });

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
                                let tc_ids: Vec<String> = msg.tool_calls.as_ref()
                                    .map(|tcs| tcs.iter().map(|tc| tc.id.clone()).collect())
                                    .unwrap_or_default();
                                crate::transform::reasoning_store::store(&text_content, rc, &tc_ids);
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
                                .cloned()
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
                            for tc in tcs {
                                let mut item = json!({
                                    "id": format!("fc_{}", tc.id),
                                    "type": "function_call",
                                    "status": "completed",
                                    "call_id": tc.id,
                                    "name": tc.function.name,
                                    "arguments": tc.function.arguments
                                });
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
                                content: msg.content.as_ref().map(|c| serde_json::Value::String(c.clone())),
                                reasoning_content: msg.reasoning_content.clone(),
                                tool_calls: msg.tool_calls.clone(),
                                tool_call_id: None,
                                name: None,
                            });
                        }
                    }
                }
                crate::gateway::session_store::store_turn(
                    &resp_id, chat_req.messages.clone(), asst_msgs,
                    chat_resp.choices.as_ref().and_then(|c| c.first())
                        .and_then(|c| c.message.as_ref())
                        .and_then(|m| m.reasoning_content.clone()),
                );
            }

            // Extract token usage from upstream
            let (in_tok, out_tok) = extract_usage(&upstream_json);
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
            let trace = json!({ "response_id": &resp_id, "stream": false }).to_string();
            log_request_success(
                &state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&responses_resp).unwrap_or_default(),
                if tool_calls_json.is_empty() { None } else { Some(&tool_calls_json) },
                &config.name, &model, 200, latency, Some(&trace),
                in_tok, out_tok, cache_w, cache_r,
            );

            Ok(Json(responses_resp).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            let status = if err.code == "PROVIDER_API_KEY_MISSING" { 401 }
                else if err.code.starts_with("UPSTREAM") { 502 }
                else { 500 };

            log_request_error_full(&state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &config.name, &model, &err, status, latency);

            Err(GatewayError(err))
        }
    }
}

async fn handle_stream_response(
    state: GatewayState,
    config: ProviderConfig,
    chat_req: crate::protocol::chat_completions::ChatCompletionsRequest,
    request_id: String,
    raw_request: String,
    converted_request: String,
    model: String,
    start: Instant,
    client_type: String,
    session_id: Option<String>,
    provider_id: String,
) -> Result<Response, GatewayError> {
    let upstream_resp = adapter::send_stream(&state.http_client, &config, &chat_req).await;

    match upstream_resp {
        Ok(response) => {
            // Bootstrap-validate the upstream stream: read the leading window
            // before any byte reaches the client so HTTP-200-with-error-frame
            // failures (quota/ban/rate-limit emitted mid-stream by MiMo / GLM
            // / DeepSeek) become a clean Err that triggers failover instead
            // of a half-streamed broken response to the client.
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
            let sent_messages = chat_req.messages.clone();
            let sa_session = session_id.clone();
            let sa_provider = provider_id.clone();

            // Spawn task to process upstream SSE and send converted events
            tokio::spawn(async move {
                let mut acc = SseAccumulator::new(resp_id, model_clone.clone());

                let result = crate::gateway::sse::process_upstream_stream(boot, tx, &mut acc).await;

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
                            let rc_opt = if acc.reasoning_content.is_empty() { None } else { Some(acc.reasoning_content.clone()) };
                            let tcs_opt = if tc_list.is_empty() { None } else {
                                Some(tc_list.iter().map(|tc| TC {
                                    id: tc.id.clone(), call_type: "function".to_string(),
                                    function: TCF { name: tc.name.clone(), arguments: tc.arguments.clone() },
                                }).collect())
                            };
                            asst_msgs.push(CM {
                                role: "assistant".to_string(),
                                content: if acc.full_text.is_empty() { None } else { Some(serde_json::Value::String(acc.full_text.clone())) },
                                reasoning_content: rc_opt.clone(),
                                tool_calls: tcs_opt,
                                tool_call_id: None, name: None,
                            });
                            let rc = if acc.reasoning_content.is_empty() { None } else { Some(acc.reasoning_content.clone()) };
                            crate::gateway::session_store::store_turn(&acc.response_id, sent_messages, asst_msgs, rc);
                        }

                        let trace = serde_json::json!({
                            "response_id": &acc.response_id,
                            "stream": true,
                            "text_len": acc.full_text.len(),
                            "tool_calls_count": tc_list.len(),
                            "reasoning_len": acc.reasoning_content.len(),
                        }).to_string();
                        // Extract tokens from SSE usage
                        let (in_tok, out_tok) = acc.usage.as_ref().map(|u| {
                            (u.get("input_tokens").and_then(|v| v.as_i64()),
                             u.get("output_tokens").and_then(|v| v.as_i64()))
                        }).unwrap_or((None, None));
                        let (cache_w, cache_r) = acc.usage.as_ref()
                            .map(crate::storage::request_logs::extract_cache_tokens)
                            .unwrap_or((None, None));

                        log_request_success(
                            &db, &client_type, "/v1/responses", &req_id, &raw_req, &conv_req,
                            "",
                            &truncate_str(&acc.full_text, 10000),
                            tool_calls_json.as_deref(),
                            &provider_name, &model_clone, 200, latency,
                            Some(&trace), in_tok, out_tok, cache_w, cache_r,
                        );
                    }
                    Err(err_msg) => {
                        let err = AppError::new("UPSTREAM_STREAM_ERROR", &err_msg);
                        log_request_error_full(&db, &client_type, "/v1/responses", &req_id, &raw_req, &conv_req,
                            &provider_name, &model_clone, &err, 502, latency);
                    }
                }
            });

            // Return SSE stream response
            let stream = ReceiverStream::new(rx);
            let body = Body::from_stream(
                tokio_stream::StreamExt::map(stream, |s| Ok::<_, std::convert::Infallible>(s))
            );

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
            let status = if err.code.starts_with("UPSTREAM") { 502 } else { 500 };
            log_request_error_full(&state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &config.name, &model, &err, status, latency);
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
                            let arguments = serde_json::to_string(input).unwrap_or("{}".to_string());
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
                    output.insert(0, json!({
                        "id": msg_id,
                        "type": "message",
                        "status": "completed",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": full_text}]
                    }));
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
            let (in_tok, out_tok) = extract_anthropic_usage(&upstream_json);
            let (cache_w, cache_r) = upstream_json.get("usage")
                .map(crate::storage::request_logs::extract_cache_tokens)
                .unwrap_or((None, None));

            // Record session affinity on Anthropic cache_read_input_tokens hit.
            if let Some(ref sid) = session_id {
                if let Some(usage) = upstream_json.get("usage") {
                    crate::gateway::session_affinity::record_if_cache_hit(sid, &provider_id, usage);
                }
            }

            let trace = json!({"response_id": &resp_id, "stream": false, "protocol": "anthropic_messages"}).to_string();
            log_request_success(
                &state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&responses_resp).unwrap_or_default(),
                if tool_calls_json.is_empty() { None } else { Some(&tool_calls_json) },
                &config.name, &model, 200, latency, Some(&trace), in_tok, out_tok, cache_w, cache_r,
            );

            Ok(Json(responses_resp).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &config.name, &model, &err, 502, latency);
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
                let result = crate::gateway::sse_anthropic::process_anthropic_stream(boot, tx, &mut acc).await;

                let latency = start.elapsed().as_millis() as i64;
                let tc_list = acc.tool_calls_list();

                if let (Some(ref sid), Some(usage)) = (sa_session.as_ref(), acc.usage.as_ref()) {
                    crate::gateway::session_affinity::record_if_cache_hit(sid, &sa_provider, usage);
                }

                match result {
                    Ok(()) => {
                        let trace = json!({
                            "response_id": &acc.response_id, "stream": true, "protocol": "anthropic_messages",
                            "text_len": acc.full_text.len(), "tool_calls_count": tc_list.len(),
                            "reasoning_len": acc.reasoning_content.len(),
                        }).to_string();
                        let (in_tok, out_tok) = acc.usage.as_ref().map(|u| {
                            (u.get("input_tokens").and_then(|v| v.as_i64()),
                             u.get("output_tokens").and_then(|v| v.as_i64()))
                        }).unwrap_or((None, None));
                        let (cache_w, cache_r) = acc.usage.as_ref()
                            .map(crate::storage::request_logs::extract_cache_tokens)
                            .unwrap_or((None, None));

                        log_request_success(
                            &db, &client_type, "/v1/responses", &req_id, &raw_req, &conv_req, "",
                            &truncate_str(&acc.full_text, 10000),
                            None, &provider_name, &model_clone, 200, latency,
                            Some(&trace), in_tok, out_tok, cache_w, cache_r,
                        );
                    }
                    Err(err_msg) => {
                        let err = AppError::new("UPSTREAM_STREAM_ERROR", &err_msg);
                        log_request_error_full(&db, &client_type, "/v1/responses", &req_id, &raw_req, &conv_req,
                            &provider_name, &model_clone, &err, 502, latency);
                    }
                }
            });

            let stream = ReceiverStream::new(rx);
            let body = Body::from_stream(
                tokio_stream::StreamExt::map(stream, |s| Ok::<_, std::convert::Infallible>(s))
            );

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
            log_request_error_full(&state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &config.name, &model, &err, 502, latency);
            Err(GatewayError(err))
        }
    }
}

fn extract_anthropic_usage(upstream: &serde_json::Value) -> (Option<i64>, Option<i64>) {
    let usage = upstream.get("usage");
    let input = usage.and_then(|u| u.get("input_tokens")).and_then(|v| v.as_i64());
    let output = usage.and_then(|u| u.get("output_tokens")).and_then(|v| v.as_i64());
    (input, output)
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
            if let Some(candidate) = upstream_json.get("candidates")
                .and_then(|c| c.as_array())
                .and_then(|a| a.first()) {
                let msg_id = format!("msg_{}", &resp_id.replace("resp_", ""));
                let mut text_parts = Vec::new();

                if let Some(parts) = candidate.get("content")
                    .and_then(|c| c.get("parts"))
                    .and_then(|p| p.as_array()) {
                    for part in parts {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            text_parts.push(text.to_string());
                        }
                        if let Some(fc) = part.get("functionCall") {
                            let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let args = fc.get("args").map(|a| a.to_string()).unwrap_or("{}".to_string());
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
                    output.insert(0, json!({
                        "id": msg_id,
                        "type": "message",
                        "status": "completed",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": full_text}]
                    }));
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
            let (in_tok, out_tok) = extract_gemini_usage(&upstream_json);
            if let Some(ref sid) = session_id {
                if let Some(usage) = upstream_json.get("usageMetadata") {
                    crate::gateway::session_affinity::record_if_cache_hit(sid, &provider_id, usage);
                }
            }
            let trace = json!({"response_id": &resp_id, "stream": false, "protocol": "gemini"}).to_string();
            log_request_success(&state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&responses_resp).unwrap_or_default(),
                None, &config.name, &model, 200, latency, Some(&trace), in_tok, out_tok, None, None);
            Ok(Json(responses_resp).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &config.name, &model, &err, 502, latency);
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
    let upstream_resp = adapter::send_gemini_stream(&state.http_client, &config, &body, &model).await;

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
                let result = crate::gateway::sse_gemini::process_gemini_stream(boot, tx, &mut acc).await;

                let latency = start.elapsed().as_millis() as i64;
                match result {
                    Ok(()) => {
                        let trace = json!({
                            "response_id": &acc.response_id, "stream": true, "protocol": "gemini",
                            "text_len": acc.full_text.len(), "tool_calls_count": acc.tool_calls.len(),
                        }).to_string();
                        let (in_tok, out_tok) = acc.usage.as_ref().map(|u| {
                            (u.get("input_tokens").and_then(|v| v.as_i64()),
                             u.get("output_tokens").and_then(|v| v.as_i64()))
                        }).unwrap_or((None, None));
                        log_request_success(&db, &client_type, "/v1/responses", &req_id, &raw_req, &conv_req, "",
                            &truncate_str(&acc.full_text, 10000),
                            None, &provider_name, &model_clone, 200, latency,
                            Some(&trace), in_tok, out_tok, None, None);
                    }
                    Err(err_msg) => {
                        let err = AppError::new("UPSTREAM_STREAM_ERROR", &err_msg);
                        log_request_error_full(&db, &client_type, "/v1/responses", &req_id, &raw_req, &conv_req,
                            &provider_name, &model_clone, &err, 502, latency);
                    }
                }
            });

            let stream = ReceiverStream::new(rx);
            let body = Body::from_stream(
                tokio_stream::StreamExt::map(stream, |s| Ok::<_, std::convert::Infallible>(s))
            );
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
            log_request_error_full(&state.db, &client_type, "/v1/responses", &request_id, &raw_request, &converted_request,
                &config.name, &model, &err, 502, latency);
            Err(GatewayError(err))
        }
    }
}

fn extract_gemini_usage(upstream: &serde_json::Value) -> (Option<i64>, Option<i64>) {
    let usage = upstream.get("usageMetadata");
    let input = usage.and_then(|u| u.get("promptTokenCount")).and_then(|v| v.as_i64());
    let output = usage.and_then(|u| u.get("candidatesTokenCount")).and_then(|v| v.as_i64());
    (input, output)
}

// ── POST /v1beta/models/:model:generateContent (Gemini CLI input) ──

pub async fn handle_gemini_generate(
    headers: HeaderMap,
    axum::extract::Path(model_path): axum::extract::Path<String>,
    AxumState(state): AxumState<GatewayState>,
    body: bytes::Bytes,
) -> Result<Response, GatewayError> {
    validate_auth(&headers)?;
    let start = Instant::now();
    let request_id = format!("req_{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string());
    let client_type = detect_client_from_ua(&headers, "Gemini CLI");

    let body = crate::gateway::body_decode::decode(&headers, body).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1beta/generateContent", &request_id, "", None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    // Extract model name from path (e.g. "gemini-2.5-flash" from "gemini-2.5-flash:generateContent")
    let model_name = model_path.split(':').next().unwrap_or(&model_path).to_string();
    let is_stream = model_path.contains("streamGenerateContent");

    let gemini_body: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        let err = AppError::new("GEMINI_PARSE_ERROR", format!("Failed to parse Gemini request: {e}"));
        log_request_error(&state.db, &client_type, "/v1beta/generateContent", &request_id, &sanitize_body(&body), None, &err, start.elapsed().as_millis() as i64);
        err
    })?;

    // Select provider (use openai_responses route profile since Gemini CLI is a coding agent)
    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db, "openai_responses", Some(&model_name), None,
    ).or_else(|_| crate::gateway::provider_selector::select_for_failover(
        &state.db, "openai_chat_completions", None, None,
    )).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1beta/generateContent", &request_id, &sanitize_body(&body), None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    let config = ProviderConfig::from_provider(&selection.provider).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1beta/generateContent", &request_id, &sanitize_body(&body), None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    let resolved_model = selection.model.clone();
    let raw_body = sanitize_body(&body);

    // Convert Gemini → Chat Completions
    let mut chat_req = gemini_to_chat::convert(&gemini_body, &resolved_model).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;
    chat_req.stream = is_stream;

    let converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();

    if is_stream {
        // Stream: Chat Completions SSE → convert each chunk to Gemini SSE format
        let upstream_resp = adapter::send_stream(&state.http_client, &config, &chat_req).await.map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, &converted_json, &config.name, &resolved_model, &e, 502, latency);
            GatewayError(e)
        })?;

        // Bootstrap-validate the upstream Chat Completions stream before
        // committing to forwarding the converted Gemini SSE back to the client.
        let boot = crate::gateway::sse_bootstrap::bootstrap_detect(upstream_resp).await.map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, &converted_json, &config.name, &resolved_model, &e, 502, latency);
            GatewayError(e)
        })?;

        let (tx, rx) = mpsc::channel::<String>(256);
        let db = state.db.clone();
        let provider_name = config.name.clone();
        let model_clone = resolved_model.clone();
        let req_id = request_id.clone();
        let raw_req = raw_body.clone();
        let conv_req = converted_json.clone();

        tokio::spawn(async move {
            use futures::StreamExt;
            let mut stream = boot.stream;
            let mut buffer = String::from_utf8_lossy(&boot.prefix).into_owned();
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
                    buffer.push_str(&String::from_utf8_lossy(&chunk));
                    buffer = buffer.replace("\r\n", "\n");
                }
                bootstrap_replayed = true;

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim_end_matches('\r').to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') { continue; }
                    let Some(data) = line.strip_prefix("data:").map(|d| d.trim()) else { continue };
                    if data == "[DONE]" { break; }

                    if let Ok(chunk_json) = serde_json::from_str::<Value>(data) {
                        // Accumulate text
                        if let Some(delta) = chunk_json.get("choices")
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
            let trace = json!({"response_id": &req_id, "stream": true, "protocol": "gemini_input"}).to_string();
            log_request_success(&db, &client_type, "/v1beta/generateContent", &req_id, &raw_req, &conv_req, "", &full_text[..full_text.len().min(10000)],
                None, &provider_name, &model_clone, 200, latency, Some(&trace), None, None, None, None);
        });

        let stream = ReceiverStream::new(rx);
        let body = Body::from_stream(
            tokio_stream::StreamExt::map(stream, |s| Ok::<_, std::convert::Infallible>(s))
        );

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(body)
            .unwrap())
    } else {
        // Non-stream
        chat_req.stream = false;
        let result = adapter::send_non_stream(&state.http_client, &config, &chat_req).await;

        match result {
            Ok(upstream_json) => {
                let gemini_resp = gemini_to_chat::response_to_gemini(&upstream_json, &resolved_model);
                let latency = start.elapsed().as_millis() as i64;
                let (in_tok, out_tok) = extract_usage(&upstream_json);
                let trace = json!({"protocol": "gemini_input"}).to_string();
                log_request_success(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, &converted_json,
                    &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                    &serde_json::to_string_pretty(&gemini_resp).unwrap_or_default(),
                    None, &config.name, &resolved_model, 200, latency, Some(&trace), in_tok, out_tok, None, None);
                Ok(Json(gemini_resp).into_response())
            }
            Err(err) => {
                let latency = start.elapsed().as_millis() as i64;
                log_request_error_full(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, &converted_json,
                    &config.name, &resolved_model, &err, 502, latency);
                Err(GatewayError(err))
            }
        }
    }
}

// ── POST /v1/chat/completions ──────────────────────────────────

pub async fn handle_chat_completions(
    headers: HeaderMap,
    AxumState(state): AxumState<GatewayState>,
    body: bytes::Bytes,
) -> Result<Response, GatewayError> {
    validate_auth(&headers)?;
    let start = Instant::now();
    let request_id = format!("req_{}", &uuid::Uuid::new_v4().to_string().replace('-', "")[..12]);
    let client_type = detect_client_from_ua(&headers, "Generic");

    let body = crate::gateway::body_decode::decode(&headers, body).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/chat/completions", &request_id, "", None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db, "openai_chat_completions", None, None,
    ).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/chat/completions", &request_id, &sanitize_body(&body), None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    let is_failover = selection.mode == "failover" && selection.candidates.len() > 1;
    let candidates = selection.candidates.clone();
    let _raw_body = sanitize_body(&body);

    let mut attempt_order: Vec<&crate::gateway::provider_selector::ProviderCandidate> = Vec::new();
    if let Some(primary) = candidates.iter().find(|c| c.provider_id == selection.provider.id) {
        attempt_order.push(primary);
    }
    if is_failover {
        for c in &candidates {
            if c.provider_id != selection.provider.id && !c.in_cooldown {
                attempt_order.push(c);
            }
        }
    }

    let mut last_error: Option<AppError> = None;

    for (attempt_idx, candidate) in attempt_order.iter().enumerate() {
        let provider = {
            let conn = state.db.lock().map_err(|_| GatewayError(AppError::internal("DB lock")))?;
            match crate::storage::providers::get_by_id(&conn, &candidate.provider_id) {
                Ok(p) => p, Err(_) => continue,
            }
        };

        let config = match ProviderConfig::from_provider(&provider) {
            Ok(c) => c, Err(e) => { last_error = Some(e); continue; }
        };

        let decision = match crate::gateway::route_decision::decide("/v1/chat/completions", &provider.protocol, &config.base_url) {
            Ok(d) => d, Err(e) => { last_error = Some(e); continue; }
        };

        if decision.mode != crate::gateway::route_decision::RouteMode::PassThrough {
            last_error = Some(AppError::new("PROTOCOL_TRANSFORM_NOT_SUPPORTED", "Not a pass-through provider"));
            continue;
        }

        let result = crate::gateway::pass_through::handle(
            &state.http_client, &state.db, &config, &decision.target_url, &body, &request_id, start, &client_type,
        ).await;

        match result {
            Ok(response) => {
                if let Some(conn) = lock_db(&state.db) {
                    let _ = crate::storage::provider_runtime_status::mark_success(&conn, &candidate.provider_id);
                }
                return Ok(response);
            }
            Err(err) => {
                if let Some(conn) = lock_db(&state.db) {
                    let _ = crate::storage::provider_runtime_status::mark_failure(
                        &conn, &candidate.provider_id, &err.code, &err.message, candidate.cooldown_seconds,
                    );
                }
                if is_failover && attempt_idx < attempt_order.len() - 1 {
                    if crate::gateway::provider_selector::should_failover(Some(502), &err.message, candidate) {
                        last_error = Some(err);
                        continue;
                    }
                }
                return Err(GatewayError(err));
            }
        }
    }

    Err(GatewayError(last_error.unwrap_or_else(|| AppError::new("FAILOVER_EXHAUSTED", "All providers failed"))))
}

// ── POST /v1/messages (Anthropic Messages API) ─────────────────

pub async fn handle_messages(
    headers: HeaderMap,
    AxumState(state): AxumState<GatewayState>,
    body: bytes::Bytes,
) -> Result<Response, GatewayError> {
    validate_auth(&headers)?;
    let start = Instant::now();
    let request_id = format!("req_{}", &uuid::Uuid::new_v4().to_string().replace('-', "")[..12]);
    let client_type = detect_client_from_ua(&headers, "Claude Code");

    let body = crate::gateway::body_decode::decode(&headers, body).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/messages", &request_id, "", None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    // Select provider — try anthropic_messages protocol first, then openai_responses as fallback
    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db, "anthropic_messages", None, None,
    ).or_else(|_| crate::gateway::provider_selector::select_for_failover(
        &state.db, "openai_responses", None, None,
    )).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/messages", &request_id, &sanitize_body(&body), None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    let config = ProviderConfig::from_provider(&selection.provider).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/messages", &request_id, &sanitize_body(&body), None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    let raw = sanitize_body(&body);

    // If provider has anthropic_base_url, pass-through directly (no conversion)
    if config.has_anthropic_url() {
        {
            let target = config.anthropic_messages_url();
            return crate::gateway::pass_through::handle_anthropic(
                &state.http_client, &state.db, &config, &target, &body, &request_id, start, &client_type,
            ).await.map_err(|e| {
                log_request_error(&state.db, &client_type, "/v1/messages", &request_id, &raw, None, &e, start.elapsed().as_millis() as i64);
                GatewayError(e)
            });
        }
    }

    // No anthropic endpoint — fall back to Messages → Chat Completions transform
    let msg_req: crate::protocol::anthropic_messages::MessagesRequest = serde_json::from_str(&body).map_err(|e| {
        let err = AppError::new("MESSAGES_PARSE_ERROR", format!("Failed to parse: {e}"));
        log_request_error(&state.db, &client_type, "/v1/messages", &request_id, &raw, None, &err, start.elapsed().as_millis() as i64);
        err
    })?;

    let model = selection.model.clone();
    let messages = crate::protocol::anthropic_messages::to_chat_messages(&msg_req);
    let tools: Option<Vec<serde_json::Value>> = msg_req.tools.as_ref().map(|t| {
        crate::transform::tool_calls::convert_tools(t, config.is_deepseek())
    }).filter(|t| !t.is_empty());

    let chat_req = crate::protocol::chat_completions::ChatCompletionsRequest {
        model: model.clone(), messages, tools,
        tool_choice: msg_req.tool_choice.clone(),
        stream: false, temperature: msg_req.temperature, top_p: msg_req.top_p,
        max_tokens: msg_req.max_tokens, thinking: None, stream_options: None,
        response_format: None, reasoning_effort: None,
        seed: None, stop: None, frequency_penalty: None, presence_penalty: None,
        parallel_tool_calls: None,
    };

    let converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
    let result = adapter::send_non_stream(&state.http_client, &config, &chat_req).await;

    match result {
        Ok(upstream_json) => {
            let response = crate::protocol::anthropic_messages::from_chat_response(&upstream_json, &model);
            let latency = start.elapsed().as_millis() as i64;
            let (in_tok, out_tok) = extract_usage(&upstream_json);
            let (cache_w, cache_r) = upstream_json.get("usage")
                .map(crate::storage::request_logs::extract_cache_tokens)
                .unwrap_or((None, None));
            let trace = json!({"mode": "transform", "protocol": "anthropic_messages"}).to_string();
            log_request_success(&state.db, &client_type, "/v1/messages", &request_id, &raw, &converted_json,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&response).unwrap_or_default(),
                None, &config.name, &model, 200, latency, Some(&trace), in_tok, out_tok, cache_w, cache_r);
            Ok(Json(response).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1/messages", &request_id, &raw, &converted_json,
                &config.name, &model, &err, 502, latency);
            Err(GatewayError(err))
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────

/// Best-effort client identification from the request's User-Agent header.
/// Falls back to a route-default label when UA is empty / unknown so that
/// at least the protocol is conveyed (e.g. Codex is the only common client
/// using /v1/responses today).
///
/// Common patterns:
///   - Codex CLI / desktop:   "OpenAI/Python" or "codex"
///   - Claude Code:           "claude-cli" / "claude-code"
///   - OpenCode:              "opencode"
///   - AtomCode:              "atomcode"
///   - Kimi CLI:              "KimiCLI/1.40.0"
///   - Cursor:                "Cursor/..."
///   - Cherry Studio:         "Cherry-Studio"
///   - Continue.dev:          "continue"
///   - generic SDKs:          "Python/requests", "node-fetch", "axios", etc.
pub(crate) fn detect_client_from_ua(headers: &HeaderMap, route_default: &str) -> String {
    let ua = headers.get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .trim();
    if ua.is_empty() {
        return route_default.to_string();
    }
    let lower = ua.to_ascii_lowercase();
    // Order matters: more specific matches first.
    if lower.contains("claude-code") || lower.contains("claude-cli") || lower.contains("claude code") {
        return "Claude Code".to_string();
    }
    if lower.contains("codex-cli") || lower.starts_with("codex/") {
        return "Codex".to_string();
    }
    if lower.contains("opencode") { return "OpenCode".to_string(); }
    if lower.contains("atomcode") { return "AtomCode".to_string(); }
    if lower.contains("kimicli") || lower.contains("kimi-cli") || lower.contains("kimi cli") {
        return "Kimi CLI".to_string();
    }
    if lower.contains("cursor") { return "Cursor".to_string(); }
    if lower.contains("cherry") { return "Cherry Studio".to_string(); }
    if lower.contains("continue") { return "Continue".to_string(); }
    if lower.contains("cline") { return "Cline".to_string(); }
    if lower.contains("roo") { return "Roo Code".to_string(); }
    if lower.contains("hermes") { return "Hermes".to_string(); }
    if lower.contains("opencode") { return "OpenCode".to_string(); }
    if lower.starts_with("openai/") || lower.contains("openai-python") {
        // Codex CLI desktop reports "OpenAI/Python ..." too; treat as Codex
        // when the route is the Responses API.
        if route_default == "Codex" { return "Codex".to_string(); }
        return "OpenAI SDK".to_string();
    }
    if lower.contains("anthropic-sdk") || lower.starts_with("anthropic/") {
        return "Anthropic SDK".to_string();
    }
    if lower.starts_with("python") || lower.contains("python-requests") || lower.contains("httpx") {
        return "Python SDK".to_string();
    }
    if lower.starts_with("node") || lower.contains("node-fetch") || lower.contains("axios") || lower.contains("undici") {
        return "Node SDK".to_string();
    }
    if lower.starts_with("curl") { return "curl".to_string(); }
    // Unknown — surface the raw first token (helps users identify new clients)
    let token: String = ua.split_whitespace().next().unwrap_or(ua).chars().take(40).collect();
    if token.is_empty() { route_default.to_string() } else { token }
}

pub(crate) fn validate_auth(headers: &HeaderMap) -> Result<(), GatewayError> {
    // 1. Try standard Authorization: Bearer <token>
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let (token, source) = if auth_header.is_empty() {
        // 2. Fallback to x-api-key (used by some Anthropic SDK versions / Claude Code)
        let x_api_key = headers
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        (x_api_key, "x-api-key")
    } else {
        (auth_header.strip_prefix("Bearer ").unwrap_or(auth_header), "authorization")
    };

    if token.is_empty() {
        return Err(GatewayError(AppError::new(
            "GATEWAY_AUTH_MISSING",
            "Gateway access token is missing",
        ).with_detail("The request does not include Authorization: Bearer <token> or X-Api-Key <token>")
         .with_suggestion("Re-apply the tool configuration from AgentGate or check the token file")));
    }

    if !local_token::validate_token(token) {
        return Err(GatewayError(AppError::new(
            "GATEWAY_AUTH_INVALID",
            "Gateway access token is invalid",
        ).with_suggestion(format!("Token received via '{source}' header does not match. Regenerate the token and re-apply tool configuration"))));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{FS_LOCK, setup_temp_home, cleanup};

    #[test]
    fn test_validate_auth_missing() {
        let headers = HeaderMap::new();
        let err = validate_auth(&headers).unwrap_err();
        assert_eq!(err.0.code, "GATEWAY_AUTH_MISSING");
    }

    #[test]
    fn test_validate_auth_invalid() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let _ = local_token::ensure_token().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer wrong_token".parse().unwrap());
        let err = validate_auth(&headers).unwrap_err();
        assert_eq!(err.0.code, "GATEWAY_AUTH_INVALID");
        cleanup(&temp);
    }

    #[test]
    fn test_validate_auth_valid() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let token = local_token::ensure_token().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", format!("Bearer {token}").parse().unwrap());
        assert!(validate_auth(&headers).is_ok());
        cleanup(&temp);
    }

    #[test]
    fn test_validate_auth_no_bearer_prefix() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let token = local_token::ensure_token().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("authorization", token.parse().unwrap());
        assert!(validate_auth(&headers).is_ok());
        cleanup(&temp);
    }

    #[test]
    fn test_validate_auth_x_api_key_valid() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let token = local_token::ensure_token().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", token.parse().unwrap());
        assert!(validate_auth(&headers).is_ok());
        cleanup(&temp);
    }

    #[test]
    fn test_validate_auth_x_api_key_invalid() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let _ = local_token::ensure_token().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", "wrong_token".parse().unwrap());
        let err = validate_auth(&headers).unwrap_err();
        assert_eq!(err.0.code, "GATEWAY_AUTH_INVALID");
        assert!(err.0.suggestion.as_ref().unwrap().contains("x-api-key"));
        cleanup(&temp);
    }

    // ── truncate_str tests ──

    #[test]
    fn test_truncate_str_ascii() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_str_chinese() {
        let s = "你好世界测试";
        // Each Chinese char is 3 bytes. "你好" = 6 bytes.
        // Truncate at 7 should land inside "世" → snap back to 6
        assert_eq!(truncate_str(s, 7), "你好");
        assert_eq!(truncate_str(s, 6), "你好");
        assert_eq!(truncate_str(s, 100), s);
    }

    #[test]
    fn test_truncate_str_emoji() {
        let s = "hello 🎉 world";
        // 🎉 is 4 bytes at position 6..10
        assert_eq!(truncate_str(s, 7), "hello "); // snap back before emoji
        assert_eq!(truncate_str(s, 10), "hello 🎉");
    }

    // ── sanitize_body tests ──

    #[test]
    fn test_sanitize_body_redacts_keys() {
        let body = r#"{"key": "sk-abcdefghij1234567890"}"#;
        let sanitized = sanitize_body(body);
        assert!(!sanitized.contains("abcdefghij1234567890"));
        assert!(sanitized.contains("sk-****"));
    }

    #[test]
    fn test_sanitize_body_multiple_keys() {
        let body = r#"sk-firstkeyvalue sk-secondkeyvalue"#;
        let sanitized = sanitize_body(body);
        assert_eq!(sanitized.matches("sk-****").count(), 2);
    }

    #[test]
    fn test_sanitize_body_short_sk_not_redacted() {
        let body = "sk-short";
        let sanitized = sanitize_body(body);
        assert_eq!(sanitized, "sk-short");
    }

    // ── GatewayError format tests ──

    #[test]
    fn test_gateway_error_has_type_field() {
        let err = GatewayError(AppError::new("UPSTREAM_STREAM_ERROR", "Provider failed")
            .with_detail("HTTP 502"));
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn test_gateway_error_status_mapping() {
        assert_eq!(
            GatewayError(AppError::new("RESPONSES_PARSE_ERROR", "bad")).into_response().status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            GatewayError(AppError::new("PROVIDER_API_KEY_MISSING", "no key")).into_response().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            GatewayError(AppError::new("ACTIVE_PROVIDER_NOT_FOUND", "none")).into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            GatewayError(AppError::new("UNKNOWN_CODE", "wat")).into_response().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_gateway_error_auth_status_codes() {
        assert_eq!(
            GatewayError(AppError::new("GATEWAY_AUTH_MISSING", "no auth")).into_response().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            GatewayError(AppError::new("GATEWAY_AUTH_INVALID", "bad token")).into_response().status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn test_lock_db_normal() {
        let conn = Connection::open_in_memory().unwrap();
        let db = Arc::new(Mutex::new(conn));
        assert!(lock_db(&db).is_some());
    }

    #[test]
    fn test_lock_db_recovers_from_poison() {
        let conn = Connection::open_in_memory().unwrap();
        let db = Arc::new(Mutex::new(conn));
        // Poison the mutex by panicking while holding the lock
        let db2 = db.clone();
        let _ = std::thread::spawn(move || {
            let _guard = db2.lock().unwrap();
            panic!("intentional panic to poison mutex");
        }).join();
        // Mutex is now poisoned — lock_db should recover
        assert!(db.lock().is_err(), "Mutex should be poisoned");
        assert!(lock_db(&db).is_some(), "lock_db should recover from poisoned mutex");
    }

    #[test]
    fn test_detect_client_from_ua_empty() {
        let headers = HeaderMap::new();
        assert_eq!(detect_client_from_ua(&headers, "Default"), "Default");
    }

    #[test]
    fn test_detect_client_from_ua_claude_code() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "claude-code/0.1.0".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Default"), "Claude Code");
    }

    #[test]
    fn test_detect_client_from_ua_codex() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "codex-cli/1.0".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Default"), "Codex");
    }

    #[test]
    fn test_detect_client_from_ua_openai_sdk() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "openai-python/1.0".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Default"), "OpenAI SDK");
    }

    #[test]
    fn test_detect_client_from_ua_openai_sdk_codex_route() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "openai-python/1.0".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Codex"), "Codex");
    }

    #[test]
    fn test_detect_client_from_ua_cursor() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "Cursor/1.0".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Default"), "Cursor");
    }

    #[test]
    fn test_detect_client_from_ua_python_sdk() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "python-requests/2.28".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Default"), "Python SDK");
    }

    #[test]
    fn test_detect_client_from_ua_node_sdk() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "node-fetch/1.0".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Default"), "Node SDK");
    }

    #[test]
    fn test_detect_client_from_ua_curl() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "curl/7.64.1".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Default"), "curl");
    }

    #[test]
    fn test_detect_client_from_ua_unknown() {
        let mut headers = HeaderMap::new();
        headers.insert("user-agent", "MyCustomAgent/1.0".parse().unwrap());
        assert_eq!(detect_client_from_ua(&headers, "Default"), "MyCustomAgent/1.0");
    }

    fn responses_req_with_input(input: serde_json::Value) -> ResponsesRequest {
        ResponsesRequest {
            model: Some("gpt-5".into()),
            input,
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: Some(false),
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            parallel_tool_calls: None,
            reasoning: None,
            text: None,
            metadata: None,
            seed: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn request_contains_images_picks_up_historic_image() {
        // Image is in an EARLIER user turn; the most recent user turn is text-only.
        // Prior to the fix this returned false and MiMo got `image_url` in the
        // converted body without promotion → 404 from upstream.
        let req = responses_req_with_input(json!([
            {"type": "message", "role": "user", "content": [
                {"type": "input_image", "image_url": {"url": "https://example.com/x.png"}}
            ]},
            {"type": "message", "role": "assistant", "content": "I see a cat."},
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "what color is it?"}
            ]}
        ]));
        assert!(request_contains_images(&req), "history image must trigger has_images=true");
    }

    #[test]
    fn request_contains_images_false_when_no_image_anywhere() {
        let req = responses_req_with_input(json!([
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "hi"}
            ]}
        ]));
        assert!(!request_contains_images(&req));
    }

    #[test]
    fn request_contains_images_ignores_assistant_content() {
        let req = responses_req_with_input(json!([
            {"type": "message", "role": "assistant", "content": [
                {"type": "image_url", "image_url": {"url": "x"}}
            ]},
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "hi"}
            ]}
        ]));
        assert!(!request_contains_images(&req), "assistant turns are not user input");
    }
}

fn get_active_provider(db: &Arc<Mutex<Connection>>) -> Result<Provider, GatewayError> {
    let conn = db.lock().map_err(|_| GatewayError(AppError::internal("DB lock failed")))?;
    let settings = crate::storage::gateway_settings::get(&conn)?;

    let provider_id = settings.active_provider_id.ok_or_else(|| {
        GatewayError(AppError::new(
            "ACTIVE_PROVIDER_NOT_FOUND",
            "No active provider configured",
        ).with_suggestion("Set an active provider in the Providers page"))
    })?;

    let provider = crate::storage::providers::get_by_id(&conn, &provider_id).map_err(|_| {
        GatewayError(AppError::new(
            "ACTIVE_PROVIDER_NOT_FOUND",
            "Active provider not found in database",
        ).with_suggestion("Set a new active provider in the Providers page"))
    })?;

    Ok(provider)
}

/// Check if the request contains image content anywhere in the conversation
/// (current turn or replayed history).
pub fn request_contains_images_pub(req: &ResponsesRequest) -> bool {
    request_contains_images(req)
}

fn request_contains_images(req: &ResponsesRequest) -> bool {
    fn content_has_images(v: &Value) -> bool {
        match v {
            Value::Array(arr) => arr.iter().any(|item| {
                let t = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                t == "input_image" || t == "image_url"
            }),
            _ => false,
        }
    }

    match &req.input {
        Value::Array(items) => {
            // Scan ALL user messages in history, not just the last one. Codex
            // replays the full conversation on every turn — if an early turn
            // had an image, the converted body still carries `image_url` parts.
            // MiMo upstream routes that body to its vision endpoint and 404s
            // ("No endpoints found that support image input") when the resolved
            // model can't accept images. Treating any historic image as
            // has_images=true lets `promote_for_capabilities` swap to a vision
            // model (e.g. mimo-v2.5) for the rest of the conversation.
            items.iter().any(|item| {
                let t = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if t != "message" || role != "user" { return false; }
                item.get("content").map(content_has_images).unwrap_or(false)
            })
        }
        _ => false,
    }
}

fn sanitize_body(body: &str) -> String {
    // Simple api key sanitization in request bodies
    let mut s = body.to_string();
    // Match patterns like sk-... and redact them
    let mut search_from = 0;
    while let Some(offset) = s[search_from..].find("sk-") {
        let start = search_from + offset;
        let end = s[start..].find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .map(|e| start + e)
            .unwrap_or(s.len());
        if end - start > 8 {
            s.replace_range(start..end, "sk-****");
            search_from = start + 7; // skip past "sk-****"
        } else {
            search_from = end;
        }
    }
    truncate_str(&s, 50000)
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Find the last char boundary at or before `max` to avoid panic on multibyte chars
    let mut boundary = max;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    s[..boundary].to_string()
}

fn extract_usage(upstream: &serde_json::Value) -> (Option<i64>, Option<i64>) {
    let usage = upstream.get("usage");
    let input = usage.and_then(|u| u.get("prompt_tokens").or(u.get("input_tokens"))).and_then(|v| v.as_i64());
    let output = usage.and_then(|u| u.get("completion_tokens").or(u.get("output_tokens"))).and_then(|v| v.as_i64());
    (input, output)
}

fn log_request_error(
    db: &Arc<Mutex<Connection>>,
    client_type: &str,
    route: &str,
    request_id: &str,
    raw_request: &str,
    converted_request: Option<&str>,
    err: &AppError,
    latency_ms: i64,
) {
    log_request_error_full(db, client_type, route, request_id, raw_request,
        converted_request.unwrap_or(""), "", "", err,
        if err.code == "RESPONSES_PARSE_ERROR" { 400 }
        else if err.code == "PROVIDER_API_KEY_MISSING" { 401 }
        else { 500 },
        latency_ms);
}

/// Lock the DB, recovering from a poisoned Mutex if necessary.
fn lock_db(db: &Arc<Mutex<Connection>>) -> Option<std::sync::MutexGuard<'_, Connection>> {
    match db.lock() {
        Ok(guard) => Some(guard),
        Err(poisoned) => {
            // Recover from a poisoned Mutex (a previous thread panicked while holding it).
            // The data may be in an inconsistent state, but SQLite WAL mode is resilient.
            Some(poisoned.into_inner())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn log_request_success(
    db: &Arc<Mutex<Connection>>,
    client_type: &str,
    route: &str,
    request_id: &str,
    raw_request: &str,
    converted_request: &str,
    raw_response: &str,
    converted_response: &str,
    tool_calls: Option<&str>,
    provider: &str,
    model: &str,
    status_code: i64,
    latency_ms: i64,
    trace_json: Option<&str>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
) {
    if let Some(conn) = lock_db(db) {
        // Calculate cost from pricing table
        let cost = crate::storage::pricing::calculate_cost_for_request(
            &conn, provider, model, input_tokens, output_tokens,
        );
        let _ = crate::storage::request_logs::insert(
            &conn, request_id, client_type, provider, model,
            route, status_code, latency_ms,
            Some(raw_request), Some(converted_request),
            if raw_response.is_empty() { None } else { Some(raw_response) },
            if converted_response.is_empty() { None } else { Some(converted_response) },
            None, tool_calls, None, trace_json,
            input_tokens, output_tokens, cost,
            cache_write_tokens, cache_read_tokens,
        );
    }
}

fn log_request_error_full(
    db: &Arc<Mutex<Connection>>,
    client_type: &str,
    route: &str,
    request_id: &str,
    raw_request: &str,
    converted_request: &str,
    provider: &str,
    model: &str,
    err: &AppError,
    status_code: i64,
    latency_ms: i64,
) {
    // Surface suggestion alongside the raw detail so users see actionable hints
    // (e.g. MiMo's "go activate the Web Search Plugin") right in the log card,
    // not buried in the JSON trace.
    let mut error_msg = format!("{}: {}", err.message, err.detail.as_deref().unwrap_or(""));
    if let Some(ref sug) = err.suggestion {
        error_msg.push_str("\n\n💡 ");
        error_msg.push_str(sug);
    }
    let trace = json!({
        "error_code": err.code,
        "suggestion": err.suggestion,
    }).to_string();
    if let Some(conn) = lock_db(db) {
        let _ = crate::storage::request_logs::insert(
            &conn, request_id, client_type,
            if provider.is_empty() { "unknown" } else { provider },
            if model.is_empty() { "unknown" } else { model },
            route, status_code, latency_ms,
            Some(raw_request),
            if converted_request.is_empty() { None } else { Some(converted_request) },
            None, None, None, None,
            Some(&error_msg),
            Some(&trace),
            None, None, None, // no cost for errors
            None, None,       // no cache tokens for errors
        );
    }
}

// ── Error type for axum ────────────────────────────────────────

pub struct GatewayError(pub AppError);

impl From<AppError> for GatewayError {
    fn from(e: AppError) -> Self {
        Self(e)
    }
}

impl IntoResponse for GatewayError {
    fn into_response(self) -> Response {
        let status = match self.0.code.as_str() {
            "RESPONSES_PARSE_ERROR" | "TRANSFORM_ERROR" | "TOOL_OUTPUT_NOT_FOUND" | "TOOL_CALL_NOT_FOUND" => StatusCode::BAD_REQUEST,
            "PROVIDER_API_KEY_MISSING" | "GATEWAY_AUTH_MISSING" | "GATEWAY_AUTH_INVALID" => StatusCode::UNAUTHORIZED,
            "ACTIVE_PROVIDER_NOT_FOUND" => StatusCode::SERVICE_UNAVAILABLE,
            c if c.starts_with("UPSTREAM") => StatusCode::BAD_GATEWAY,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // Build error message with detail for better client display
        let full_message = match &self.0.detail {
            Some(detail) if !detail.is_empty() => format!("{}: {}", self.0.message, detail),
            _ => self.0.message.clone(),
        };

        // Use OpenAI-compatible error format so clients (Codex, Claude Code, etc.)
        // can parse and display the error message correctly.
        // OpenAI expects: {"error": {"message": "...", "type": "...", "code": "..."}}
        let body = json!({
            "error": {
                "message": full_message,
                "type": self.0.code,
                "code": self.0.code,
                "detail": self.0.detail,
                "suggestion": self.0.suggestion,
            }
        });

        (status, Json(body)).into_response()
    }
}
