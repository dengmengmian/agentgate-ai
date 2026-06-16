use axum::body::Body;
use axum::extract::{State as AxumState, OriginalUri};
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
    detect_client_from_ua, lock_db, log_request_error, log_request_error_full, log_request_success,
    native_model_override, refine_struct_body, refine_value_body, request_contains_images,
    sanitize_body, trace_with_degradation_events, truncate_str, validate_auth, GatewayError,
};
use super::GatewayState;

mod anthropic;
mod chat;
mod gemini;
use anthropic::*;
use chat::*;
use gemini::*;

// ── POST /v1/responses ─────────────────────────────────────────

pub async fn handle_responses(
    OriginalUri(uri): OriginalUri,
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
    let uri_path = uri.path().to_string();
    let is_codex_compact = crate::gateway::codex_compact::is_codex_v2_compaction(&headers, &uri_path);

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

        // Codex remote compaction v2:本地做 summary,绕过上游(上游模型多半接不上)。
        // 走 chat completions 调用,跟当前 provider 共用同一把 API key 和模型。
        // 失败不再失败转移——compact 是 best-effort,上层失败就让原流程接力。
        if is_codex_compact {
            return match crate::gateway::codex_compact::handle_codex_compaction(
                &state.http_client,
                &config,
                &req,
                &request_id,
                start,
            )
            .await
            {
                Ok(resp) => {
                    let trace = json!({
                        "mode": "codex_compact",
                        "client_protocol": "openai_responses",
                        "provider_protocol": "openai_chat_completions",
                        "route": &uri_path,
                        "summary": "AgentGate 本地做 summary 替代远程 v2 compaction",
                    })
                    .to_string();
                    log_request_success(
                        &state.db,
                        &client_type,
                        &uri_path,
                        &request_id,
                        &sanitize_body(&body),
                        "",
                        "(codex_compact SSE)",
                        "",
                        None,
                        &provider.name,
                        &model,
                        200,
                        start.elapsed().as_millis() as i64,
                        Some(&trace),
                        crate::gateway::usage::TokenUsage::default(),
                    );
                    Ok(resp)
                }
                Err(e) => {
                    log_request_error(
                        &state.db,
                        &client_type,
                        &uri_path,
                        &request_id,
                        &sanitize_body(&body),
                        Some(&provider.name),
                        &e.0,
                        start.elapsed().as_millis() as i64,
                    );
                    Err(e)
                }
            };
        }

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
            // 长历史自压缩:超阈值时摘要中段历史,落回上游窗口内。默认开启,阈值按模型
            // 上下文窗口 ×85% 自适应(详见 auto_compact),内部按需额外调一次上游。
            crate::gateway::auto_compact::maybe_compact(&state.http_client, &config, &mut chat_req)
                .await;
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
                    req.clone(),
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
        AppError::new(
            crate::errors::codes::FAILOVER_EXHAUSTED,
            "All providers failed",
        )
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::{CompletionChoice, CompletionMessage};
    use serde_json::json;

    #[test]
    fn chat_non_stream_response_envelope_preserves_responses_metadata() {
        let req = ResponsesRequest {
            model: Some("client-model".into()),
            input: json!("hello"),
            instructions: Some("Be concise".into()),
            tools: Some(vec![json!({"type": "function", "name": "shell"})]),
            tool_choice: Some(json!("auto")),
            temperature: Some(0.2),
            top_p: Some(0.9),
            max_output_tokens: Some(123),
            parallel_tool_calls: Some(false),
            reasoning: Some(json!({"effort": "high"})),
            text: Some(json!({"format": {"type": "json_object"}})),
            metadata: Some(json!({"trace": "abc"})),
            previous_response_id: Some("resp_prev".into()),
            ..Default::default()
        };
        let chat_resp = ChatCompletionResponse {
            id: Some("chatcmpl_1".into()),
            usage: Some(json!({
                "prompt_tokens": 10,
                "completion_tokens": 3,
                "total_tokens": 13
            })),
            choices: Some(vec![CompletionChoice {
                finish_reason: Some("length".into()),
                message: Some(CompletionMessage {
                    role: Some("assistant".into()),
                    content: Some("partial".into()),
                    reasoning_content: None,
                    tool_calls: None,
                }),
            }]),
        };

        let resp = build_chat_non_stream_responses_response(
            "resp_test",
            "mimo-v2.5-pro",
            &req,
            &chat_resp,
            vec![json!({
                "id": "msg_test",
                "type": "message",
                "status": "completed",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "partial"}]
            })],
        );

        assert_eq!(resp["status"], "incomplete");
        assert_eq!(
            resp["incomplete_details"],
            json!({"reason": "max_output_tokens"})
        );
        assert_eq!(resp["usage"]["input_tokens"], 10);
        assert_eq!(resp["usage"]["output_tokens"], 3);
        assert_eq!(resp["usage"]["total_tokens"], 13);
        assert_eq!(resp["parallel_tool_calls"], false);
        assert_eq!(resp["tool_choice"], json!("auto"));
        assert_eq!(
            resp["reasoning"],
            json!({"effort": "high", "summary": null})
        );
        assert_eq!(resp["text"], json!({"format": {"type": "json_object"}}));
        assert_eq!(resp["metadata"], json!({"trace": "abc"}));
        assert_eq!(resp["previous_response_id"], "resp_prev");
        assert_eq!(resp["instructions"], "Be concise");
        assert_eq!(resp["temperature"], 0.2);
        assert_eq!(resp["top_p"], 0.9);
        assert_eq!(resp["max_output_tokens"], 123);
        assert_eq!(
            resp["tools"],
            json!([{"type": "function", "name": "shell"}])
        );
        assert_eq!(resp["truncation"], "disabled");
    }
}
