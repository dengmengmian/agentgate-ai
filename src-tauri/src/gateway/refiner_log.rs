//! Structured per-request log of refiner activity, persisted to
//! `request_logs.trace_json`.
//!
//! Every "the gateway touched the bytes" event ends up here so the GUI's Logs
//! page can show *why* a request that the user sent as X reached upstream as Y
//! — without forcing the user to diff raw_request vs. converted_request line by
//! line. Each variant is optional so a transparent pass-through request still
//! produces a tiny JSON blob (or none at all).
//!
//! This module only defines the schema. Writers attach an instance to the
//! existing `trace_json` field in `request_logs::insert`. Refiner modules
//! (Body Filter, Thinking Rectifier, Error Mapper) will populate it in commit
//! #2; #1 ships the data structure so downstream consumers can compile.

use serde::{Deserialize, Serialize};

/// One top-level envelope per request. Serialized into `request_logs.trace_json`
/// alongside the existing trace data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefinerLog {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_filter: Option<BodyFilterAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_rectifier: Option<ThinkingRectifierAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_mapper: Option<ErrorMapperAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circuit_breaker: Option<CircuitBreakerEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degradation: Option<DegradationEvent>,
}

impl RefinerLog {
    /// True when no refiner acted — used to skip the JSON write entirely.
    pub fn is_empty(&self) -> bool {
        self.body_filter.is_none()
            && self.thinking_rectifier.is_none()
            && self.error_mapper.is_none()
            && self.circuit_breaker.is_none()
            && self.degradation.is_none()
    }
}

/// Records which top-level fields the body filter stripped before forwarding.
/// `reason` should name the rule that fired ("provider_quirks" /
/// "capability_mismatch") so the UI can offer "turn off this rule" actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BodyFilterAction {
    pub stripped_fields: Vec<String>,
    pub reason: String,
}

/// Records thinking/reasoning parameter rewrites. `from` and `to` are JSON
/// strings (not parsed values) so the log can render the diff verbatim even
/// when the value shape varies by provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingRectifierAction {
    pub field: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub reason: String,
}

/// Records upstream-error → client-error mapping. `upstream_code` is the raw
/// provider code; `mapped_code` is what the client received.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorMapperAction {
    pub upstream_code: Option<String>,
    pub upstream_message: Option<String>,
    pub mapped_code: String,
    pub mapped_message: String,
}

/// Records breaker state observed for the selected provider at request time
/// and any transitions caused by this request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerEvent {
    pub provider_id: String,
    /// "closed" | "open" | "half_open" — state *before* the request.
    pub observed_state: String,
    /// "closed_after_success" | "opened_after_failure" | "stayed_open" | etc.
    /// Empty / None when the request didn't change state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
}

/// Records each step of model-degradation-chain traversal. `chain` is the
/// full ordered list the selector walked; `picked` is the model that finally
/// succeeded (or None if the entire chain failed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradationEvent {
    pub requested_model: String,
    pub chain: Vec<String>,
    pub picked: Option<String>,
    /// Why the chain was traversed. e.g. "primary_404" / "primary_model_not_available".
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_refiner_log_serializes_to_empty_object() {
        let log = RefinerLog::default();
        let json = serde_json::to_string(&log).unwrap();
        assert_eq!(json, "{}");
        assert!(log.is_empty());
    }

    #[test]
    fn populated_log_round_trips() {
        let log = RefinerLog {
            body_filter: Some(BodyFilterAction {
                stripped_fields: vec!["web_search".into()],
                reason: "provider_quirks".into(),
            }),
            degradation: Some(DegradationEvent {
                requested_model: "gpt-5-codex".into(),
                chain: vec!["gpt-5-codex".into(), "gpt-5-mini".into()],
                picked: Some("gpt-5-mini".into()),
                reason: "primary_404".into(),
            }),
            ..Default::default()
        };
        assert!(!log.is_empty());
        let json = serde_json::to_string(&log).unwrap();
        let parsed: RefinerLog = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.body_filter.as_ref().unwrap().stripped_fields,
            vec!["web_search".to_string()]
        );
        assert_eq!(
            parsed.degradation.as_ref().unwrap().picked.as_deref(),
            Some("gpt-5-mini")
        );
        // is_empty is false because some variants populated.
        assert!(!parsed.is_empty());
    }

    #[test]
    fn optional_fields_omitted_in_json() {
        let log = RefinerLog {
            body_filter: Some(BodyFilterAction {
                stripped_fields: vec!["web_search".into()],
                reason: "test".into(),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("body_filter"));
        assert!(!json.contains("thinking_rectifier"));
        assert!(!json.contains("error_mapper"));
    }
}
