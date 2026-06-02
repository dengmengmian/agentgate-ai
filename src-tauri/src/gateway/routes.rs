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

/// Run refiner pipeline on a Value-shaped outbound request body, mutating it
/// in place. Returns the RefinerLog (current callers ignore it pending the
/// trace_json wiring change; once that lands, every handler should stash it
/// into the request log). Failing to lock the DB or read settings degrades
/// to no-op — the gateway should still forward the request transparently.
fn refine_value_body(db: &Arc<Mutex<Connection>>, provider: &Provider, body: &mut Value)
    -> crate::gateway::refiner_log::RefinerLog
{
    let settings = match db.lock().ok().and_then(|c| crate::storage::gateway_settings::get(&c).ok()) {
        Some(s) => s,
        None => return crate::gateway::refiner_log::RefinerLog::default(),
    };
    crate::gateway::refiners::runtime::apply_request(provider, &settings, body)
}

/// Convenience wrapper: serde-ify a serializable request struct, run the
/// refiner pipeline against the JSON view, then ask serde to materialise the
/// modified struct back. If either serde leg fails the original struct is
/// returned untouched — refiner errors must never block the request.
fn refine_struct_body<T>(db: &Arc<Mutex<Connection>>, provider: &Provider, req: &mut T)
    -> crate::gateway::refiner_log::RefinerLog
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let mut body = match serde_json::to_value(&*req) {
        Ok(v) => v,
        Err(_) => return crate::gateway::refiner_log::RefinerLog::default(),
    };
    let log = refine_value_body(db, provider, &mut body);
    if !log.is_empty() {
        if let Ok(new) = serde_json::from_value::<T>(body) {
            *req = new;
        }
    }
    log
}

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

// ── POST /v1/messages/count_tokens (Anthropic) ────────────────
//
// Claude Code 跑长 prompt 前调用此端点预估 token 数。Anthropic 自己实现了精确
// 计数（用 tokenizer），我们本地用启发式估算：
//   - text content 字符数 / 4（英文）或字符数 / 1.6（中文密集）取大
//   - tool_use input_schema 加 schema 复杂度估算
//   - thinking budget 不算 input
//
// 不转发上游因为：① 不所有 anthropic 兼容 provider 都实现此端点；② 启发式
// 足够 client 做 budget check 用，精确值由上游业务请求返。

pub async fn handle_count_tokens(
    headers: HeaderMap,
    AxumState(_state): AxumState<GatewayState>,
    body: bytes::Bytes,
) -> Result<Json<Value>, GatewayError> {
    validate_auth(&headers)?;
    let body = crate::gateway::body_decode::decode(&headers, body).map_err(GatewayError)?;
    let v: Value = serde_json::from_str(&body)
        .map_err(|e| GatewayError(AppError::new("COUNT_TOKENS_PARSE_ERROR", format!("Failed to parse: {e}"))))?;

    let estimate = estimate_anthropic_tokens(&v);
    Ok(Json(json!({"input_tokens": estimate})))
}

fn estimate_anthropic_tokens(req: &Value) -> i64 {
    let mut chars: usize = 0;
    if let Some(sys) = req.get("system") {
        chars += count_chars(sys);
    }
    if let Some(messages) = req.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            if let Some(c) = msg.get("content") {
                chars += count_chars(c);
            }
        }
    }
    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        for tool in tools {
            chars += tool.to_string().len();
        }
    }
    // 启发式：4 chars/token 对英文友好，中文密集时偏低但仍 conservative
    ((chars as f64) / 4.0).ceil() as i64
}

fn count_chars(v: &Value) -> usize {
    match v {
        Value::String(s) => s.chars().count(),
        Value::Array(arr) => arr.iter().map(|x| count_chars(x)).sum(),
        Value::Object(o) => o.values().map(|x| count_chars(x)).sum(),
        _ => 0,
    }
}

// Gemini countTokens 的处理直接合并到 handle_gemini_generate 里（router 没法
// 按 :countTokens 后缀分发，handler 入口分流更稳）。

// ── GET /v1beta/models (Gemini 客户端拉 models 列表) ──────────

pub async fn list_gemini_models(
    headers: HeaderMap,
    AxumState(state): AxumState<GatewayState>,
) -> Result<Json<Value>, GatewayError> {
    validate_auth(&headers)?;
    let provider = get_active_provider(&state.db)?;

    let mut models: Vec<Value> = Vec::new();
    models.push(json!({
        "name": format!("models/{}", provider.default_model),
        "displayName": provider.default_model,
        "supportedGenerationMethods": ["generateContent", "streamGenerateContent", "countTokens"],
    }));
    if let Some(ref rm) = provider.reasoning_model {
        if !rm.is_empty() {
            models.push(json!({
                "name": format!("models/{rm}"),
                "displayName": rm,
                "supportedGenerationMethods": ["generateContent", "streamGenerateContent", "countTokens"],
            }));
        }
    }
    Ok(Json(json!({"models": models})))
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
            let model_override = native_model_override(&provider, req.model.as_deref(), Some(&model));
            crate::gateway::pass_through::handle(
                &state.http_client, &state.db, &config, &target_url, "/v1/responses", "openai_responses", &body, model_override.as_deref(), &request_id, start, &client_type, Some(&headers),
            ).await.map_err(|e| GatewayError(e))
        } else if config.is_anthropic() {
            // Claude Messages API conversion (only for Anthropic-type providers)
            // auto_cache_control: default true unless provider explicitly set false
            let auto_cache = provider.auto_cache_control.unwrap_or(true);
            let mut anthropic_body = match responses_to_anthropic::convert(&req, &model, auto_cache) {
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
                handle_anthropic_stream_response(state.clone(), config.clone(), anthropic_body, request_id.clone(), raw_body.clone(), converted_json, model.clone(), start, client_type.clone(), session_id.clone(), candidate.provider_id.clone()).await
            } else {
                handle_anthropic_non_stream_response(state.clone(), config.clone(), anthropic_body, request_id.clone(), raw_body.clone(), converted_json, model.clone(), start, client_type.clone(), session_id.clone(), candidate.provider_id.clone()).await
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
            let mut chat_req = match responses_to_chat::convert_with_provider_matrix(&req, &model, provider_transform.as_ref(), &matrix) {
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
                                .map(|anns| crate::protocol::responses_events::normalize_annotations(anns))
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
                                let safe_args = crate::transform::tool_calls::salvage_tool_arguments(
                                    &tc.function.arguments, &tc.function.name, &tc.id, finish,
                                );
                                // #1 namespace 还原（split 后无 prefix 时透传原名）
                                let (display_name, namespace) =
                                    crate::transform::tool_calls::split_namespace_tool_name(&tc.function.name)
                                        .map(|(ns, name)| (name, Some(ns)))
                                        .unwrap_or_else(|| (tc.function.name.clone(), None));
                                let mut item = json!({
                                    "id": format!("fc_{}", tc.id),
                                    "type": "function_call",
                                    "status": "completed",
                                    "call_id": tc.id,
                                    "name": display_name,
                                    "arguments": safe_args,
                                });
                                if let Some(ns) = namespace { item["namespace"] = json!(ns); }
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
            let trace = trace_with_degradation_events(
                json!({ "response_id": &resp_id, "stream": false }),
                &chat_req.diagnostic_events,
            );
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

                let result = crate::gateway::sse::process_upstream_stream_inner(
                    boot, tx.clone(), &mut acc, true, true,
                ).await;

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

                        // Bug #9 修复：trace 加 finish_reason / reasoning_tokens /
                        // truncated 字段，让 `agentgate logs` 能直接看出截断原因
                        // （而不是猜是 max_tokens 还是 AgentGate 自己挂了）。
                        let reasoning_tokens = acc.usage.as_ref()
                            .and_then(|u| u.get("output_tokens_details"))
                            .and_then(|d| d.get("reasoning_tokens"))
                            .and_then(|v| v.as_i64());
                        let truncated = matches!(
                            acc.finish_reason.as_deref(),
                            Some("length") | Some("max_tokens")
                        );
                        let trace = trace_with_degradation_events(serde_json::json!({
                            "response_id": &acc.response_id,
                            "stream": true,
                            "text_len": acc.full_text.len(),
                            "tool_calls_count": tc_list.len(),
                            "reasoning_len": acc.reasoning_content.len(),
                            "finish_reason": acc.finish_reason.as_deref(),
                            "reasoning_tokens": reasoning_tokens,
                            "truncated": truncated,
                        }), &diagnostic_events);
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

    // Gemini 路径形如 "gemini-2.5-flash:generateContent" / ":streamGenerateContent"
    // / ":countTokens"。axum 无法在 router 层按 action 分发，handler 入口分流。
    if model_path.ends_with(":countTokens") {
        let body = crate::gateway::body_decode::decode(&headers, body).map_err(GatewayError)?;
        let v: Value = serde_json::from_str(&body)
            .map_err(|e| GatewayError(AppError::new("COUNT_TOKENS_PARSE_ERROR", format!("Failed to parse: {e}"))))?;
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

    // Gemini → Gemini passthrough：选中的 provider 是 google_gemini 原生上游，
    // 直接调上游 generateContent / streamGenerateContent，body 原样转发回 client。
    // 不绕 Chat 转换，避免丢 thinking / grounding / safetySettings 这些 Gemini-only 字段。
    if config.is_gemini() {
        // Override body 里的 model（如有 mapping）
        let model_override = native_model_override(&selection.provider, Some(&model_name), Some(&resolved_model));
        let final_model = model_override.unwrap_or(resolved_model.clone());

        if is_stream {
            let upstream_resp = adapter::send_gemini_stream(&state.http_client, &config, &gemini_body, &final_model).await
                .map_err(|e| {
                    log_request_error_full(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, "", &config.name, &final_model, &e, 502, start.elapsed().as_millis() as i64);
                    GatewayError(e)
                })?;
            let boot = crate::gateway::sse_bootstrap::bootstrap_detect(upstream_resp).await
                .map_err(|e| {
                    log_request_error_full(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, "", &config.name, &final_model, &e, 502, start.elapsed().as_millis() as i64);
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
                let prefix_text = String::from_utf8_lossy(&boot.prefix).into_owned();
                if !prefix_text.is_empty() {
                    let _ = tx.send(prefix_text).await;
                }
                let mut stream = boot.stream;
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(b) => {
                            if tx.send(String::from_utf8_lossy(&b).into_owned()).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                let latency = start.elapsed().as_millis() as i64;
                let trace = json!({"mode": "native_pass_through", "protocol": "gemini", "stream": true}).to_string();
                log_request_success(&db, &client_type_owned, "/v1beta/generateContent", &req_id, &raw_req, "", "", "", None,
                    &provider_name, &model_clone, 200, latency, Some(&trace), None, None, None, None);
            });
            let stream = ReceiverStream::new(rx);
            let body = Body::from_stream(
                tokio_stream::StreamExt::map(stream, |s| Ok::<_, std::convert::Infallible>(s))
            );
            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/event-stream")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(body)
                .unwrap());
        } else {
            let result = adapter::send_gemini_non_stream(&state.http_client, &config, &gemini_body, &final_model).await;
            match result {
                Ok(upstream_json) => {
                    let latency = start.elapsed().as_millis() as i64;
                    let (in_tok, out_tok) = extract_gemini_usage(&upstream_json);
                    let trace = json!({"mode": "native_pass_through", "protocol": "gemini", "stream": false}).to_string();
                    log_request_success(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, "",
                        &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                        &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                        None, &config.name, &final_model, 200, latency, Some(&trace), in_tok, out_tok, None, None);
                    return Ok(Json(upstream_json).into_response());
                }
                Err(err) => {
                    let latency = start.elapsed().as_millis() as i64;
                    log_request_error_full(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, "",
                        &config.name, &final_model, &err, 502, latency);
                    return Err(GatewayError(err));
                }
            }
        }
    }

    // Convert Gemini → Chat Completions
    let mut chat_req = gemini_to_chat::convert(&gemini_body, &resolved_model).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, None, &e, start.elapsed().as_millis() as i64);
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
        let upstream_resp = adapter::send_stream(&state.http_client, &config, &mut chat_req).await.map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1beta/generateContent", &request_id, &raw_body, &converted_json, &config.name, &resolved_model, &e, 502, latency);
            GatewayError(e)
        })?;
        converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();

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
        let diagnostic_events = chat_req.diagnostic_events.clone();

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
            let trace = trace_with_degradation_events(
                json!({"response_id": &req_id, "stream": true, "protocol": "gemini_input"}),
                &diagnostic_events,
            );
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
        let result = adapter::send_non_stream(&state.http_client, &config, &mut chat_req).await;

        match result {
            Ok(upstream_json) => {
                converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
                let gemini_resp = gemini_to_chat::response_to_gemini(&upstream_json, &resolved_model);
                let latency = start.elapsed().as_millis() as i64;
                let (in_tok, out_tok) = extract_usage(&upstream_json);
                let trace = trace_with_degradation_events(
                    json!({"protocol": "gemini_input"}),
                    &chat_req.diagnostic_events,
                );
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

    let requested_model = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(str::to_string));

    // Provider 选取：先按 openai_chat_completions 路由 profile 选；选不到再
    // fallback 到 anthropic_messages —— 让只配了 Anthropic 端点的 provider
    // 也能服务 Chat 客户端，下面 anthropic 分支负责协议转换。
    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db, "openai_chat_completions", requested_model.as_deref(), None,
    ).or_else(|_| crate::gateway::provider_selector::select_for_failover(
        &state.db, "anthropic_messages", requested_model.as_deref(), None,
    )).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/chat/completions", &request_id, &sanitize_body(&body), None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;

    let is_failover = selection.mode == "failover" && selection.candidates.len() > 1;
    let candidates = selection.candidates.clone();
    let raw_body = sanitize_body(&body);

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

        // Chat → Anthropic 转换分支：provider 是 anthropic 且配了 anthropic_base_url。
        // 走 client_chat_to_anthropic_handle 转换请求体、调上游 Anthropic、再把响应/SSE
        // 翻译成 Chat 形态发回。
        if config.is_anthropic() && config.has_anthropic_url() {
            let model_override = native_model_override(&provider, requested_model.as_deref(), Some(&candidate.model));
            let model = model_override.unwrap_or_else(|| candidate.model.clone());
            let result = client_chat_to_anthropic_handle(
                state.clone(), config.clone(), provider.clone(), &body, model.clone(),
                request_id.clone(), raw_body.clone(), start, client_type.clone(),
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
                            &conn, &candidate.provider_id, &err.0.code, &err.0.message, candidate.cooldown_seconds,
                        );
                    }
                    if is_failover && attempt_idx < attempt_order.len() - 1 {
                        if crate::gateway::provider_selector::should_failover(Some(502), &err.0.message, candidate) {
                            last_error = Some(err.0);
                            continue;
                        }
                    }
                    return Err(err);
                }
            }
        }

        let decision = match crate::gateway::route_decision::decide("/v1/chat/completions", &provider.protocol, &config.base_url) {
            Ok(d) => d, Err(e) => { last_error = Some(e); continue; }
        };

        if decision.mode != crate::gateway::route_decision::RouteMode::PassThrough {
            last_error = Some(AppError::new("PROTOCOL_TRANSFORM_NOT_SUPPORTED", "Not a pass-through provider"));
            continue;
        }

        let model_override = native_model_override(&provider, requested_model.as_deref(), Some(&candidate.model));
        let result = crate::gateway::pass_through::handle(
            &state.http_client, &state.db, &config, &decision.target_url, "/v1/chat/completions", "openai_chat_completions", &body, model_override.as_deref(), &request_id, start, &client_type, Some(&headers),
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
        let err = AppError::new("CHAT_PARSE_ERROR", format!("Failed to parse chat request: {e}"));
        log_request_error(&state.db, &client_type, "/v1/chat/completions", &request_id, &raw_request, None, &err, start.elapsed().as_millis() as i64);
        GatewayError(err)
    })?;
    // model_mapping 覆盖
    chat_req.model = model.clone();

    let want_stream = chat_req.stream;

    // 2. Chat → Anthropic 转换
    let mut anthropic_body = crate::transform::chat_to_anthropic::convert(&chat_req).map_err(|e| {
        log_request_error(&state.db, &client_type, "/v1/chat/completions", &request_id, &raw_request, None, &e, start.elapsed().as_millis() as i64);
        GatewayError(e)
    })?;
    // 3. 网关精炼层：开关全关时是 no-op，开了才会按 quirks 改写 outbound body
    let _refiner_log = refine_value_body(&state.db, &provider, &mut anthropic_body);
    let converted_request = serde_json::to_string_pretty(&anthropic_body).unwrap_or_default();

    if want_stream {
        return client_chat_to_anthropic_stream(
            state, config, anthropic_body, model, request_id, raw_request, converted_request, start, client_type,
        ).await;
    }

    // 3. 非流式：发 Anthropic non-stream 请求
    let result = adapter::send_anthropic_non_stream(&state.http_client, &config, &anthropic_body).await;
    match result {
        Ok(upstream_json) => {
            let chat_resp = crate::transform::anthropic_to_chat::convert(&upstream_json, &model);
            let latency = start.elapsed().as_millis() as i64;
            // usage 同时含 Anthropic + OpenAI 两形态字段，extract_cache_tokens 都识别
            let (in_tok, out_tok) = (
                chat_resp.get("usage").and_then(|u| u.get("prompt_tokens")).and_then(|v| v.as_i64()),
                chat_resp.get("usage").and_then(|u| u.get("completion_tokens")).and_then(|v| v.as_i64()),
            );
            let (cache_w, cache_r) = chat_resp.get("usage")
                .map(crate::storage::request_logs::extract_cache_tokens)
                .unwrap_or((None, None));
            let trace = json!({"mode": "transform", "protocol": "chat_to_anthropic", "stream": false}).to_string();
            log_request_success(&state.db, &client_type, "/v1/chat/completions", &request_id, &raw_request, &converted_request,
                &serde_json::to_string_pretty(&upstream_json).unwrap_or_default(),
                &serde_json::to_string_pretty(&chat_resp).unwrap_or_default(),
                None, &config.name, &model, 200, latency, Some(&trace), in_tok, out_tok, cache_w, cache_r);
            Ok(Json(chat_resp).into_response())
        }
        Err(err) => {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1/chat/completions", &request_id, &raw_request, &converted_request,
                &config.name, &model, &err, 502, latency);
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

    let upstream = adapter::send_anthropic_stream(&state.http_client, &config, &anthropic_body).await
        .map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1/chat/completions", &request_id, &raw_request, &converted_request,
                &config.name, &model, &e, 502, latency);
            GatewayError(e)
        })?;

    // Bootstrap 校验：HTTP 200 + 错误帧的情况能被识别并报错触发 failover
    let boot = crate::gateway::sse_bootstrap::bootstrap_detect(upstream).await
        .map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1/chat/completions", &request_id, &raw_request, &converted_request,
                &config.name, &model, &e, 502, latency);
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
                        if !data_str.is_empty() { data_str.push('\n'); }
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
                        if let Some(obj) = final_usage_json.as_mut().and_then(|u| u.as_object_mut()) {
                            if let Some(ot) = u.get("output_tokens") {
                                obj.insert("output_tokens".into(), ot.clone());
                            }
                        }
                    }
                }
                if event_type == "message_start" {
                    if let Some(u) = data.get("message").and_then(|m| m.get("usage")) {
                        final_usage_json.get_or_insert_with(|| json!({}));
                        if let Some(obj) = final_usage_json.as_mut().and_then(|u| u.as_object_mut()) {
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
                }).to_string();
                let (in_tok, out_tok) = final_usage_json.as_ref().map(|u| (
                    u.get("input_tokens").and_then(|v| v.as_i64()),
                    u.get("output_tokens").and_then(|v| v.as_i64()),
                )).unwrap_or((None, None));
                let (cache_w, cache_r) = final_usage_json.as_ref()
                    .map(crate::storage::request_logs::extract_cache_tokens)
                    .unwrap_or((None, None));
                log_request_success(&db, &client_type_owned, "/v1/chat/completions", &req_id, &raw_req, &conv_req,
                    "", &truncate_str(&total_text, 10000), None,
                    &provider_name, &model_clone, 200, latency, Some(&trace), in_tok, out_tok, cache_w, cache_r);
            }
            Some(msg) => {
                let err = AppError::new("UPSTREAM_STREAM_ERROR", &msg);
                log_request_error_full(&db, &client_type_owned, "/v1/chat/completions", &req_id, &raw_req, &conv_req,
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

    let requested_model = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("model").and_then(|m| m.as_str()).map(str::to_string));

    // Select provider — try anthropic_messages protocol first, then openai_responses as fallback
    let selection = crate::gateway::provider_selector::select_for_failover(
        &state.db,
        "anthropic_messages",
        requested_model.as_deref(),
        None,
    ).or_else(|_| {
        crate::gateway::provider_selector::select_for_failover(
            &state.db,
            "openai_responses",
            requested_model.as_deref(),
            None,
        )
    }).map_err(|e| {
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
            let model_override = native_model_override(&selection.provider, requested_model.as_deref(), Some(&selection.model));
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
    // Anthropic 工具形态 {name, description, input_schema} —— 没有顶层 type，
    // 必须走 anthropic_messages::tools_to_chat，否则 transform::tool_calls::convert_tools
    // 会把整组工具丢弃。
    let tools: Option<Vec<serde_json::Value>> = msg_req.tools.as_ref().map(|t| {
        crate::protocol::anthropic_messages::tools_to_chat(t, config.is_deepseek())
    }).filter(|t| !t.is_empty());
    // tool_choice 也得翻译：Anthropic {type:"tool",name:"X"} 与 Chat
    // {type:"function",function:{name:"X"}} 不通用；{type:"any"} → "required"。
    let tool_choice = msg_req.tool_choice.as_ref()
        .map(crate::protocol::anthropic_messages::tool_choice_to_chat);
    // thinking.budget_tokens → reasoning_effort 字符串。Chat 没有真正的 budget 字段，
    // 桶化映射是最接近的等价表达（与 Responses→Anthropic 方向对称）。
    let reasoning_effort = msg_req.thinking.as_ref()
        .and_then(crate::protocol::anthropic_messages::thinking_to_reasoning_effort);
    let want_stream = msg_req.stream.unwrap_or(false);

    let mut chat_req = crate::protocol::chat_completions::ChatCompletionsRequest {
        model: model.clone(), messages, tools,
        tool_choice,
        stream: want_stream,
        temperature: msg_req.temperature, top_p: msg_req.top_p,
        max_tokens: msg_req.max_tokens,
        max_completion_tokens: msg_req.max_tokens, // 同步透传新字段（C 修复）
        thinking: None,
        // include_usage 必加：默认 Chat stream 不带 usage，client 看 token 都是 0；
        // 加上后终块带完整 usage，message_delta 能正确报 output_tokens。
        stream_options: if want_stream { Some(json!({"include_usage": true})) } else { None },
        response_format: None, reasoning_effort,
        seed: None, stop: None, frequency_penalty: None, presence_penalty: None,
        parallel_tool_calls: None,
        diagnostic_events: Vec::new(),
    };

    let _refiner_log = refine_struct_body(&state.db, &selection.provider, &mut chat_req);
    let mut converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();

    if want_stream {
        // 真流式：边收上游 Chat SSE chunk 边转 Anthropic 事件、立即转发给 client。
        // 首字延迟 = 上游首字延迟（1-3s 级别），不是上游完整耗时。
        return handle_anthropic_fallback_stream(
            state, config, chat_req, model, request_id, raw, converted_json, start, client_type,
        ).await;
    }

    let result = adapter::send_non_stream(&state.http_client, &config, &mut chat_req).await;
    match result {
        Ok(upstream_json) => {
            converted_json = serde_json::to_string_pretty(&chat_req).unwrap_or_default();
            let response = crate::protocol::anthropic_messages::from_chat_response(&upstream_json, &model);
            let latency = start.elapsed().as_millis() as i64;
            let (in_tok, out_tok) = extract_usage(&upstream_json);
            let (cache_w, cache_r) = upstream_json.get("usage")
                .map(crate::storage::request_logs::extract_cache_tokens)
                .unwrap_or((None, None));
            let trace = trace_with_degradation_events(
                json!({"mode": "transform", "protocol": "anthropic_messages", "stream": false}),
                &chat_req.diagnostic_events,
            );
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

    let upstream = adapter::send_stream(&state.http_client, &config, &mut chat_req).await
        .map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1/messages", &request_id, &raw_request, &converted_request,
                &config.name, &model, &e, 502, latency);
            GatewayError(e)
        })?;
    converted_request = serde_json::to_string_pretty(&chat_req).unwrap_or_default();

    // 用 sse_bootstrap 检查上游首批字节——HTTP 200 + 错误帧的情况能被识别并
    // 转成正常错误回给 client 的 SDK，而不是糊弄它走假流式。
    let boot = crate::gateway::sse_bootstrap::bootstrap_detect(upstream).await
        .map_err(|e| {
            let latency = start.elapsed().as_millis() as i64;
            log_request_error_full(&state.db, &client_type, "/v1/messages", &request_id, &raw_request, &converted_request,
                &config.name, &model, &e, 502, latency);
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
                match serde_json::from_str(data_str) { Ok(c) => c, Err(_) => return true };
            // 顺手把可观测信号采集起来（落日志用）
            if let Some(u) = &chunk.usage { *final_usage = Some(u.clone()); }
            if let Some(choices) = &chunk.choices {
                for c in choices {
                    if let Some(d) = &c.delta {
                        if let Some(t) = &d.content { total_text.push_str(t); }
                    }
                }
            }
            for ev in converter.process_chunk(&chunk) {
                if tx.send(ev).await.is_err() { return false; }
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
                        if !handle_frame(&mut converter, &tx, data, &mut total_text, &mut final_usage_json).await {
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
            if tx.send(ev).await.is_err() { break; }
        }

        let _ = bootstrap_replayed; // 仅用于潜在 debug，无副作用

        let latency = start.elapsed().as_millis() as i64;
        let (in_tok, out_tok) = final_usage_json.as_ref().map(|u| {
            let i = u.get("prompt_tokens").or_else(|| u.get("input_tokens")).and_then(|v| v.as_i64());
            let o = u.get("completion_tokens").or_else(|| u.get("output_tokens")).and_then(|v| v.as_i64());
            (i, o)
        }).unwrap_or((None, None));
        let (cache_w, cache_r) = final_usage_json.as_ref()
            .map(crate::storage::request_logs::extract_cache_tokens)
            .unwrap_or((None, None));
        let trace = trace_with_degradation_events(
            json!({"mode": "transform", "protocol": "anthropic_messages", "stream": true}),
            &diagnostic_events,
        );
        log_request_success(&db, &client_type_owned, "/v1/messages", &req_id, &raw_req, &conv_req,
            &final_usage_json.map(|u| serde_json::to_string_pretty(&u).unwrap_or_default()).unwrap_or_default(),
            &total_text.chars().take(10_000).collect::<String>(),
            None, &provider_name, &model_clone, 200, latency, Some(&trace), in_tok, out_tok, cache_w, cache_r);
    });

    let stream = ReceiverStream::new(rx);
    let body = axum::body::Body::from_stream(
        tokio_stream::StreamExt::map(stream, |s| Ok::<_, std::convert::Infallible>(s))
    );
    Ok(axum::response::Response::builder()
        .status(axum::http::StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "text/event-stream")
        .header(axum::http::header::CACHE_CONTROL, "no-cache")
        .body(body)
        .unwrap())
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

    fn provider_for_native_model_tests() -> Provider {
        Provider {
            id: "p1".to_string(),
            name: "DeepSeek".to_string(),
            provider_type: "deepseek".to_string(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            api_key: Some("sk-test".to_string()),
            default_model: "deepseek-v4-flash".to_string(),
            reasoning_model: Some("deepseek-v4-pro".to_string()),
            supported_models: Some(r#"["deepseek-v4-pro","deepseek-v4-flash"]"#.to_string()),
            model_mapping: Some(r#"{"gpt-5.5":"deepseek-v4-pro"}"#.to_string()),
            extra_headers: None,
            anthropic_base_url: None,
            responses_base_url: None,
            protocol: "openai_chat_completions".to_string(),
            timeout_seconds: 300,
            status: "active".to_string(),
            supports_vision: Some(false),
            auto_cache_control: None,
            supports_cache: None,
            model_capabilities: None,
            provider_quirks: None,
            body_filter_enabled: None,
            thinking_rectifier_enabled: None,
            error_mapper_enabled: None,
            model_degradation_chain: None,
            enabled: true,
            is_active: true,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn native_model_override_maps_agentgate_virtual_model() {
        let provider = provider_for_native_model_tests();
        assert_eq!(
            native_model_override(&provider, Some("agentgate"), None),
            Some("deepseek-v4-flash".to_string())
        );
    }

    #[test]
    fn native_model_override_maps_prefixed_agentgate_virtual_model() {
        let provider = provider_for_native_model_tests();
        assert_eq!(
            native_model_override(&provider, Some("openai/agentgate"), None),
            Some("deepseek-v4-flash".to_string())
        );
    }

    #[test]
    fn native_model_override_uses_route_selected_model_for_agentgate() {
        let provider = provider_for_native_model_tests();
        assert_eq!(
            native_model_override(&provider, Some("agentgate"), Some("deepseek-v4-pro")),
            Some("deepseek-v4-pro".to_string())
        );
    }

    #[test]
    fn native_model_override_still_prefers_explicit_mapping() {
        let provider = provider_for_native_model_tests();
        assert_eq!(
            native_model_override(&provider, Some("gpt-5.5"), Some("deepseek-v4-flash")),
            Some("deepseek-v4-pro".to_string())
        );
    }

    #[test]
    fn native_model_override_preserves_unmapped_real_model() {
        let provider = provider_for_native_model_tests();
        assert_eq!(native_model_override(&provider, Some("mimo-v2.5"), Some("deepseek-v4-flash")), None);
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
    fn request_contains_images_ignores_historic_image_when_current_turn_text_only() {
        // 反转旧测试：history 有图但当前 turn 是纯文本 → false。
        // 历史 image 由 mimo.rs::finalize_request strip 兜底，不需要 promote。
        // 避免一次发图导致整个会话被强制路由到 vision 模型（128K context）。
        let req = responses_req_with_input(json!([
            {"type": "message", "role": "user", "content": [
                {"type": "input_image", "image_url": {"url": "https://example.com/x.png"}}
            ]},
            {"type": "message", "role": "assistant", "content": "I see a cat."},
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "what color is it?"}
            ]}
        ]));
        assert!(!request_contains_images(&req),
            "history image must NOT force promotion when current turn is text-only");
    }

    #[test]
    fn request_contains_images_true_when_current_turn_has_image() {
        // 当前 turn 真发图 → 必须 promote 到 vision 模型（图还没被 strip）
        let req = responses_req_with_input(json!([
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "earlier message"}
            ]},
            {"type": "message", "role": "assistant", "content": "ok"},
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "look at this"},
                {"type": "input_image", "image_url": {"url": "https://example.com/x.png"}}
            ]}
        ]));
        assert!(request_contains_images(&req),
            "current user turn with image must trigger promotion");
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

    #[test]
    fn request_contains_images_ignores_tool_outputs_after_user_image() {
        // 当前 turn user 有图，但后面跟了 tool/function_call_output（rev 遍历
        // 跳过非 user message，正确找到最后一条 user）
        let req = responses_req_with_input(json!([
            {"type": "message", "role": "user", "content": [
                {"type": "input_image", "image_url": {"url": "x"}}
            ]},
            {"type": "function_call_output", "call_id": "c1", "output": "stuff"}
        ]));
        assert!(request_contains_images(&req),
            "rev iter must skip tool items and find last user message");
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

const AGENTGATE_VIRTUAL_MODEL: &str = "agentgate";

fn native_model_override(provider: &Provider, requested_model: Option<&str>, resolved_model: Option<&str>) -> Option<String> {
    let requested = requested_model?.trim();
    if requested.is_empty() {
        return None;
    }

    if is_agentgate_virtual_model(requested) {
        return Some(resolved_model.unwrap_or(&provider.default_model).to_string());
    }

    explicit_model_mapping(provider, requested)
}

fn is_agentgate_virtual_model(requested: &str) -> bool {
    let model = requested.rsplit_once('/').map(|(_, model)| model).unwrap_or(requested);
    model.eq_ignore_ascii_case(AGENTGATE_VIRTUAL_MODEL)
}

fn explicit_model_mapping(provider: &Provider, requested: &str) -> Option<String> {
    let mapping = provider.model_mapping.as_ref()?;
    serde_json::from_str::<std::collections::HashMap<String, String>>(mapping)
        .ok()
        .and_then(|m| m.get(requested).cloned())
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

    // 只看**最后一条** user message 是否含 image。历史 image 不算。
    //
    // 旧实现扫整个 history（"any historic image → promote"），为了避免 MiMo 上游
    // 看到 history image_url 后 404。但我们后来给 mimo.rs::finalize_request
    // 加了 image_url 自动剥离 + 注 OCR notice（#6 修复）兜底，404 不再发生。
    // 这条保护过时了，反而成了**副作用源**：
    //   - 用户某轮发过图 → 整个会话剩余请求被强制 promote 到 vision 模型
    //   - mimo-v2.5-pro (1M ctx) → mimo-v2.5 (128K ctx) 降级
    //   - 大会话进入 95%+ window 紧张区间 → 模型短回复 stop
    //
    // 第一性原理：vision 需求 = 模型现在需要看到一张图 = 当前 turn 有图。
    // 历史 image 已经被 strip 兜底，不需要为它牺牲 context window。
    match &req.input {
        Value::Array(items) => {
            // 找最后一条 user message（不是最后一条 message——尾部可能是
            // tool 结果或 function_call 等）
            items
                .iter()
                .rev()
                .find(|item| {
                    item.get("type").and_then(|t| t.as_str()) == Some("message")
                        && item.get("role").and_then(|r| r.as_str()) == Some("user")
                })
                .and_then(|item| item.get("content"))
                .map(content_has_images)
                .unwrap_or(false)
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

fn trace_with_degradation_events(
    mut trace: serde_json::Value,
    events: &[crate::protocol::chat_completions::CapabilityDegradationEvent],
) -> String {
    if !events.is_empty() {
        trace["degradation_events"] = serde_json::json!(events);
    }
    trace.to_string()
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
            Some("gateway"), None, Some(request_id),
        );
    }
    // Prometheus 指标
    crate::gateway::metrics::record_request(
        route, client_type, provider, status_code as u16, latency_ms as f64 / 1000.0,
    );
    if let Some(t) = input_tokens {
        crate::gateway::metrics::record_tokens(provider, model, "input", t);
    }
    if let Some(t) = output_tokens {
        crate::gateway::metrics::record_tokens(provider, model, "output", t);
    }
    if let Some(t) = cache_read_tokens {
        crate::gateway::metrics::record_tokens(provider, model, "cache_read", t);
    }
    if let Some(t) = cache_write_tokens {
        crate::gateway::metrics::record_tokens(provider, model, "cache_write", t);
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
            Some("gateway"), None, Some(request_id),
        );
    }
    // Prometheus 指标（错误也算一次请求）
    crate::gateway::metrics::record_request(
        route, client_type,
        if provider.is_empty() { "unknown" } else { provider },
        status_code as u16, latency_ms as f64 / 1000.0,
    );
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
