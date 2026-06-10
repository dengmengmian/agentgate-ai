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
    let mut mapped_code = upstream_code
        .as_deref()
        .and_then(|c| quirks.error_code_overrides.get(c).cloned())
        .unwrap_or_else(|| classify(status, upstream_code.as_deref()));
    let mut mapped_message = upstream_message.clone().unwrap_or_else(|| {
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

    // Context overflow semantic detection: upgrade generic 400 into a clearly
    // labelled `context_length_exceeded` so clients (and users) can act.
    if status == 400 && detect_context_overflow(&mapped_message, upstream_code.as_deref()) {
        mapped_code = "context_length_exceeded".to_string();
        mapped_message = format!(
            "{} — 建议：清理对话历史或换用更长上下文模型。",
            mapped_message.trim_end_matches(['。', '.', ' '])
        );
    }

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
    let code = body
        .pointer("/error/code")
        .or_else(|| body.pointer("/error/type"))
        .and_then(|v| v.as_str())
        .map(String::from)
        // Gemini shape: `{"error":{"status":"PERMISSION_DENIED",...}}`
        .or_else(|| {
            body.pointer("/error/status")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
    let message = body
        .pointer("/error/message")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            body.get("message")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
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

/// Heuristic: does this 400 response look like "input too long for model"?
///
/// Matches common English + Chinese phrasings across providers. Hits return
/// `true` so the caller can re-label the error as `context_length_exceeded`,
/// which is what clients (OpenAI SDK, Anthropic SDK, our own UI) check to
/// decide between "show error" vs "auto-trim history and retry".
///
/// Code path is checked first because some providers (OpenAI) already return
/// `context_length_exceeded` as the code — that's a free hit without regex.
fn detect_context_overflow(message: &str, upstream_code: Option<&str>) -> bool {
    if let Some(code) = upstream_code {
        let lc = code.to_ascii_lowercase();
        if lc.contains("context_length")
            || lc.contains("context length")
            || lc == "string_above_max_length"
        {
            return true;
        }
    }
    let m = message.to_ascii_lowercase();
    // English patterns — substring match, lowercased.
    const EN: &[&str] = &[
        "maximum context length",
        "max context length",
        "context length exceeded",
        "context window",
        "prompt is too long",
        "prompt too long",
        "input is too long",
        "input too long",
        "too many tokens",
        "tokens exceed",
        "reduce the length of the messages",
    ];
    if EN.iter().any(|p| m.contains(p)) {
        return true;
    }
    // Chinese patterns — keep raw, no lowercasing needed.
    const ZH: &[&str] = &[
        "上下文过长",
        "上下文超长",
        "上下文长度",
        "输入过长",
        "输入太长",
        "提示词过长",
        "超出最大长度",
        "tokens 超出",
        "tokens超出",
    ];
    ZH.iter().any(|p| message.contains(p))
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
            id: "p".into(),
            name: "P".into(),
            provider_type: provider_type.into(),
            base_url: "https://x".into(),
            api_key: Some("sk".into()),
            default_model: "m".into(),
            reasoning_model: None,
            supported_models: None,
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            responses_base_url: None,
            protocol: "openai_chat_completions".into(),
            timeout_seconds: 60,
            status: "ok".into(),
            supports_vision: None,
            auto_cache_control: None,
            supports_cache: None,
            model_capabilities: None,
            provider_quirks: None,
            body_filter_enabled: None,
            thinking_rectifier_enabled: None,
            error_mapper_enabled: None,
            model_degradation_chain: None,
            model_context_windows: None,
            enabled: true,
            is_active: true,
            created_at: "now".into(),
            updated_at: "now".into(),
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
        assert_eq!(
            m.action.upstream_code.as_deref(),
            Some("RESOURCE_EXHAUSTED")
        );
        assert_eq!(m.action.mapped_code, "resource_exhausted");
    }

    #[test]
    fn upstream_dash_separated_code_normalised_to_snake() {
        let p = provider("openai");
        let body = json!({"error": {"code": "Rate-Limit-Exceeded", "message": "x"}});
        let m = apply(&p, &body, 429, "openai_responses").unwrap();
        assert_eq!(m.action.mapped_code, "rate_limit_exceeded");
    }

    #[test]
    fn openai_context_overflow_message_detected() {
        let p = provider("openai");
        let body = json!({
            "error": {
                "message": "This model's maximum context length is 128000 tokens. However, your messages resulted in 130000 tokens. Please reduce the length of the messages.",
                "type": "invalid_request_error",
                "code": "context_length_exceeded"
            }
        });
        let m = apply(&p, &body, 400, "openai_responses").unwrap();
        assert_eq!(m.action.mapped_code, "context_length_exceeded");
        assert!(m.action.mapped_message.contains("建议"));
    }

    #[test]
    fn anthropic_prompt_too_long_detected() {
        let p = provider("anthropic");
        let body = json!({
            "type": "error",
            "error": {"type": "invalid_request_error", "message": "prompt is too long: 250000 tokens > 200000 maximum"}
        });
        let m = apply(&p, &body, 400, "anthropic_messages").unwrap();
        assert_eq!(m.body["error"]["type"], "context_length_exceeded");
    }

    #[test]
    fn chinese_overflow_phrasing_detected() {
        let p = provider("deepseek");
        let body = json!({
            "error": {"message": "输入过长，超过模型上下文长度限制", "code": "invalid_request_error"}
        });
        let m = apply(&p, &body, 400, "openai_responses").unwrap();
        assert_eq!(m.action.mapped_code, "context_length_exceeded");
    }

    #[test]
    fn unrelated_400_not_misclassified_as_overflow() {
        let p = provider("openai");
        let body = json!({
            "error": {"message": "missing required field: model", "type": "invalid_request_error"}
        });
        let m = apply(&p, &body, 400, "openai_responses").unwrap();
        assert_eq!(m.action.mapped_code, "invalid_request_error");
        assert!(!m.action.mapped_message.contains("建议"));
    }

    #[test]
    fn overflow_only_triggers_on_400_not_429() {
        // A 429 with a long-tokens message shouldn't get relabelled.
        let p = provider("openai");
        let body = json!({
            "error": {"message": "too many tokens per minute, slow down", "code": "rate_limit_exceeded"}
        });
        let m = apply(&p, &body, 429, "openai_responses").unwrap();
        assert_eq!(m.action.mapped_code, "rate_limit_exceeded");
    }
}
