use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::Response;
use futures::StreamExt;
use rusqlite::Connection;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::errors::AppError;
use crate::providers::adapter::ProviderConfig;

const MAX_LOG_BODY: usize = 50_000;
const MAX_SSE_LOG: usize = 1_000_000;

/// Handle a Chat Completions pass-through request (stream or non-stream).
pub async fn handle(
    http_client: &reqwest::Client,
    db: &Arc<Mutex<Connection>>,
    config: &ProviderConfig,
    target_url: &str,
    raw_body: &str,
    request_id: &str,
    start: Instant,
    client_type: &str,
) -> Result<Response, AppError> {
    let mut body_json: serde_json::Value = serde_json::from_str(raw_body)
        .unwrap_or(serde_json::json!({}));

    let is_stream = body_json.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    // Keep the model if it's in provider's supported list, otherwise use default.
    let requested = body_json.get("model").and_then(|v| v.as_str()).unwrap_or("");
    let is_known = requested == config.default_model
        || config.reasoning_model.as_deref() == Some(requested);
    // Note: pass_through doesn't have access to supported_models list from ProviderConfig,
    // but default_model + reasoning_model cover the main cases. For full list matching,
    // the provider_selector already resolved the correct model before pass_through is called.
    let model = if is_known { requested.to_string() } else { config.default_model.clone() };
    body_json["model"] = serde_json::json!(&model);
    let rewritten_body = body_json.to_string();

    if is_stream {
        handle_stream(http_client, db, config, target_url, &rewritten_body, request_id, &model, start, client_type).await
    } else {
        handle_non_stream(http_client, db, config, target_url, &rewritten_body, request_id, &model, start, client_type).await
    }
}

async fn handle_non_stream(
    http_client: &reqwest::Client,
    db: &Arc<Mutex<Connection>>,
    config: &ProviderConfig,
    target_url: &str,
    raw_body: &str,
    request_id: &str,
    model: &str,
    start: Instant,
    client_type: &str,
) -> Result<Response, AppError> {
    let resp = crate::providers::adapter::send_with_net_retry(|| {
        http_client
            .post(target_url)
            .header("Authorization", format!("Bearer {}", config.select_api_key()))
            .header("Content-Type", "application/json")
            .body(raw_body.to_string())
    }, 1).await
        .map_err(|e| AppError::new("PASS_THROUGH_REQUEST_FAILED",
            format!("Failed to connect to provider: {e}")))?;

    let upstream_status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    let sanitized_response = sanitize(&body_text, config.api_key());
    let latency = start.elapsed().as_millis() as i64;

    let trace = json!({
        "mode": "pass_through",
        "client_protocol": "openai_chat_completions",
        "provider_protocol": "openai_chat_completions",
        "route": "/v1/chat/completions",
        "target_url": target_url,
        "upstream_status": upstream_status.as_u16(),
    }).to_string();

    let status_code = upstream_status.as_u16() as i64;
    let error_msg = if upstream_status.is_success() {
        None
    } else {
        Some(truncate(&sanitized_response, 2000))
    };

    log_to_db(
        db, client_type, "/v1/chat/completions", request_id, &config.name, model,
        &sanitize(raw_body, config.api_key()),
        &sanitized_response,
        error_msg.as_deref(),
        &trace,
        status_code, latency,
    );

    let axum_status = StatusCode::from_u16(upstream_status.as_u16())
        .unwrap_or(StatusCode::BAD_GATEWAY);

    Ok(Response::builder()
        .status(axum_status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body_text))
        .unwrap())
}

async fn handle_stream(
    http_client: &reqwest::Client,
    db: &Arc<Mutex<Connection>>,
    config: &ProviderConfig,
    target_url: &str,
    raw_body: &str,
    request_id: &str,
    model: &str,
    start: Instant,
    client_type: &str,
) -> Result<Response, AppError> {
    let resp = crate::providers::adapter::send_with_net_retry(|| {
        http_client
            .post(target_url)
            .header("Authorization", format!("Bearer {}", config.select_api_key()))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .body(raw_body.to_string())
    }, 1).await
        .map_err(|e| AppError::new("PASS_THROUGH_STREAM_FAILED",
            format!("Failed to connect to provider: {e}")))?;

    let upstream_status = resp.status();
    if !upstream_status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = sanitize(&body_text, config.api_key());
        let latency = start.elapsed().as_millis() as i64;

        let trace = json!({
            "mode": "pass_through",
            "client_protocol": "openai_chat_completions",
            "provider_protocol": "openai_chat_completions",
            "route": "/v1/chat/completions",
            "target_url": target_url,
            "upstream_status": upstream_status.as_u16(),
        }).to_string();

        log_to_db(
            db, client_type, "/v1/chat/completions", request_id, &config.name, model,
            &sanitize(raw_body, config.api_key()),
            "", Some(&truncate(&sanitized, 2000)),
            &trace, upstream_status.as_u16() as i64, latency,
        );

        let axum_status = StatusCode::from_u16(upstream_status.as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY);

        return Ok(Response::builder()
            .status(axum_status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body_text))
            .unwrap());
    }

    // Bootstrap-validate the stream before committing to forwarding: catches
    // HTTP-200-with-error-frame failures (quota / rate-limit emitted mid-
    // stream by GLM / MiMo even on direct pass-through) and turns them into
    // a clean Err so the outer route loop can fail over.
    let boot = crate::gateway::sse_bootstrap::bootstrap_detect(resp).await?;

    // Stream: pipe upstream SSE to client, log asynchronously
    let (tx, rx) = mpsc::channel::<String>(512);
    let db_clone = db.clone();
    let provider_name = config.name.clone();
    let model_clone = model.to_string();
    let req_id = request_id.to_string();
    let raw_req = sanitize(raw_body, config.api_key());
    let target = target_url.to_string();
    let api_key = config.api_key().to_string();
    let client_type_owned = client_type.to_string();

    tokio::spawn(async move {
        let prefix_text = String::from_utf8_lossy(&boot.prefix).to_string();
        let mut sse_log = String::new();
        let mut sse_size: usize = 0;

        // Replay the bootstrap prefix first so any bytes already pulled
        // during the scan reach the client.
        if !prefix_text.is_empty() {
            let to_add = prefix_text.len().min(MAX_SSE_LOG);
            sse_log.push_str(&prefix_text[..to_add]);
            sse_size += to_add;
            let _ = tx.send(prefix_text).await;
        }

        let mut stream = boot.stream;
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    // Log (limited)
                    if sse_size < MAX_SSE_LOG {
                        let to_add = text.len().min(MAX_SSE_LOG - sse_size);
                        sse_log.push_str(&text[..to_add]);
                        sse_size += to_add;
                    }
                    let _ = tx.send(text).await;
                }
                Err(e) => {
                    let msg = crate::gateway::sse_bootstrap::describe_stream_error(&e);
                    let payload = format!(
                        "data: {}\n\n",
                        serde_json::json!({"error": {"message": msg, "type": "upstream_stream_idle"}})
                    );
                    let _ = tx.send(payload).await;
                    break;
                }
            }
        }

        let latency = start.elapsed().as_millis() as i64;
        let trace = serde_json::json!({
            "mode": "pass_through",
            "client_protocol": "openai_chat_completions",
            "provider_protocol": "openai_chat_completions",
            "route": "/v1/chat/completions",
            "target_url": &target,
            "stream": true,
            "sse_bytes": sse_size,
        }).to_string();

        let sanitized_sse = sanitize(&sse_log, &api_key);
        if let Some(conn) = lock_db(&db_clone) {
            let _ = crate::storage::request_logs::insert(
                &conn, &req_id, &client_type_owned, &provider_name, &model_clone,
                "/v1/chat/completions", 200, latency,
                Some(&raw_req), None,
                None, None,
                Some(&truncate(&sanitized_sse, MAX_SSE_LOG)),
                None, None,
                Some(&trace),
                None, None, None, // no cost / tokens parsed on stream pass-through
                None, None,        // no cache tokens
            );
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

/// Lock the DB, recovering from a poisoned Mutex if necessary.
fn lock_db(db: &Arc<Mutex<Connection>>) -> Option<std::sync::MutexGuard<'_, Connection>> {
    match db.lock() {
        Ok(guard) => Some(guard),
        Err(poisoned) => Some(poisoned.into_inner()),
    }
}

fn sanitize(text: &str, api_key: &str) -> String {
    let mut s = text.to_string();
    if api_key.len() > 4 {
        s = s.replace(api_key, "sk-***REDACTED***");
    }
    truncate(&s, MAX_LOG_BODY)
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Find the last char boundary at or before `max` to avoid panic on multibyte chars
    let mut boundary = max;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}...(truncated)", &s[..boundary])
}

fn log_to_db(
    db: &Arc<Mutex<Connection>>,
    client_type: &str,
    route: &str,
    request_id: &str,
    provider: &str,
    model: &str,
    raw_request: &str,
    raw_response: &str,
    error_message: Option<&str>,
    trace_json: &str,
    status_code: i64,
    latency_ms: i64,
) {
    if let Some(conn) = lock_db(db) {
        let _ = crate::storage::request_logs::insert(
            &conn, request_id, client_type, provider, model,
            route, status_code, latency_ms,
            Some(raw_request), None,
            if raw_response.is_empty() { None } else { Some(raw_response) },
            None, None, None,
            error_message,
            Some(trace_json),
            None, None, None,
            None, None,
        );
    }
}

/// Anthropic Messages API pass-through — forward directly to provider's Anthropic endpoint.
/// Used when provider has `anthropic_base_url` set (e.g. DeepSeek, Kimi).
pub async fn handle_anthropic(
    http_client: &reqwest::Client,
    db: &Arc<Mutex<Connection>>,
    config: &ProviderConfig,
    target_url: &str,
    raw_body: &str,
    request_id: &str,
    start: Instant,
    client_type: &str,
) -> Result<Response, AppError> {
    let is_stream = serde_json::from_str::<serde_json::Value>(raw_body)
        .ok()
        .and_then(|v| v.get("stream")?.as_bool())
        .unwrap_or(false);

    // Rewrite model using provider's resolve_model
    let mut body_json: serde_json::Value = serde_json::from_str(raw_body)
        .unwrap_or(serde_json::json!({}));
    if let Some(_requested) = body_json.get("model").and_then(|v| v.as_str()) {
        // Use model_mapping/supported_models/default_model resolution
        // We need the full Provider for resolve_model, but we only have ProviderConfig
        // Just use default_model as fallback since ProviderConfig doesn't have resolve_model
        let base_model = config.default_model.clone();
        // Apply CC-only model suffix rewrites (e.g. MiMo's [1m] for 1M context).
        // Only fires on the Anthropic passthrough path; the OpenAI/Codex path
        // is untouched, where these suffixes would be invalid.
        let model = crate::gateway::anthropic_model_suffix::for_anthropic(
            &config.provider_type, &base_model,
        );
        body_json["model"] = serde_json::json!(model);
    }
    let rewritten_body = body_json.to_string();

    // Anthropic uses x-api-key header instead of Bearer.
    // Builder is reconstructed inside the retry closure so a transient connect
    // failure (e.g. a dead keep-alive connection returned by the pool) can be
    // retried with a fresh connection.
    let build_request = || {
        let mut b = http_client
            .post(target_url)
            .header("x-api-key", config.select_api_key())
            .header("content-type", "application/json")
            .header("anthropic-version", "2023-06-01");
        for (k, v) in &config.extra_headers {
            b = b.header(k.as_str(), v.as_str());
        }
        b.body(rewritten_body.clone())
    };

    if is_stream {
        // Stream pass-through
        let resp = crate::providers::adapter::send_with_net_retry(&build_request, 1).await
            .map_err(|e| AppError::new("PASS_THROUGH_REQUEST_FAILED", format!("Failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let sanitized = sanitize(&body_text, config.api_key());
            let latency = start.elapsed().as_millis() as i64;
            log_to_db(db, client_type, "/v1/messages", request_id, &config.name, &config.default_model,
                &sanitize(raw_body, config.api_key()), "", Some(&truncate(&sanitized, 2000)),
                &json!({"mode":"anthropic_pass_through","target":target_url}).to_string(),
                status.as_u16() as i64, latency);
            let axum_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            return Ok(Response::builder().status(axum_status)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body_text)).unwrap());
        }

        // Pipe SSE stream
        let (tx, rx) = mpsc::channel::<String>(512);
        let db_clone = db.clone();
        let provider_name = config.name.clone();
        let model = config.default_model.clone();
        let req_id = request_id.to_string();
        let raw_req = sanitize(raw_body, config.api_key());
        let target = target_url.to_string();
        let api_key = config.api_key().to_string();
        let client_type_owned = client_type.to_string();

        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            let mut sse_log = String::new();
            let mut sse_size: usize = 0;
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).to_string();
                        if sse_size < MAX_SSE_LOG { let to_add = text.len().min(MAX_SSE_LOG - sse_size); sse_log.push_str(&text[..to_add]); sse_size += to_add; }
                        let _ = tx.send(text).await;
                    }
                    Err(e) => {
                        let msg = crate::gateway::sse_bootstrap::describe_stream_error(&e);
                        let payload = format!(
                            "event: error\ndata: {}\n\n",
                            json!({"type":"error","error":{"type":"upstream_stream_idle","message":msg}})
                        );
                        let _ = tx.send(payload).await;
                        break;
                    }
                }
            }
            let latency = start.elapsed().as_millis() as i64;
            let trace = json!({"mode":"anthropic_pass_through","target":&target,"stream":true}).to_string();
            let sanitized_sse = sanitize(&sse_log, &api_key);
            if let Some(conn) = lock_db(&db_clone) {
                let _ = crate::storage::request_logs::insert(
                    &conn, &req_id, client_type_owned.as_str(), &provider_name, &model,
                    "/v1/messages", 200, latency,
                    Some(&raw_req), None, None, None,
                    Some(&truncate(&sanitized_sse, MAX_SSE_LOG)), None, None, Some(&trace), None, None, None,
                    None, None,
                );
            }
        });

        let stream = ReceiverStream::new(rx);
        let body = Body::from_stream(
            tokio_stream::StreamExt::map(stream, |s| Ok::<_, std::convert::Infallible>(s))
        );
        Ok(Response::builder().status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(body).unwrap())
    } else {
        // Non-stream
        let resp = crate::providers::adapter::send_with_net_retry(&build_request, 1).await
            .map_err(|e| AppError::new("PASS_THROUGH_REQUEST_FAILED", format!("Failed: {e}")))?;
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = sanitize(&body_text, config.api_key());
        let latency = start.elapsed().as_millis() as i64;
        let trace = json!({"mode":"anthropic_pass_through","target":target_url}).to_string();
        let err_msg = if status.is_success() { None } else { Some(truncate(&sanitized, 2000)) };
        log_to_db(db, client_type, "/v1/messages", request_id, &config.name, &config.default_model,
            &sanitize(raw_body, config.api_key()), &sanitized,
            err_msg.as_deref(),
            &trace, status.as_u16() as i64, latency);
        let axum_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        Ok(Response::builder().status(axum_status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body_text)).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_replaces_api_key() {
        let text = "error: sk-abc123def456 is invalid";
        let result = sanitize(text, "sk-abc123def456");
        assert!(result.contains("sk-***REDACTED***"));
        assert!(!result.contains("sk-abc123def456"));
    }

    #[test]
    fn test_sanitize_no_change_if_key_short() {
        let text = "error: sk- is invalid";
        let result = sanitize(text, "sk-");
        // api_key.len() == 3 <= 4, so no replacement
        assert_eq!(result, text);
    }

    #[test]
    fn test_sanitize_truncates_long_text() {
        let text = "x".repeat(MAX_LOG_BODY + 100);
        let result = sanitize(&text, "sk-key");
        assert!(result.ends_with("...(truncated)"));
        assert!(result.len() < text.len());
    }

    #[test]
    fn test_truncate_within_limit() {
        let s = "short text";
        assert_eq!(truncate(s, 100), "short text");
    }

    #[test]
    fn test_truncate_exceeds_limit() {
        let s = "x".repeat(200);
        let result = truncate(&s, 100);
        assert!(result.starts_with("xxxxxxxxxx"));
        assert!(result.ends_with("...(truncated)"));
    }

    #[test]
    fn test_truncate_exact_limit() {
        let s = "x".repeat(50);
        assert_eq!(truncate(&s, 50), s);
    }

    #[test]
    fn test_truncate_chinese_boundary() {
        let s = "你好世界"; // 4 chars, 12 bytes
        // Truncate at byte 7 — inside "世" (bytes 6..9) → snap back to 6
        let result = truncate(s, 7);
        assert_eq!(result, "你好...(truncated)");
    }

    #[test]
    fn test_truncate_emoji_boundary() {
        let s = "hi🎉ok"; // "hi" 2B + 🎉 4B + "ok" 2B = 8B
        // Truncate at 3 — inside 🎉 → snap back to 2
        let result = truncate(s, 3);
        assert_eq!(result, "hi...(truncated)");
    }
}
