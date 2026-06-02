//! Error Mapper — rewrite upstream error responses into the shape the client
//! expects.
//!
//! When AgentGate converts between protocols (e.g. Codex Responses → DeepSeek
//! Chat Completions), a 4xx/5xx from upstream comes back in the upstream's
//! error shape. The client expects its own protocol's error shape:
//!   - **OpenAI Responses / Chat Completions** → `{"error":{"message","type","code"}}`
//!   - **Anthropic Messages** → `{"type":"error","error":{"type","message"}}`
//!   - **Gemini** → `{"error":{"code","message","status"}}`
//!
//! This module normalises upstream errors into the requested client shape so
//! the client can parse / retry / surface them correctly.
//!
//! Returns `Some(MappedError)` when the body was rewritten; `None` means
//! "leave the response alone" (e.g. the upstream already matches the client
//! protocol, or we couldn't extract a reasonable code from the upstream
//! shape).
//!
//! Provider quirks contribute via `error_code_overrides` — a map from
//! upstream code/string → standardised code (e.g. DeepSeek's
//! `"insufficient_balance"` → `"insufficient_quota"`).

use serde_json::{json, Value};

use crate::gateway::refiner_log::ErrorMapperAction;
use crate::models::provider::Provider;

#[derive(Debug, Clone)]
pub struct MappedError {
    /// Reshaped body matching the client protocol.
    pub body: Value,
    /// Structured action for trace_json.
    pub action: ErrorMapperAction,
}

/// Map an upstream error response into the shape expected by `client_protocol`.
/// `body` is the parsed JSON body returned by upstream; `status` is the HTTP
/// status code. Returns `None` when nothing to map (no error structure found).
pub fn apply(
    provider: &Provider,
    body: &Value,
    status: u16,
    client_protocol: &str,
) -> Option<MappedError> {
    let (upstream_code, upstream_message) = extract_upstream(body, status);
    if upstream_code.is_none() && upstream_message.is_none() && !(status >= 400) {
        return None;
    }

    let quirks = super::resolve_quirks(provider);
    let mapped_code = upstream_code
        .as_deref()
        .and_then(|c| quirks.error_code_overrides.get(c).cloned())
        .unwrap_or_else(|| classify(status, upstream_code.as_deref()));
    let mapped_message = upstream_message.clone().unwrap_or_else(|| {
        // Fall back to a generic message if upstream gave us nothing useful.
        match status {
            400 => "Bad request".into(),
            401 => "Unauthorized — check API key".into(),
            403 => "Forbidden".into(),
            404 => "Not found".into(),
            429 => "Rate limited".into(),
            500..=599 => "Upstream server error".into(),
            _ => format!("Request failed with status {status}"),
        }
    });

    let reshaped = reshape(client_protocol, &mapped_code, &mapped_message, status);
    Some(MappedError {
        body: reshaped,
        action: ErrorMapperAction {
            upstream_code,
            upstream_message,
            mapped_code,
            mapped_message,
        },
    })
}

/// Pull (code, message) out of common upstream shapes.
fn extract_upstream(body: &Value, _status: u16) -> (Option<String>, Option<String>) {
    let code = body.pointer("/error/code")
        .or_else(|| body.pointer("/error/type"))
        .and_then(|v| v.as_str())
        .map(String::from)
        // Gemini shape: `{"error":{"status":"PERMISSION_DENIED",...}}`
        .or_else(|| {
            body.pointer("/error/status")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
    let message = body.pointer("/error/message").and_then(|v| v.as_str()).map(String::from)
        .or_else(|| body.get("message").and_then(|v| v.as_str()).map(String::from));
    (code, message)
}

/// Default classification when the provider didn't supply a recognisable code.
/// Uses HTTP status to give the client a useful label.
fn classify(status: u16, upstream_code: Option<&str>) -> String {
    if let Some(c) = upstream_code {
        // Lowercase, snake-case the upstream code so clients can match on it.
        return normalise_code(c);
    }
    match status {
        400 => "invalid_request_error",
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        408 => "request_timeout",
        413 => "request_too_large",
        429 => "rate_limit_error",
        500 => "api_error",
        502 => "bad_gateway",
        503 => "service_unavailable",
        504 => "gateway_timeout",
        _ if (500..=599).contains(&status) => "api_error",
        _ => "unknown_error",
    }
    .into()
}

fn normalise_code(code: &str) -> String {
    code.trim()
        .replace(' ', "_")
        .replace('-', "_")
        .to_ascii_lowercase()
}

/// Reshape the error body into the client protocol's expected shape.
fn reshape(client_protocol: &str, code: &str, message: &str, status: u16) -> Value {
    match client_protocol {
        "anthropic_messages" => json!({
            "type": "error",
            "error": {
                "type": code,
                "message": message,
            }
        }),
        "gemini" => json!({
            "error": {
                "code": status,
                "message": message,
                "status": code.to_ascii_uppercase(),
            }
        }),
        // Default: OpenAI Responses / Chat Completions shape — same envelope.
        _ => json!({
            "error": {
                "message": message,
                "type": code,
                "code": code,
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(provider_type: &str) -> Provider {
        Provider {
            id: "p".into(), name: "P".into(), provider_type: provider_type.into(),
            base_url: "https://x".into(), api_key: Some("sk".into()),
            default_model: "m".into(), reasoning_model: None,
            supported_models: None, model_mapping: None, extra_headers: None,
            anthropic_base_url: None, responses_base_url: None,
            protocol: "openai_chat_completions".into(),
            timeout_seconds: 60, status: "ok".into(),
            supports_vision: None, auto_cache_control: None, supports_cache: None,
            model_capabilities: None,
            provider_quirks: None,
            body_filter_enabled: None,
            thinking_rectifier_enabled: None,
            error_mapper_enabled: None,
            model_degradation_chain: None,
            enabled: true, is_active: true,
            created_at: "now".into(), updated_at: "now".into(),
        }
    }

    #[test]
    fn openai_shape_to_anthropic_shape() {
        let p = provider("deepseek");
        let body = json!({
            "error": {"message": "rate limited", "type": "rate_limit_exceeded", "code": "rate_limit_exceeded"}
        });
        let m = apply(&p, &body, 429, "anthropic_messages").unwrap();
        assert_eq!(m.body["type"], "error");
        assert_eq!(m.body["error"]["type"], "rate_limit_exceeded");
        assert_eq!(m.body["error"]["message"], "rate limited");
        assert_eq!(m.action.mapped_code, "rate_limit_exceeded");
    }

    #[test]
    fn anthropic_shape_to_openai_shape() {
        let p = provider("anthropic");
        let body = json!({
            "type": "error",
            "error": {"type": "invalid_request_error", "message": "bad input"}
        });
        let m = apply(&p, &body, 400, "openai_responses").unwrap();
        assert_eq!(m.body["error"]["type"], "invalid_request_error");
        assert_eq!(m.body["error"]["message"], "bad input");
    }

    #[test]
    fn missing_message_falls_back_to_status_default() {
        let p = provider("deepseek");
        let body = json!({});
        let m = apply(&p, &body, 401, "openai_responses").unwrap();
        assert!(m
            .body
            .pointer("/error/message")
            .and_then(|v| v.as_str())
            .unwrap()
            .contains("API key"));
        assert_eq!(m.action.mapped_code, "authentication_error");
    }

    #[test]
    fn quirks_override_table_remaps_code() {
        let mut p = provider("deepseek");
        p.provider_quirks = Some(
            r#"{"error_code_overrides":{"insufficient_balance":"insufficient_quota"}}"#.into(),
        );
        let body = json!({
            "error": {"message": "out of credits", "code": "insufficient_balance"}
        });
        let m = apply(&p, &body, 402, "openai_responses").unwrap();
        assert_eq!(m.action.mapped_code, "insufficient_quota");
        assert_eq!(m.body["error"]["code"], "insufficient_quota");
    }

    #[test]
    fn gemini_shape_target() {
        let p = provider("openai");
        let body = json!({
            "error": {"message": "denied", "code": "permission_denied"}
        });
        let m = apply(&p, &body, 403, "gemini").unwrap();
        assert_eq!(m.body["error"]["code"], 403);
        assert_eq!(m.body["error"]["status"], "PERMISSION_DENIED");
    }

    #[test]
    fn upstream_with_no_error_envelope_and_success_status_is_noop() {
        let p = provider("openai");
        let body = json!({"ok": true});
        assert!(apply(&p, &body, 200, "openai_responses").is_none());
    }

    #[test]
    fn gemini_status_field_is_picked_up() {
        let p = provider("gemini");
        let body = json!({"error": {"status": "RESOURCE_EXHAUSTED", "message": "quota"}});
        let m = apply(&p, &body, 429, "openai_responses").unwrap();
        assert_eq!(m.action.upstream_code.as_deref(), Some("RESOURCE_EXHAUSTED"));
        assert_eq!(m.action.mapped_code, "resource_exhausted");
    }

    #[test]
    fn upstream_dash_separated_code_normalised_to_snake() {
        let p = provider("openai");
        let body = json!({"error": {"code": "Rate-Limit-Exceeded", "message": "x"}});
        let m = apply(&p, &body, 429, "openai_responses").unwrap();
        assert_eq!(m.action.mapped_code, "rate_limit_exceeded");
    }
}
