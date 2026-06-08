use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use rusqlite::Connection;
use serde_json::{json, Value};

use crate::errors::AppError;
use crate::models::provider::Provider;
use crate::protocol::openai_responses::ResponsesRequest;
use crate::security::local_token;

/// Run refiner pipeline on a Value-shaped outbound request body, mutating it
/// in place. Returns the RefinerLog (current callers ignore it pending the
/// trace_json wiring change; once that lands, every handler should stash it
/// into the request log). Failing to lock the DB or read settings degrades
/// to no-op — the gateway should still forward the request transparently.
pub(crate) fn refine_value_body(
    db: &crate::storage::db::DbPool,
    provider: &Provider,
    body: &mut Value,
) -> crate::gateway::refiner_log::RefinerLog {
    let settings = match db
        .get()
        .ok()
        .and_then(|c| crate::storage::gateway_settings::get(&c).ok())
    {
        Some(s) => s,
        None => return crate::gateway::refiner_log::RefinerLog::default(),
    };
    crate::gateway::refiners::runtime::apply_request(provider, &settings, body)
}

/// Convenience wrapper: serde-ify a serializable request struct, run the
/// refiner pipeline against the JSON view, then ask serde to materialise the
/// modified struct back. If either serde leg fails the original struct is
/// returned untouched — refiner errors must never block the request.
pub(crate) fn refine_struct_body<T>(
    db: &crate::storage::db::DbPool,
    provider: &Provider,
    req: &mut T,
) -> crate::gateway::refiner_log::RefinerLog
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
///   - AgentGate Pet:         "AgentGate-Pet/..."
///   - generic SDKs:          "Python/requests", "node-fetch", "axios", etc.
pub(crate) fn detect_client_from_ua(headers: &HeaderMap, route_default: &str) -> String {
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .trim();
    if ua.is_empty() {
        return route_default.to_string();
    }
    let lower = ua.to_ascii_lowercase();
    // Order matters: more specific matches first.
    if lower.contains("agentgate-pet") {
        return "Pet".to_string();
    }
    if lower.contains("claude-code")
        || lower.contains("claude-cli")
        || lower.contains("claude code")
    {
        return "Claude Code".to_string();
    }
    if lower.contains("codex-cli") || lower.starts_with("codex/") {
        return "Codex".to_string();
    }
    if lower.contains("opencode") {
        return "OpenCode".to_string();
    }
    if lower.contains("atomcode") {
        return "AtomCode".to_string();
    }
    if lower.contains("kimicli") || lower.contains("kimi-cli") || lower.contains("kimi cli") {
        return "Kimi CLI".to_string();
    }
    if lower.contains("cursor") {
        return "Cursor".to_string();
    }
    if lower.contains("cherry") {
        return "Cherry Studio".to_string();
    }
    if lower.contains("continue") {
        return "Continue".to_string();
    }
    if lower.contains("cline") {
        return "Cline".to_string();
    }
    if lower.contains("roo") {
        return "Roo Code".to_string();
    }
    if lower.contains("hermes") {
        return "Hermes".to_string();
    }
    if lower.contains("opencode") {
        return "OpenCode".to_string();
    }
    if lower.starts_with("openai/") || lower.contains("openai-python") {
        // Codex CLI desktop reports "OpenAI/Python ..." too; treat as Codex
        // when the route is the Responses API.
        if route_default == "Codex" {
            return "Codex".to_string();
        }
        return "OpenAI SDK".to_string();
    }
    if lower.contains("anthropic-sdk") || lower.starts_with("anthropic/") {
        return "Anthropic SDK".to_string();
    }
    if lower.starts_with("python") || lower.contains("python-requests") || lower.contains("httpx") {
        return "Python SDK".to_string();
    }
    if lower.starts_with("node")
        || lower.contains("node-fetch")
        || lower.contains("axios")
        || lower.contains("undici")
    {
        return "Node SDK".to_string();
    }
    if lower.starts_with("curl") {
        return "curl".to_string();
    }
    // Unknown — surface the raw first token (helps users identify new clients)
    let token: String = ua
        .split_whitespace()
        .next()
        .unwrap_or(ua)
        .chars()
        .take(40)
        .collect();
    if token.is_empty() {
        route_default.to_string()
    } else {
        token
    }
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
        (
            auth_header.strip_prefix("Bearer ").unwrap_or(auth_header),
            "authorization",
        )
    };

    if token.is_empty() {
        return Err(GatewayError(AppError::new(
            crate::errors::codes::GATEWAY_AUTH_MISSING,
            "Gateway access token is missing",
        ).with_detail("The request does not include Authorization: Bearer <token> or X-Api-Key <token>")
         .with_suggestion("Re-apply the tool configuration from AgentGate or check the token file")));
    }

    if !local_token::validate_token(token) {
        return Err(GatewayError(AppError::new(
            crate::errors::codes::GATEWAY_AUTH_INVALID,
            "Gateway access token is invalid",
        ).with_suggestion(format!("Token received via '{source}' header does not match. Regenerate the token and re-apply tool configuration"))));
    }

    Ok(())
}

pub(crate) fn get_active_provider(db: &crate::storage::db::DbPool) -> Result<Provider, GatewayError> {
    let conn = db
        .get()
        .map_err(|_| GatewayError(AppError::internal("DB lock failed")))?;
    let settings = crate::storage::gateway_settings::get(&conn)?;

    let provider_id = settings.active_provider_id.ok_or_else(|| {
        GatewayError(
            AppError::new(crate::errors::codes::ACTIVE_PROVIDER_NOT_FOUND, "No active provider configured")
                .with_suggestion("Set an active provider in the Providers page"),
        )
    })?;

    let provider = crate::storage::providers::get_by_id(&conn, &provider_id).map_err(|_| {
        GatewayError(
            AppError::new(
                crate::errors::codes::ACTIVE_PROVIDER_NOT_FOUND,
                "Active provider not found in database",
            )
            .with_suggestion("Set a new active provider in the Providers page"),
        )
    })?;

    Ok(provider)
}

const AGENTGATE_VIRTUAL_MODEL: &str = "agentgate";

pub(crate) fn native_model_override(
    provider: &Provider,
    requested_model: Option<&str>,
    resolved_model: Option<&str>,
) -> Option<String> {
    let requested = requested_model?.trim();
    if requested.is_empty() {
        return None;
    }

    if is_agentgate_virtual_model(requested) {
        return Some(
            resolved_model
                .unwrap_or(&provider.default_model)
                .to_string(),
        );
    }

    explicit_model_mapping(provider, requested)
}

fn is_agentgate_virtual_model(requested: &str) -> bool {
    let model = requested
        .rsplit_once('/')
        .map(|(_, model)| model)
        .unwrap_or(requested);
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

pub(crate) fn request_contains_images(req: &ResponsesRequest) -> bool {
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

/// 取 `body.messages` 里最后一条 `role=="user"` 的 content,判断是否含指定 type 的图片块。
/// 语义与 [`request_contains_images`] 对齐:只看当前 turn,历史图片不算
/// ——避免某轮发图后整个会话被强制路由到 vision 模型。
fn last_user_content_has_images(body: &Value, image_types: &[&str]) -> bool {
    let messages = match body.get("messages").and_then(|m| m.as_array()) {
        Some(m) => m,
        None => return false,
    };
    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"));
    let content = match last_user.and_then(|m| m.get("content")) {
        Some(c) => c,
        None => return false,
    };
    match content {
        Value::Array(arr) => arr.iter().any(|item| {
            let t = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            image_types.contains(&t)
        }),
        _ => false,
    }
}

/// Chat Completions 请求体最后一条 user message 是否含图片(`image_url`)。
pub(crate) fn chat_request_has_images(body: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .map(|v| last_user_content_has_images(&v, &["image_url"]))
        .unwrap_or(false)
}

/// Anthropic Messages 请求体最后一条 user message 是否含图片(`image` block)。
pub(crate) fn anthropic_request_has_images(body: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .map(|v| last_user_content_has_images(&v, &["image"]))
        .unwrap_or(false)
}

pub(crate) fn sanitize_body(body: &str) -> String {
    // Simple api key sanitization in request bodies
    let mut s = body.to_string();
    // Match patterns like sk-... and redact them
    let mut search_from = 0;
    while let Some(offset) = s[search_from..].find("sk-") {
        let start = search_from + offset;
        let end = s[start..]
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
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

pub(crate) fn truncate_str(s: &str, max: usize) -> String {
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

pub(crate) fn trace_with_degradation_events(
    mut trace: serde_json::Value,
    events: &[crate::protocol::chat_completions::CapabilityDegradationEvent],
) -> String {
    if !events.is_empty() {
        trace["degradation_events"] = serde_json::json!(events);
    }
    trace.to_string()
}

fn protocol_for_log_route(route: &str) -> Option<&'static str> {
    match route {
        "/v1/responses" => Some("openai_responses"),
        "/v1/chat/completions" => Some("openai_chat_completions"),
        "/v1/messages" => Some("anthropic_messages"),
        _ => None,
    }
}

pub(crate) fn enrich_trace_with_route_decision(
    conn: &Connection,
    route: &str,
    provider_name: &str,
    model: &str,
    raw_request: &str,
    trace_json: Option<&str>,
) -> Option<String> {
    let mut trace = trace_json
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or_else(|| json!({}));
    if trace.get("route_decision").is_some() {
        return Some(trace.to_string());
    }

    let protocol = protocol_for_log_route(route)?;
    let profile = crate::storage::route_profiles::get_default_for_protocol(conn, protocol)
        .ok()
        .flatten()?;
    let providers = crate::storage::route_profiles::list_providers(conn, &profile.id).ok()?;
    let selected = providers.iter().find(|p| p.provider_name == provider_name);
    let request_has_images = route == "/v1/responses"
        && serde_json::from_str::<ResponsesRequest>(raw_request)
            .map(|req| request_contains_images(&req))
            .unwrap_or(false);
    let matched_conditions = selected
        .and_then(|p| p.routing_conditions.as_ref())
        .and_then(|s| serde_json::from_str::<Value>(s).ok());

    trace["route_decision"] = json!({
        "profile_id": profile.id,
        "profile_name": profile.name,
        "mode": profile.mode,
        "selected_provider_id": selected.map(|p| p.provider_id.as_str()),
        "selected_provider_name": provider_name,
        "selected_model": model,
        "selected_priority": selected.map(|p| p.priority),
        "matched_conditions": matched_conditions,
        "fallback_chain": route_fallback_chain(&providers, provider_name),
        "candidates": providers.iter().map(|p| {
            json!({
                "provider_id": p.provider_id,
                "provider_name": p.provider_name,
                "priority": p.priority,
                "model": p.model_override,
                "in_cooldown": p.cooldown_until.as_ref().map(|until| {
                    chrono::DateTime::parse_from_rfc3339(until)
                        .map(|cd| cd > chrono::Utc::now())
                        .unwrap_or(false)
                }).unwrap_or(false),
                "supports_vision": p.supports_vision,
                "has_conditions": p.routing_conditions.is_some(),
                "skip_reasons": route_candidate_skip_reasons(p, request_has_images),
            })
        }).collect::<Vec<_>>(),
    });
    Some(trace.to_string())
}

pub(crate) fn route_fallback_chain(
    providers: &[crate::models::route_profile::RouteProfileProviderView],
    selected_provider_name: &str,
) -> Vec<Value> {
    providers
        .iter()
        .enumerate()
        .map(|(idx, p)| {
            json!({
                "provider_id": p.provider_id,
                "provider_name": p.provider_name,
                "priority": p.priority,
                "role": if idx == 0 { "primary" } else { "fallback" },
                "step": idx + 1,
                "selected": p.provider_name == selected_provider_name,
            })
        })
        .collect()
}

pub(crate) fn route_candidate_skip_reasons(
    provider: &crate::models::route_profile::RouteProfileProviderView,
    request_has_images: bool,
) -> Vec<String> {
    let mut reasons = Vec::new();
    if !provider.enabled {
        reasons.push("disabled".to_string());
    }
    if !provider.runtime_available {
        reasons.push("runtime_unavailable".to_string());
    }
    let in_cooldown = provider.cooldown_until.as_ref().map(|until| {
        chrono::DateTime::parse_from_rfc3339(until)
            .map(|cd| cd > chrono::Utc::now())
            .unwrap_or(false)
    }).unwrap_or(false);
    if in_cooldown {
        reasons.push("cooldown".to_string());
    }
    if request_has_images && provider.supports_vision == Some(false) {
        reasons.push("unsupported_vision".to_string());
    }
    reasons
}

pub(crate) fn log_request_error(
    db: &crate::storage::db::DbPool,
    client_type: &str,
    route: &str,
    request_id: &str,
    raw_request: &str,
    converted_request: Option<&str>,
    err: &AppError,
    latency_ms: i64,
) {
    log_request_error_full(
        db,
        client_type,
        route,
        request_id,
        raw_request,
        converted_request.unwrap_or(""),
        "",
        "",
        err,
        if err.code == "RESPONSES_PARSE_ERROR" {
            400
        } else if err.code == "PROVIDER_API_KEY_MISSING" {
            401
        } else {
            500
        },
        latency_ms,
    );
}

/// 从连接池借一个连接,池满 / 超时返回 None(调用方决定怎么兜底)。
pub(crate) fn lock_db(
    db: &crate::storage::db::DbPool,
) -> Option<r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager>> {
    db.get().ok()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn log_request_success(
    db: &crate::storage::db::DbPool,
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
    usage: crate::gateway::usage::TokenUsage,
) {
    let crate::gateway::usage::TokenUsage {
        input: input_tokens,
        output: output_tokens,
        cache_write: cache_write_tokens,
        cache_read: cache_read_tokens,
    } = usage;
    if let Some(conn) = lock_db(db) {
        // Calculate cost from pricing table
        let cost = crate::storage::pricing::calculate_cost_for_request(
            &conn,
            provider,
            model,
            input_tokens,
            output_tokens,
        );
        let trace_json =
            enrich_trace_with_route_decision(&conn, route, provider, model, raw_request, trace_json);
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
            Some(converted_request),
            if raw_response.is_empty() {
                None
            } else {
                Some(raw_response)
            },
            if converted_response.is_empty() {
                None
            } else {
                Some(converted_response)
            },
            None,
            tool_calls,
            None,
            trace_json.as_deref(),
            input_tokens,
            output_tokens,
            cost,
            cache_write_tokens,
            cache_read_tokens,
            Some("gateway"),
            None,
            Some(request_id),
        );
    }
    // Prometheus 指标
    crate::gateway::metrics::record_request(
        route,
        client_type,
        provider,
        status_code as u16,
        latency_ms as f64 / 1000.0,
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

pub(crate) fn log_request_error_full(
    db: &crate::storage::db::DbPool,
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
    })
    .to_string();
    if let Some(conn) = lock_db(db) {
        let _ = crate::storage::request_logs::insert(
            &conn,
            request_id,
            client_type,
            if provider.is_empty() {
                "unknown"
            } else {
                provider
            },
            if model.is_empty() { "unknown" } else { model },
            route,
            status_code,
            latency_ms,
            Some(raw_request),
            if converted_request.is_empty() {
                None
            } else {
                Some(converted_request)
            },
            None,
            None,
            None,
            None,
            Some(&error_msg),
            Some(&trace),
            None,
            None,
            None, // no cost for errors
            None,
            None, // no cache tokens for errors
            Some("gateway"),
            None,
            Some(request_id),
        );
    }
    // Prometheus 指标（错误也算一次请求）
    crate::gateway::metrics::record_request(
        route,
        client_type,
        if provider.is_empty() {
            "unknown"
        } else {
            provider
        },
        status_code as u16,
        latency_ms as f64 / 1000.0,
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
            "RESPONSES_PARSE_ERROR"
            | "TRANSFORM_ERROR"
            | "TOOL_OUTPUT_NOT_FOUND"
            | "TOOL_CALL_NOT_FOUND" => StatusCode::BAD_REQUEST,
            "PROVIDER_API_KEY_MISSING" | "GATEWAY_AUTH_MISSING" | "GATEWAY_AUTH_INVALID" => {
                StatusCode::UNAUTHORIZED
            }
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

