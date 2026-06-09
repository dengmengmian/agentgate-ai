use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use futures::StreamExt;
use rusqlite::Connection;
use serde_json::json;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::errors::AppError;
use crate::providers::adapter::ProviderConfig;

const MAX_LOG_BODY: usize = 50_000;
const MAX_SSE_LOG: usize = 1_000_000;

/// Anthropic 透传路径需要把客户端的几个 protocol-level header 透到上游，
/// 否则用户没法启用 context-1m / prompt-caching 这类 beta 能力。
/// 名单严格白名单：authorization / host / hop-by-hop / cookie 类**不**列入。
const ANTHROPIC_FORWARD_HEADERS: &[&str] = &["anthropic-beta", "anthropic-version"];

/// OpenAI 兼容透传路径转发的客户端 header 白名单。
const OPENAI_FORWARD_HEADERS: &[&str] = &["openai-beta", "openai-organization", "openai-project"];

// 注：pass_through 当前**不**转发上游响应 header（只显式设 Content-Type），
// 所以不需要 hop-by-hop 黑名单。未来若加 anthropic-ratelimit-* / x-request-id
// 等上游响应 header 转发，必须先过滤掉 RFC 7230 hop-by-hop 头：
// connection / keep-alive / proxy-authenticate / proxy-authorization / te /
// trailer / transfer-encoding / upgrade。

fn forward_client_headers(
    mut builder: reqwest::RequestBuilder,
    client_headers: Option<&HeaderMap>,
    whitelist: &[&str],
) -> reqwest::RequestBuilder {
    let Some(headers) = client_headers else {
        return builder;
    };
    for name in whitelist {
        if let Some(v) = headers.get(*name).and_then(|h| h.to_str().ok()) {
            if !v.is_empty() {
                builder = builder.header(*name, v);
            }
        }
    }
    builder
}

/// Handle a native upstream pass-through request (stream or non-stream).
pub async fn handle(
    http_client: &reqwest::Client,
    db: &crate::storage::db::DbPool,
    config: &ProviderConfig,
    target_url: &str,
    route: &str,
    client_protocol: &str,
    raw_body: &str,
    model_override: Option<&str>,
    request_id: &str,
    start: Instant,
    client_type: &str,
    client_headers: Option<&HeaderMap>,
) -> Result<Response, AppError> {
    let mut body_json: serde_json::Value =
        serde_json::from_str(raw_body).unwrap_or(serde_json::json!({}));

    let is_stream = body_json
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Native pass-through is transparent by default: preserve the request model.
    // Only rewrite when the caller supplies an explicit override (for example,
    // model_mapping). If the client omitted model entirely, fall back to default.
    let requested = body_json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let (model, model_resolution) =
        resolve_native_model(requested, model_override, &config.default_model);
    let trace_mode = native_trace_mode(model_override);
    body_json["model"] = serde_json::json!(&model);
    let rewritten_body = body_json.to_string();

    if is_stream {
        handle_stream(
            http_client,
            db,
            config,
            target_url,
            route,
            client_protocol,
            &rewritten_body,
            request_id,
            &model,
            trace_mode,
            model_resolution,
            start,
            client_type,
            client_headers,
        )
        .await
    } else {
        handle_non_stream(
            http_client,
            db,
            config,
            target_url,
            route,
            client_protocol,
            &rewritten_body,
            request_id,
            &model,
            trace_mode,
            model_resolution,
            start,
            client_type,
            client_headers,
        )
        .await
    }
}

fn resolve_native_model(
    requested: &str,
    model_override: Option<&str>,
    default_model: &str,
) -> (String, &'static str) {
    if let Some(mapped) = model_override {
        return (mapped.to_string(), "model_mapping");
    }
    if requested.is_empty() {
        return (default_model.to_string(), "default_model");
    }
    (requested.to_string(), "request_model")
}

fn native_trace_mode(model_override: Option<&str>) -> &'static str {
    if model_override.is_some() {
        "native_pass_through_model_mapping"
    } else {
        "native_pass_through"
    }
}

async fn handle_non_stream(
    http_client: &reqwest::Client,
    db: &crate::storage::db::DbPool,
    config: &ProviderConfig,
    target_url: &str,
    route: &str,
    client_protocol: &str,
    raw_body: &str,
    request_id: &str,
    model: &str,
    trace_mode: &str,
    model_resolution: &str,
    start: Instant,
    client_type: &str,
    client_headers: Option<&HeaderMap>,
) -> Result<Response, AppError> {
    let resp = crate::providers::adapter::send_with_net_retry(
        || {
            let b = http_client
                .post(target_url)
                .header(
                    "Authorization",
                    format!("Bearer {}", config.select_api_key()),
                )
                .header("Content-Type", "application/json");
            forward_client_headers(b, client_headers, OPENAI_FORWARD_HEADERS)
                .body(raw_body.to_string())
        },
        1,
    )
    .await
    .map_err(|e| {
        AppError::new(
            crate::errors::codes::PASS_THROUGH_REQUEST_FAILED,
            format!("Failed to connect to provider: {e}"),
        )
    })?;

    let upstream_status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    let sanitized_response = sanitize(&body_text, config.api_key());
    let latency = start.elapsed().as_millis() as i64;

    let trace = json!({
        "mode": trace_mode,
        "client_protocol": client_protocol,
        "provider_protocol": client_protocol,
        "model_resolution": model_resolution,
        "route": route,
        "target_url": target_url,
        "upstream_status": upstream_status.as_u16(),
    })
    .to_string();

    let status_code = upstream_status.as_u16() as i64;
    let error_msg = if upstream_status.is_success() {
        None
    } else {
        Some(truncate(&sanitized_response, 2000))
    };

    log_to_db(
        db,
        client_type,
        route,
        request_id,
        &config.name,
        model,
        &sanitize(raw_body, config.api_key()),
        &sanitized_response,
        error_msg.as_deref(),
        &trace,
        status_code,
        latency,
    );

    let axum_status =
        StatusCode::from_u16(upstream_status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

    Ok(Response::builder()
        .status(axum_status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body_text))
        .unwrap())
}

async fn handle_stream(
    http_client: &reqwest::Client,
    db: &crate::storage::db::DbPool,
    config: &ProviderConfig,
    target_url: &str,
    route: &str,
    client_protocol: &str,
    raw_body: &str,
    request_id: &str,
    model: &str,
    trace_mode: &str,
    model_resolution: &str,
    start: Instant,
    client_type: &str,
    client_headers: Option<&HeaderMap>,
) -> Result<Response, AppError> {
    let resp = crate::providers::adapter::send_with_net_retry(
        || {
            let b = http_client
                .post(target_url)
                .header(
                    "Authorization",
                    format!("Bearer {}", config.select_api_key()),
                )
                .header("Content-Type", "application/json")
                .header("Accept", "text/event-stream");
            forward_client_headers(b, client_headers, OPENAI_FORWARD_HEADERS)
                .body(raw_body.to_string())
        },
        1,
    )
    .await
    .map_err(|e| {
        AppError::new(
            crate::errors::codes::PASS_THROUGH_STREAM_FAILED,
            format!("Failed to connect to provider: {e}"),
        )
    })?;

    let upstream_status = resp.status();
    if !upstream_status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = sanitize(&body_text, config.api_key());
        let latency = start.elapsed().as_millis() as i64;

        let trace = json!({
            "mode": trace_mode,
            "client_protocol": client_protocol,
            "provider_protocol": client_protocol,
            "model_resolution": model_resolution,
            "route": route,
            "target_url": target_url,
            "upstream_status": upstream_status.as_u16(),
        })
        .to_string();

        log_to_db(
            db,
            client_type,
            route,
            request_id,
            &config.name,
            model,
            &sanitize(raw_body, config.api_key()),
            "",
            Some(&truncate(&sanitized, 2000)),
            &trace,
            upstream_status.as_u16() as i64,
            latency,
        );

        let axum_status =
            StatusCode::from_u16(upstream_status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);

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
    let route_owned = route.to_string();
    let client_protocol_owned = client_protocol.to_string();
    let trace_mode_owned = trace_mode.to_string();
    let model_resolution_owned = model_resolution.to_string();
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
            if tx.send(prefix_text).await.is_err() {
                // Client dropped before first byte landed—放弃 stream，避免
                // 继续把上游 token 灌进黑洞。
                return;
            }
        }

        let mut stream = boot.stream;
        // 末尾缓冲：usage chunk 在流末尾，而 sse_log 到上限就丢后面、会截掉 usage。
        // 单独保留最后 ~16KB 专门解析 usage（够装最后的 usage chunk）。旁路，不碰转发。
        let mut usage_tail = String::new();
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
                    usage_tail.push_str(&text);
                    if usage_tail.len() > 16384 {
                        let cut = usage_tail.len() - 16384;
                        let mut b = cut;
                        while b < usage_tail.len() && !usage_tail.is_char_boundary(b) {
                            b += 1;
                        }
                        usage_tail.drain(..b);
                    }
                    // tx.send 在 client 已断开（mpsc receiver drop）时返回 Err。
                    // 显式 break——不然 reqwest 仍在从上游读，浪费 token + 占
                    // 用 keep-alive 连接。
                    if tx.send(text).await.is_err() {
                        break;
                    }
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
            "mode": &trace_mode_owned,
            "client_protocol": &client_protocol_owned,
            "provider_protocol": &client_protocol_owned,
            "model_resolution": &model_resolution_owned,
            "route": &route_owned,
            "target_url": &target,
            "stream": true,
            "sse_bytes": sse_size,
        })
        .to_string();

        let sanitized_sse = sanitize(&sse_log, &api_key);
        if let Some(conn) = lock_db(&db_clone) {
            // 旁路解析直通响应里的 token usage（流已原样转发，这里只读不改），
            // 有则记 token + 算成本；解析不出保持现状（None）。
            let (inp, out) = match parse_chat_usage(&usage_tail) {
                Some((i, o)) => (Some(i), Some(o)),
                None => (None, None),
            };
            let cost = if inp.is_some() || out.is_some() {
                crate::storage::pricing::calculate_cost_for_request(
                    &conn,
                    &provider_name,
                    &model_clone,
                    inp,
                    out,
                )
            } else {
                None
            };
            let _ = crate::storage::request_logs::insert(
                &conn,
                &req_id,
                &client_type_owned,
                &provider_name,
                &model_clone,
                &route_owned,
                200,
                latency,
                Some(&raw_req),
                None,
                None,
                None,
                Some(&truncate(&sanitized_sse, MAX_SSE_LOG)),
                None,
                None,
                Some(&with_route_decision(
                    &conn,
                    &route_owned,
                    &provider_name,
                    &model_clone,
                    &raw_req,
                    &trace,
                )),
                inp,
                out,
                cost,
                None,
                None, // no cache tokens
                Some("gateway"),
                None,
                Some(&req_id),
            );
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

/// 从连接池借一个连接,池满 / 超时返回 None(调用方决定怎么兜底)。
fn lock_db(
    db: &crate::storage::db::DbPool,
) -> Option<r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>> {
    db.get().ok()
}

/// 给直通日志的 trace 补 route_decision（按协议反推默认 profile），让「按策略」统计
/// 能拿到数据——直通路径之前不写路由决策。纯日志旁路：enrich 失败保留原 trace，
/// 绝不碰转发/转换/路由。
fn with_route_decision(
    conn: &Connection,
    route: &str,
    provider: &str,
    model: &str,
    raw_request: &str,
    trace: &str,
) -> String {
    crate::gateway::routes::enrich_trace_with_route_decision(
        conn,
        route,
        provider,
        model,
        raw_request,
        Some(trace),
    )
    .unwrap_or_else(|| trace.to_string())
}

/// 从 Chat Completions 流式响应尾部 SSE 文本里解析最后一条非 null 的 usage，
/// 返回 (prompt_tokens, completion_tokens)。直通模式不转换流，这里只**旁路**读
/// usage 做 token/成本统计——不碰转发、解析失败返回 None 保持现状。
fn parse_chat_usage(sse_tail: &str) -> Option<(i64, i64)> {
    for line in sse_tail.lines().rev() {
        let data = line
            .strip_prefix("data:")
            .map(str::trim)
            .unwrap_or_else(|| line.trim());
        if data.is_empty() || data == "[DONE]" || !data.contains("\"usage\"") {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
            if let Some(usage) = v.get("usage").filter(|u| !u.is_null()) {
                let inp = usage
                    .get("prompt_tokens")
                    .and_then(serde_json::Value::as_i64);
                let out = usage
                    .get("completion_tokens")
                    .and_then(serde_json::Value::as_i64);
                if inp.is_some() || out.is_some() {
                    return Some((inp.unwrap_or(0), out.unwrap_or(0)));
                }
            }
        }
    }
    None
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
    db: &crate::storage::db::DbPool,
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
        let enriched = with_route_decision(&conn, route, provider, model, raw_request, trace_json);
        let _ = crate::storage::request_logs::insert(
            &conn,
            request_id,
            client_type,
            provider,
            model,
            route,
            status_code,
            latency_ms,
            Some(raw_request),
            None,
            if raw_response.is_empty() {
                None
            } else {
                Some(raw_response)
            },
            None,
            None,
            None,
            error_message,
            Some(&enriched),
            None,
            None,
            None,
            None,
            None,
            Some("gateway"),
            None,
            Some(request_id),
        );
    }
}

/// Anthropic Messages API pass-through — forward directly to provider's Anthropic endpoint.
/// Used when provider has `anthropic_base_url` set (e.g. DeepSeek, Kimi).
pub async fn handle_anthropic(
    http_client: &reqwest::Client,
    db: &crate::storage::db::DbPool,
    config: &ProviderConfig,
    target_url: &str,
    raw_body: &str,
    model_override: Option<&str>,
    request_id: &str,
    start: Instant,
    client_type: &str,
    client_headers: Option<&HeaderMap>,
) -> Result<Response, AppError> {
    let is_stream = serde_json::from_str::<serde_json::Value>(raw_body)
        .ok()
        .and_then(|v| v.get("stream")?.as_bool())
        .unwrap_or(false);

    let mut body_json: serde_json::Value =
        serde_json::from_str(raw_body).unwrap_or(serde_json::json!({}));
    let requested = body_json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let (base_model, model_resolution) =
        resolve_native_model(requested, model_override, &config.default_model);
    let trace_mode = native_trace_mode(model_override);
    // Provider-specific final model cleanup for Anthropic passthrough.
    // OpenAI/Codex paths use their own resolved model value before reaching
    // this handler.
    let model =
        crate::gateway::anthropic_model_suffix::for_anthropic(&config.provider_type, &base_model);
    body_json["model"] = serde_json::json!(&model);
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
            // 默认 anthropic-version；若 client 显式带了同名 header，
            // 下面 forward_client_headers 会覆盖这条——reqwest 同 header
            // 重复 set 会保留最后一次的值。
            .header("anthropic-version", "2023-06-01");
        for (k, v) in &config.extra_headers {
            b = b.header(k.as_str(), v.as_str());
        }
        // 把 client 的 anthropic-beta（如 context-1m-2025-08-07）+
        // anthropic-version（如果 client 想用更新版本）透到上游。其它
        // header 一律不转发（host / authorization 等敏感字段不能漏出去）。
        b = forward_client_headers(b, client_headers, ANTHROPIC_FORWARD_HEADERS);
        b.body(rewritten_body.clone())
    };

    if is_stream {
        // Stream pass-through
        let resp = crate::providers::adapter::send_with_net_retry(&build_request, 1)
            .await
            .map_err(|e| {
                AppError::new(
                    crate::errors::codes::PASS_THROUGH_REQUEST_FAILED,
                    format!("Failed: {e}"),
                )
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let sanitized = sanitize(&body_text, config.api_key());
            let latency = start.elapsed().as_millis() as i64;
            log_to_db(
                db,
                client_type,
                "/v1/messages",
                request_id,
                &config.name,
                &model,
                &sanitize(raw_body, config.api_key()),
                "",
                Some(&truncate(&sanitized, 2000)),
                &json!({"mode":trace_mode,"target":target_url,"model_resolution":model_resolution})
                    .to_string(),
                status.as_u16() as i64,
                latency,
            );
            let axum_status =
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            return Ok(Response::builder()
                .status(axum_status)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body_text))
                .unwrap());
        }

        // Pipe SSE stream
        let (tx, rx) = mpsc::channel::<String>(512);
        let db_clone = db.clone();
        let provider_name = config.name.clone();
        let model = model.clone();
        let req_id = request_id.to_string();
        let raw_req = sanitize(raw_body, config.api_key());
        let target = target_url.to_string();
        let api_key = config.api_key().to_string();
        let client_type_owned = client_type.to_string();
        let trace_mode_owned = trace_mode.to_string();
        let model_resolution_owned = model_resolution.to_string();

        tokio::spawn(async move {
            let mut stream = resp.bytes_stream();
            let mut sse_log = String::new();
            let mut sse_size: usize = 0;
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes).to_string();
                        if sse_size < MAX_SSE_LOG {
                            let to_add = text.len().min(MAX_SSE_LOG - sse_size);
                            sse_log.push_str(&text[..to_add]);
                            sse_size += to_add;
                        }
                        // Client 断开则提前退出，省 upstream token。
                        if tx.send(text).await.is_err() {
                            break;
                        }
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
            let trace = json!({"mode":&trace_mode_owned,"target":&target,"model_resolution":&model_resolution_owned,"stream":true}).to_string();
            let sanitized_sse = sanitize(&sse_log, &api_key);
            if let Some(conn) = lock_db(&db_clone) {
                let _ = crate::storage::request_logs::insert(
                    &conn,
                    &req_id,
                    client_type_owned.as_str(),
                    &provider_name,
                    &model,
                    "/v1/messages",
                    200,
                    latency,
                    Some(&raw_req),
                    None,
                    None,
                    None,
                    Some(&truncate(&sanitized_sse, MAX_SSE_LOG)),
                    None,
                    None,
                    Some(&with_route_decision(
                        &conn,
                        "/v1/messages",
                        &provider_name,
                        &model,
                        &raw_req,
                        &trace,
                    )),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some("gateway"),
                    None,
                    Some(&req_id),
                );
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
            .body(body)
            .unwrap())
    } else {
        // Non-stream
        let resp = crate::providers::adapter::send_with_net_retry(&build_request, 1)
            .await
            .map_err(|e| {
                AppError::new(
                    crate::errors::codes::PASS_THROUGH_REQUEST_FAILED,
                    format!("Failed: {e}"),
                )
            })?;
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = sanitize(&body_text, config.api_key());
        let latency = start.elapsed().as_millis() as i64;
        let trace =
            json!({"mode":trace_mode,"target":target_url,"model_resolution":model_resolution})
                .to_string();
        let err_msg = if status.is_success() {
            None
        } else {
            Some(truncate(&sanitized, 2000))
        };
        log_to_db(
            db,
            client_type,
            "/v1/messages",
            request_id,
            &config.name,
            &model,
            &sanitize(raw_body, config.api_key()),
            &sanitized,
            err_msg.as_deref(),
            &trace,
            status.as_u16() as i64,
            latency,
        );
        let axum_status = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        Ok(Response::builder()
            .status(axum_status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body_text))
            .unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_usage_picks_final_non_null_chunk() {
        let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}],\"usage\":null}\n\n\
                   data: {\"choices\":[],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":42}}\n\n\
                   data: [DONE]\n\n";
        assert_eq!(parse_chat_usage(sse), Some((100, 42)));
    }

    #[test]
    fn parse_usage_none_when_all_null() {
        let sse = "data: {\"usage\":null}\n\ndata: [DONE]\n\n";
        assert_eq!(parse_chat_usage(sse), None);
    }

    #[test]
    fn native_model_mapping_wins() {
        assert_eq!(
            resolve_native_model(
                "claude-sonnet-4-6",
                Some("mimo-v2.5-pro[1m]"),
                "mimo-v2.5-pro"
            ),
            ("mimo-v2.5-pro[1m]".to_string(), "model_mapping")
        );
    }

    #[test]
    fn native_model_preserves_request_when_unmapped() {
        assert_eq!(
            resolve_native_model("mimo-v2.5-pro", None, "mimo-v2.5"),
            ("mimo-v2.5-pro".to_string(), "request_model")
        );
    }

    #[test]
    fn native_model_uses_default_only_when_missing() {
        assert_eq!(
            resolve_native_model("", None, "mimo-v2.5-pro"),
            ("mimo-v2.5-pro".to_string(), "default_model")
        );
    }

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
