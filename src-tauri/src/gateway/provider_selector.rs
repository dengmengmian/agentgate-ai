use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use crate::errors::AppError;
use crate::models::provider::Provider;
use crate::models::route_profile::RouteProfileProviderView;
use crate::protocol::openai_responses::ResponsesRequest;
use crate::storage;

/// The result of selecting a provider for a request.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProviderSelection {
    pub route_profile_id: String,
    pub route_profile_name: String,
    pub mode: String,
    pub provider: Provider,
    pub model: String,
    pub priority: i64,
    pub reason: String,
    /// All candidates in order, for failover iteration
    pub candidates: Vec<ProviderCandidate>,
}

#[derive(Debug, Clone)]
pub struct ProviderCandidate {
    pub provider_id: String,
    pub provider_name: String,
    pub priority: i64,
    pub model: String,
    pub in_cooldown: bool,
    pub supports_vision: Option<bool>,
    pub cooldown_seconds: i64,
    pub failover_on_status_codes: Vec<i64>,
    pub failover_on_error_keywords: Vec<String>,
}

/// Request characteristics for condition matching.
#[derive(Debug, Clone)]
pub struct RequestAnalysis {
    pub input_char_count: usize,
    pub has_images: bool,
    pub has_tools: bool,
    #[allow(dead_code)]
    pub tool_count: usize,
    pub system_text: String,
    #[allow(dead_code)]
    pub message_count: usize,
}

/// Analyze a ResponsesRequest to extract routing-relevant characteristics.
pub fn analyze_request(req: &ResponsesRequest) -> RequestAnalysis {
    let input_str = req.input.to_string();
    let input_char_count = input_str.len();

    let has_images = crate::gateway::routes::request_contains_images_pub(req);

    let has_tools = req.tools.as_ref().map_or(false, |t| !t.is_empty());
    let tool_count = req.tools.as_ref().map_or(0, |t| t.len());

    let system_text = req.instructions.clone()
        .or_else(|| req.system.clone())
        .unwrap_or_default();

    let message_count = match &req.input {
        Value::Array(items) => items.iter().filter(|i| {
            i.get("type").and_then(|t| t.as_str()) == Some("message")
        }).count(),
        _ => 1,
    };

    RequestAnalysis {
        input_char_count,
        has_images,
        has_tools,
        tool_count,
        system_text,
        message_count,
    }
}

/// Routing conditions that can be attached to a provider in a route profile.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RoutingConditions {
    pub min_input_chars: Option<usize>,
    pub max_input_chars: Option<usize>,
    pub has_images: Option<bool>,
    pub has_tools: Option<bool>,
    pub system_keywords: Option<Vec<String>>,
    pub model_override: Option<String>,
}

/// Check if all non-null conditions match the request analysis.
fn matches_conditions(conditions: &RoutingConditions, analysis: &RequestAnalysis) -> bool {
    if let Some(min) = conditions.min_input_chars {
        if analysis.input_char_count < min { return false; }
    }
    if let Some(max) = conditions.max_input_chars {
        if analysis.input_char_count > max { return false; }
    }
    if let Some(img) = conditions.has_images {
        if analysis.has_images != img { return false; }
    }
    if let Some(tools) = conditions.has_tools {
        if analysis.has_tools != tools { return false; }
    }
    if let Some(ref keywords) = conditions.system_keywords {
        if !keywords.is_empty() {
            let lower = analysis.system_text.to_lowercase();
            if !keywords.iter().any(|kw| lower.contains(&kw.to_lowercase())) {
                return false;
            }
        }
    }
    true
}

fn build_candidates(
    conn: &Connection,
    rp_providers: &[RouteProfileProviderView],
    requested_model: Option<&str>,
    analysis: Option<&RequestAnalysis>,
) -> Result<Vec<ProviderCandidate>, AppError> {
    let mut candidates = Vec::new();

    for rpp in rp_providers {
        if !rpp.enabled {
            continue;
        }

        // Check routing conditions (if analysis available and conditions configured)
        let mut condition_model_override: Option<String> = None;
        if let (Some(ref cond_json), Some(ref req_analysis)) = (&rpp.routing_conditions, &analysis) {
            if let Ok(conditions) = serde_json::from_str::<RoutingConditions>(cond_json) {
                if !matches_conditions(&conditions, req_analysis) {
                    continue; // Skip this provider — conditions not met
                }
                condition_model_override = conditions.model_override.clone();
            }
        }

        // Model resolution: condition_model_override → model_override → model_mapping → supported_models → default_model
        let provider_info = storage::providers::get_by_id(conn, &rpp.provider_id).ok();

        let model = condition_model_override.or_else(|| rpp.model_override.clone()).unwrap_or_else(|| {
            if let Some(ref p) = provider_info {
                if let Some(req) = requested_model {
                    return p.resolve_model(req);
                }
                p.default_model.clone()
            } else {
                String::new()
            }
        });

        let in_cooldown = rpp.cooldown_until.as_ref().map_or(false, |until| {
            chrono::DateTime::parse_from_rfc3339(until)
                .map(|cd| cd > chrono::Utc::now())
                .unwrap_or(false)
        });

        let status_codes: Vec<i64> = rpp.failover_on_status_codes.as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| vec![402, 429, 500, 502, 503, 504]);

        let keywords: Vec<String> = rpp.failover_on_error_keywords.as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        let supports_vision = provider_info.as_ref().and_then(|p| p.supports_vision);

        candidates.push(ProviderCandidate {
            provider_id: rpp.provider_id.clone(),
            provider_name: rpp.provider_name.clone(),
            priority: rpp.priority,
            model,
            in_cooldown,
            supports_vision,
            cooldown_seconds: rpp.cooldown_seconds,
            failover_on_status_codes: status_codes,
            failover_on_error_keywords: keywords,
        });
    }

    Ok(candidates)
}

fn select_global_fallback(
    conn: &Connection,
    requested_model: Option<&str>,
) -> Result<ProviderSelection, AppError> {
    let settings = storage::gateway_settings::get(conn)?;
    let provider_id = settings.active_provider_id.ok_or_else(|| {
        AppError::new("ACTIVE_PROVIDER_NOT_FOUND", "No active provider configured")
            .with_suggestion("Set an active provider in the Providers page")
    })?;

    let provider = storage::providers::get_by_id(conn, &provider_id)?;
    let model = match requested_model {
        Some(req) => provider.resolve_model(req),
        None => provider.default_model.clone(),
    };

    Ok(ProviderSelection {
        route_profile_id: String::new(),
        route_profile_name: "Global Fallback".to_string(),
        mode: "manual".to_string(),
        provider,
        model,
        priority: 0,
        reason: "No route profile found, using global active provider".to_string(),
        candidates: vec![],
    })
}

/// Select provider for failover mode. Returns the ordered list of providers to try.
/// If `request` is provided, routing conditions on providers will be evaluated.
pub fn select_for_failover(
    db: &Arc<Mutex<Connection>>,
    input_protocol: &str,
    requested_model: Option<&str>,
    request: Option<&ResponsesRequest>,
) -> Result<ProviderSelection, AppError> {
    let conn = db.lock().map_err(|_| AppError::internal("DB lock failed"))?;

    let profile = storage::route_profiles::get_default_for_protocol(&conn, input_protocol)?;

    if let Some(profile) = profile {
        let rp_providers = storage::route_profiles::list_providers(&conn, &profile.id)?;
        if rp_providers.is_empty() {
            return Err(AppError::new("ROUTE_PROFILE_EMPTY", "Route profile has no providers"));
        }

        let analysis = request.map(analyze_request);
        let candidates = build_candidates(&conn, &rp_providers, requested_model, analysis.as_ref())?;
        if candidates.is_empty() {
            return Err(AppError::new("NO_PROVIDER_CANDIDATE", "No available provider candidate"));
        }

        // Manual mode: use active_provider_id; Failover mode: first non-cooldown
        let (selected, reason) = if profile.mode == "manual" {
            if let Some(ref active_id) = profile.active_provider_id {
                if let Some(c) = candidates.iter().find(|c| c.provider_id == *active_id) {
                    (c, "Manual: active provider")
                } else {
                    (&candidates[0], "Manual: active not in candidates, using first")
                }
            } else {
                (&candidates[0], "Manual: no active set, using first")
            }
        } else {
            let c = candidates.iter().find(|c| !c.in_cooldown).unwrap_or(&candidates[0]);
            (c, "Failover: first available")
        };

        let provider = storage::providers::get_by_id(&conn, &selected.provider_id)?;

        Ok(ProviderSelection {
            route_profile_id: profile.id,
            route_profile_name: profile.name,
            mode: profile.mode,
            provider,
            model: selected.model.clone(),
            priority: selected.priority,
            reason: reason.to_string(),
            candidates,
        })
    } else {
        select_global_fallback(&conn, requested_model)
    }
}

/// Check if we should failover based on error status/message and the candidate's config.
pub fn should_failover(status_code: Option<u16>, error_msg: &str, candidate: &ProviderCandidate) -> bool {
    // Check status code
    if let Some(code) = status_code {
        if candidate.failover_on_status_codes.contains(&(code as i64)) {
            return true;
        }
    }

    // Check error keywords
    let lower = error_msg.to_lowercase();
    for kw in &candidate.failover_on_error_keywords {
        if lower.contains(&kw.to_lowercase()) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate_with_defaults() -> ProviderCandidate {
        ProviderCandidate {
            provider_id: "p1".to_string(),
            provider_name: "Test".to_string(),
            priority: 0,
            model: "gpt-4".to_string(),
            in_cooldown: false,
            supports_vision: None,
            cooldown_seconds: 60,
            failover_on_status_codes: vec![402, 429, 500, 502, 503, 504],
            failover_on_error_keywords: vec!["rate limit".to_string(), "timeout".to_string()],
        }
    }

    #[test]
    fn test_should_failover_on_status_code() {
        let c = candidate_with_defaults();
        assert!(should_failover(Some(429), "ok", &c));
        assert!(should_failover(Some(500), "ok", &c));
        assert!(should_failover(Some(503), "ok", &c));
    }

    #[test]
    fn test_should_not_failover_on_success_code() {
        let c = candidate_with_defaults();
        assert!(!should_failover(Some(200), "ok", &c));
        assert!(!should_failover(Some(400), "bad request", &c));
        assert!(!should_failover(Some(404), "not found", &c));
    }

    #[test]
    fn test_should_failover_on_keyword() {
        let c = candidate_with_defaults();
        assert!(should_failover(None, "Rate limit exceeded", &c));
        assert!(should_failover(None, "Connection timeout", &c));
        assert!(should_failover(None, "request timeout error", &c));
    }

    #[test]
    fn test_should_failover_on_status_and_keyword() {
        let c = candidate_with_defaults();
        assert!(should_failover(Some(429), "Rate limit exceeded", &c));
    }

    #[test]
    fn test_should_not_failover_no_match() {
        let c = candidate_with_defaults();
        assert!(!should_failover(None, "everything is fine", &c));
        assert!(!should_failover(Some(200), "success", &c));
    }

    #[test]
    fn test_should_failover_custom_status_codes() {
        let mut c = candidate_with_defaults();
        c.failover_on_status_codes = vec![418];
        assert!(should_failover(Some(418), "I'm a teapot", &c));
        assert!(!should_failover(Some(500), "error", &c));
    }

    #[test]
    fn test_should_failover_custom_keywords() {
        let mut c = candidate_with_defaults();
        c.failover_on_error_keywords = vec!["insufficient_quota".to_string()];
        assert!(should_failover(None, "insufficient_quota", &c));
        assert!(!should_failover(None, "rate limit", &c));
    }

    #[test]
    fn test_should_failover_empty_lists() {
        let mut c = candidate_with_defaults();
        c.failover_on_status_codes = vec![];
        c.failover_on_error_keywords = vec![];
        assert!(!should_failover(Some(500), "error", &c));
        assert!(!should_failover(None, "error", &c));
    }

    // ── Routing conditions tests ──

    fn test_analysis(chars: usize, images: bool, tools: bool, system: &str) -> RequestAnalysis {
        RequestAnalysis {
            input_char_count: chars, has_images: images, has_tools: tools,
            tool_count: 0, system_text: system.to_string(), message_count: 1,
        }
    }

    #[test]
    fn test_matches_conditions_empty() {
        let cond = RoutingConditions::default();
        let analysis = test_analysis(100, false, false, "");
        assert!(matches_conditions(&cond, &analysis));
    }

    #[test]
    fn test_matches_conditions_min_chars() {
        let cond = RoutingConditions { min_input_chars: Some(1000), ..Default::default() };
        assert!(!matches_conditions(&cond, &test_analysis(500, false, false, "")));
        assert!(matches_conditions(&cond, &test_analysis(1000, false, false, "")));
        assert!(matches_conditions(&cond, &test_analysis(5000, false, false, "")));
    }

    #[test]
    fn test_matches_conditions_max_chars() {
        let cond = RoutingConditions { max_input_chars: Some(1000), ..Default::default() };
        assert!(matches_conditions(&cond, &test_analysis(500, false, false, "")));
        assert!(matches_conditions(&cond, &test_analysis(1000, false, false, "")));
        assert!(!matches_conditions(&cond, &test_analysis(5000, false, false, "")));
    }

    #[test]
    fn test_matches_conditions_has_images() {
        let cond = RoutingConditions { has_images: Some(true), ..Default::default() };
        assert!(!matches_conditions(&cond, &test_analysis(100, false, false, "")));
        assert!(matches_conditions(&cond, &test_analysis(100, true, false, "")));
    }

    #[test]
    fn test_matches_conditions_has_tools() {
        let cond = RoutingConditions { has_tools: Some(true), ..Default::default() };
        assert!(!matches_conditions(&cond, &test_analysis(100, false, false, "")));
        assert!(matches_conditions(&cond, &test_analysis(100, false, true, "")));
    }

    #[test]
    fn test_matches_conditions_system_keywords() {
        let cond = RoutingConditions {
            system_keywords: Some(vec!["background".to_string(), "subagent".to_string()]),
            ..Default::default()
        };
        assert!(!matches_conditions(&cond, &test_analysis(100, false, false, "You are a helpful assistant")));
        assert!(matches_conditions(&cond, &test_analysis(100, false, false, "Run this in background mode")));
        assert!(matches_conditions(&cond, &test_analysis(100, false, false, "This is a SUBAGENT task")));
    }

    #[test]
    fn test_matches_conditions_combined() {
        let cond = RoutingConditions {
            min_input_chars: Some(1000),
            has_images: Some(true),
            ..Default::default()
        };
        assert!(!matches_conditions(&cond, &test_analysis(500, true, false, ""))); // chars too low
        assert!(!matches_conditions(&cond, &test_analysis(2000, false, false, ""))); // no images
        assert!(matches_conditions(&cond, &test_analysis(2000, true, false, ""))); // both match
    }

    #[test]
    fn test_matches_conditions_parse_json() {
        let json = r#"{"has_images": true, "system_keywords": ["background"]}"#;
        let cond: RoutingConditions = serde_json::from_str(json).unwrap();
        assert!(matches_conditions(&cond, &test_analysis(100, true, false, "background task")));
        assert!(!matches_conditions(&cond, &test_analysis(100, false, false, "background task")));
    }

    #[test]
    fn test_should_failover_keyword_case_insensitive() {
        let c = candidate_with_defaults();
        assert!(should_failover(None, "RATE LIMIT", &c));
        assert!(should_failover(None, "Timeout", &c));
        assert!(should_failover(None, "TIMEOUT", &c));
    }
}
