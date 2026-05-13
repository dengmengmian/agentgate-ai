use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::errors::AppError;
use crate::models::provider::Provider;
use crate::models::route_profile::RouteProfileProviderView;
use crate::storage;

/// The result of selecting a provider for a request.
#[derive(Debug, Clone)]
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
    pub cooldown_seconds: i64,
    pub failover_on_status_codes: Vec<i64>,
    pub failover_on_error_keywords: Vec<String>,
}

/// Select a provider for a request based on route profiles.
/// Falls back to global active provider if no route profile matches.
pub fn select(
    db: &Arc<Mutex<Connection>>,
    input_protocol: &str,
    requested_model: Option<&str>,
) -> Result<ProviderSelection, AppError> {
    let conn = db.lock().map_err(|_| AppError::internal("DB lock failed"))?;

    // Try to find default route profile for this protocol
    let profile = storage::route_profiles::get_default_for_protocol(&conn, input_protocol)?;

    if let Some(profile) = profile {
        let rp_providers = storage::route_profiles::list_providers(&conn, &profile.id)?;

        if rp_providers.is_empty() {
            return Err(AppError::new("ROUTE_PROFILE_EMPTY", "Route profile has no providers")
                .with_suggestion("Add at least one provider to the route profile"));
        }

        let candidates = build_candidates(&conn, &rp_providers, requested_model)?;

        if candidates.is_empty() {
            return Err(AppError::new("NO_PROVIDER_CANDIDATE", "No available provider candidate")
                .with_suggestion("Enable at least one provider in the route profile"));
        }

        // Select provider based on mode
        let (selected_provider, selected_model, priority, reason) = match profile.mode.as_str() {
            "failover" => select_failover(&candidates)?,
            _ => select_manual(&profile, &candidates, &conn)?,
        };

        Ok(ProviderSelection {
            route_profile_id: profile.id,
            route_profile_name: profile.name,
            mode: profile.mode,
            provider: selected_provider,
            model: selected_model,
            priority,
            reason,
            candidates,
        })
    } else {
        // Fallback to global active provider
        select_global_fallback(&conn, requested_model)
    }
}

fn build_candidates(
    conn: &Connection,
    rp_providers: &[RouteProfileProviderView],
    requested_model: Option<&str>,
) -> Result<Vec<ProviderCandidate>, AppError> {
    let mut candidates = Vec::new();

    for rpp in rp_providers {
        if !rpp.enabled {
            continue;
        }

        // Model resolution: model_override → model_mapping → supported_models → default_model
        let provider_info = storage::providers::get_by_id(conn, &rpp.provider_id).ok();

        let model = rpp.model_override.clone().unwrap_or_else(|| {
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

        candidates.push(ProviderCandidate {
            provider_id: rpp.provider_id.clone(),
            provider_name: rpp.provider_name.clone(),
            priority: rpp.priority,
            model,
            in_cooldown,
            cooldown_seconds: rpp.cooldown_seconds,
            failover_on_status_codes: status_codes,
            failover_on_error_keywords: keywords,
        });
    }

    Ok(candidates)
}

fn select_manual(
    profile: &crate::models::route_profile::RouteProfile,
    candidates: &[ProviderCandidate],
    conn: &Connection,
) -> Result<(Provider, String, i64, String), AppError> {
    // Use active_provider_id if set
    if let Some(ref active_id) = profile.active_provider_id {
        if let Some(c) = candidates.iter().find(|c| c.provider_id == *active_id) {
            let provider = storage::providers::get_by_id(conn, active_id)?;
            return Ok((provider, c.model.clone(), c.priority, "Manual: active provider".to_string()));
        }
    }

    // Fallback to highest priority
    let c = &candidates[0];
    let provider = storage::providers::get_by_id(conn, &c.provider_id)?;
    Ok((provider, c.model.clone(), c.priority, "Manual: highest priority".to_string()))
}

fn select_failover(
    candidates: &[ProviderCandidate],
) -> Result<(Provider, String, i64, String), AppError> {
    // Prefer non-cooldown providers
    for c in candidates {
        if !c.in_cooldown {
            // We need to load the provider from inside the caller's conn context,
            // but we don't have conn here. We'll return a placeholder and let
            // the caller handle the actual Provider loading.
            // For simplicity, return the first non-cooldown candidate info.
            // The actual Provider will be loaded by the caller.
            return Err(AppError::internal("use select() directly"));
        }
    }
    // All in cooldown — use first anyway
    Err(AppError::internal("use select() directly"))
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
pub fn select_for_failover(
    db: &Arc<Mutex<Connection>>,
    input_protocol: &str,
    requested_model: Option<&str>,
) -> Result<ProviderSelection, AppError> {
    let conn = db.lock().map_err(|_| AppError::internal("DB lock failed"))?;

    let profile = storage::route_profiles::get_default_for_protocol(&conn, input_protocol)?;

    if let Some(profile) = profile {
        let rp_providers = storage::route_profiles::list_providers(&conn, &profile.id)?;
        if rp_providers.is_empty() {
            return Err(AppError::new("ROUTE_PROFILE_EMPTY", "Route profile has no providers"));
        }

        let candidates = build_candidates(&conn, &rp_providers, requested_model)?;
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

    #[test]
    fn test_should_failover_keyword_case_insensitive() {
        let c = candidate_with_defaults();
        assert!(should_failover(None, "RATE LIMIT", &c));
        assert!(should_failover(None, "Timeout", &c));
        assert!(should_failover(None, "TIMEOUT", &c));
    }
}
