use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

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
    pub routing_conditions: Option<String>,
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

    let system_text = req
        .instructions
        .clone()
        .or_else(|| req.system.clone())
        .unwrap_or_default();

    let message_count = match &req.input {
        Value::Array(items) => items
            .iter()
            .filter(|i| i.get("type").and_then(|t| t.as_str()) == Some("message"))
            .count(),
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
        if analysis.input_char_count < min {
            return false;
        }
    }
    if let Some(max) = conditions.max_input_chars {
        if analysis.input_char_count > max {
            return false;
        }
    }
    if let Some(img) = conditions.has_images {
        if analysis.has_images != img {
            return false;
        }
    }
    if let Some(tools) = conditions.has_tools {
        if analysis.has_tools != tools {
            return false;
        }
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
        if let (Some(ref cond_json), Some(ref req_analysis)) = (&rpp.routing_conditions, &analysis)
        {
            match serde_json::from_str::<RoutingConditions>(cond_json) {
                Ok(conditions) => {
                    if !matches_conditions(&conditions, req_analysis) {
                        continue;
                    }
                    condition_model_override = conditions.model_override.clone();
                }
                Err(_) => continue,
            }
        }

        // Model resolution: condition_model_override → model_override → model_mapping → supported_models → default_model
        let provider_info = storage::providers::get_by_id(conn, &rpp.provider_id).ok();

        let model = condition_model_override
            .or_else(|| rpp.model_override.clone())
            .unwrap_or_else(|| {
                if let Some(ref p) = provider_info {
                    if let Some(req) = requested_model {
                        return p.resolve_model(req);
                    }
                    p.default_model.clone()
                } else {
                    String::new()
                }
            });

        // Capability-aware model promotion: if request demands a capability
        // (e.g. vision) the resolved model lacks but another model on the same
        // provider has, swap to that model. Only fires when model_capabilities
        // matrix is populated; otherwise we leave the resolved model alone.
        let model = if let (Some(p), Some(req_analysis)) = (provider_info.as_ref(), analysis) {
            promote_for_capabilities(p, &model, req_analysis)
        } else {
            model
        };

        let in_cooldown = rpp.cooldown_until.as_ref().map_or(false, |until| {
            chrono::DateTime::parse_from_rfc3339(until)
                .map(|cd| cd > chrono::Utc::now())
                .unwrap_or(false)
        });

        let status_codes: Vec<i64> = rpp
            .failover_on_status_codes
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_else(|| vec![402, 429, 500, 502, 503, 504]);

        let keywords: Vec<String> = rpp
            .failover_on_error_keywords
            .as_ref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();

        // supports_vision precedence:
        //   1. model_capabilities matrix (any model declares "vision") — MOST ACCURATE,
        //      reflects per-model reality. Wins over the legacy single boolean.
        //   2. explicit per-provider flag (legacy probe result) — only when matrix unset.
        //   3. None — unknown, no opinion.
        //
        // Earlier the order was flipped, which caused MiMo providers where the legacy
        // probe wrote `supports_vision=false` (because mimo-v2.5-pro 404'd on image) to
        // be skipped from image requests entirely — even though the matrix declared
        // mimo-v2.5 / mimo-v2-omni as vision-capable. The promotion step (below) would
        // never run because the provider was filtered out in routes.rs:114 first.
        let supports_vision = provider_info
            .as_ref()
            .and_then(|p| {
                let caps = p.parse_capabilities();
                if caps.is_empty() {
                    None
                } else {
                    Some(caps.values().any(|c| {
                        c.iter()
                            .any(|x| x == crate::providers::capabilities::CAP_VISION)
                    }))
                }
            })
            .or_else(|| provider_info.as_ref().and_then(|p| p.supports_vision));

        candidates.push(ProviderCandidate {
            provider_id: rpp.provider_id.clone(),
            provider_name: rpp.provider_name.clone(),
            priority: rpp.priority,
            model,
            routing_conditions: rpp.routing_conditions.clone(),
            in_cooldown,
            supports_vision,
            cooldown_seconds: rpp.cooldown_seconds,
            failover_on_status_codes: status_codes,
            failover_on_error_keywords: keywords,
        });
    }

    Ok(candidates)
}

/// 按 route profile 的 selection_strategy 对 failover 候选稳定排序。
/// "cheapest"：模型单价(input+output)升序；"fastest"：近 24h 平均延迟升序；
/// 其它（含 "priority"）：保持手工顺序不动。查不到价格/延迟的候选排末尾，
/// 平手按原 priority。
fn sort_candidates_by_strategy(
    conn: &Connection,
    candidates: &mut [ProviderCandidate],
    strategy: &str,
) {
    match strategy {
        "cheapest" => candidates.sort_by(|a, b| {
            candidate_unit_cost(conn, a)
                .partial_cmp(&candidate_unit_cost(conn, b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.priority.cmp(&b.priority))
        }),
        "fastest" => {
            let lat = storage::request_logs::avg_latency_by_provider(conn, 24).unwrap_or_default();
            candidates.sort_by(|a, b| {
                let la = lat.get(&a.provider_name).copied().unwrap_or(f64::MAX);
                let lb = lat.get(&b.provider_name).copied().unwrap_or(f64::MAX);
                la.partial_cmp(&lb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.priority.cmp(&b.priority))
            });
        }
        _ => {}
    }
}

/// 候选模型单价排序键：input+output 单价之和($/1M)。查不到价时返回 MAX 排末尾。
/// provider 用实例名，与成本计算/日志写入一致。
fn candidate_unit_cost(conn: &Connection, c: &ProviderCandidate) -> f64 {
    storage::pricing::get_price(conn, &c.provider_name, &c.model)
        .map(|(input, output)| input + output)
        .unwrap_or(f64::MAX)
}

fn select_global_fallback(
    conn: &Connection,
    requested_model: Option<&str>,
    analysis: Option<&RequestAnalysis>,
) -> Result<ProviderSelection, AppError> {
    let settings = storage::gateway_settings::get(conn)?;
    let provider_id = settings.active_provider_id.ok_or_else(|| {
        AppError::new(
            crate::errors::codes::ACTIVE_PROVIDER_NOT_FOUND,
            "No active provider configured",
        )
        .with_suggestion("Set an active provider in the Providers page")
    })?;

    let provider = storage::providers::get_by_id(conn, &provider_id)?;
    let model = match requested_model {
        Some(req) => provider.resolve_model(req),
        None => provider.default_model.clone(),
    };
    // Capability-aware promotion in fallback path too — mirrors the route_profile path.
    let model = if let Some(req_analysis) = analysis {
        promote_for_capabilities(&provider, &model, req_analysis)
    } else {
        model
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
    db: &crate::storage::db::DbPool,
    input_protocol: &str,
    requested_model: Option<&str>,
    request: Option<&ResponsesRequest>,
) -> Result<ProviderSelection, AppError> {
    let conn = db.get().map_err(|_| AppError::internal("DB lock failed"))?;

    let profile = storage::route_profiles::get_default_for_protocol(&conn, input_protocol)?;

    if let Some(profile) = profile {
        let rp_providers = storage::route_profiles::list_providers(&conn, &profile.id)?;
        if rp_providers.is_empty() {
            return Err(AppError::new(
                crate::errors::codes::ROUTE_PROFILE_EMPTY,
                "Route profile has no providers",
            ));
        }

        let analysis = request.map(analyze_request);
        let mut candidates =
            build_candidates(&conn, &rp_providers, requested_model, analysis.as_ref())?;
        if candidates.is_empty() {
            return Err(AppError::new(
                crate::errors::codes::NO_PROVIDER_CANDIDATE,
                "No available provider candidate",
            ));
        }

        // Failover 模式按 selection_strategy 重排候选（cheapest/fastest）；
        // manual 模式按 active_id 选、priority 维持原序，都不需要重排。
        if profile.mode != "manual" && profile.selection_strategy != "priority" {
            sort_candidates_by_strategy(&conn, &mut candidates, &profile.selection_strategy);
        }

        // Manual mode: use active_provider_id; Failover mode: first non-cooldown
        let (selected, reason) = if profile.mode == "manual" {
            if let Some(ref active_id) = profile.active_provider_id {
                if let Some(c) = candidates.iter().find(|c| c.provider_id == *active_id) {
                    (c, "Manual: active provider")
                } else {
                    (
                        &candidates[0],
                        "Manual: active not in candidates, using first",
                    )
                }
            } else {
                (&candidates[0], "Manual: no active set, using first")
            }
        } else {
            let c = candidates
                .iter()
                .find(|c| !c.in_cooldown)
                .unwrap_or(&candidates[0]);
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
        let analysis = request.map(analyze_request);
        select_global_fallback(&conn, requested_model, analysis.as_ref())
    }
}

/// Promote the resolved model to a sibling that has the capability the
/// request needs, if the current pick lacks it. Currently checks vision
/// (image input). Returns the original model when:
///   - the provider has no model_capabilities matrix configured,
///   - the current model already satisfies the request, or
///   - no sibling model satisfies the demanded capability.
fn promote_for_capabilities(
    provider: &Provider,
    current_model: &str,
    analysis: &RequestAnalysis,
) -> String {
    let caps_map = provider.parse_capabilities();
    if caps_map.is_empty() {
        return current_model.to_string();
    }

    // Strip any qualifier ([1m] etc.) before looking up in the matrix.
    let base = strip_qualifier(current_model);
    let current_caps: Vec<String> = caps_map.get(base).cloned().unwrap_or_default();
    let has = |c: &str| current_caps.iter().any(|x| x == c);

    if analysis.has_images && !has(crate::providers::capabilities::CAP_VISION) {
        if let Some(picked) = pick_best_substitute(
            provider,
            crate::providers::capabilities::CAP_VISION,
            &current_caps,
            &caps_map,
        ) {
            return picked;
        }
    }
    // Future: extend with audio_in / tts / etc. as more clients send them.

    current_model.to_string()
}

/// Pick the best vision-capable (or whatever-capable) substitute, preferring
/// the model that preserves the most of the original model's other capabilities.
/// Ties broken by `supported_models` order (first wins).
///
/// Example: original = mimo-v2.5-pro [reasoning, tools, web_search]; needed = vision.
/// Candidates: [mimo-v2-omni (text+vision+tools), mimo-v2.5 (text+vision+reasoning+tools+web_search)].
/// Original keeps 3 caps in v2.5 (tools+reasoning+web_search) vs 1 in omni (tools).
/// → pick v2.5 even though omni came first in the supported_models list.
fn pick_best_substitute(
    provider: &Provider,
    required: &str,
    original_caps: &[String],
    caps_map: &std::collections::HashMap<String, Vec<String>>,
) -> Option<String> {
    let candidates = provider.models_with_capability(required);
    if candidates.is_empty() {
        return None;
    }
    let original: std::collections::HashSet<&str> =
        original_caps.iter().map(|s| s.as_str()).collect();

    // Iterate in supported_models order so ties favor the user's listed priority.
    let mut best: Option<(String, usize)> = None;
    for model in candidates {
        let model_caps: std::collections::HashSet<&str> = caps_map
            .get(model.as_str())
            .map(|caps| caps.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();
        let overlap = original.iter().filter(|c| model_caps.contains(*c)).count();
        // Strict > so the first model with this score wins (stable tie-break).
        if best.as_ref().map_or(true, |(_, score)| overlap > *score) {
            best = Some((model, overlap));
        }
    }
    best.map(|(m, _)| m)
}

fn strip_qualifier(model: &str) -> &str {
    if let Some(stripped) = model.strip_suffix(']') {
        if let Some(open) = stripped.rfind('[') {
            return &stripped[..open];
        }
    }
    model
}

/// Walk the per-provider `model_degradation_chain` for the given requested
/// model. Returns the *full ordered chain including the requested model
/// itself* (head = primary), so the failover loop can iterate without
/// having to track "did I try the original yet?" separately. Returns
/// just `[requested]` when the provider has no degradation chain configured
/// or the requested model has no fallbacks listed.
///
/// Example:
///   provider.model_degradation_chain = {"gpt-5-codex": ["gpt-5-mini","gpt-4o"]}
///   degradation_chain_for_model(provider, "gpt-5-codex")
///       → ["gpt-5-codex", "gpt-5-mini", "gpt-4o"]
///
/// Models not present in the chain (e.g. user requested "claude-sonnet-4"
/// against a provider with no entry for it) return a single-element vec
/// — the failover loop falls back to provider-level failover at that point.
pub fn degradation_chain_for_model(provider: &Provider, requested_model: &str) -> Vec<String> {
    let chain = provider.parse_degradation_chain();
    let mut result = vec![requested_model.to_string()];
    if let Some(fallbacks) = chain.get(requested_model) {
        for m in fallbacks {
            if m != requested_model && !result.contains(m) {
                result.push(m.clone());
            }
        }
    }
    result
}

pub fn route_decision_trace(selection: &ProviderSelection) -> Value {
    let selected_conditions = selection
        .candidates
        .iter()
        .find(|c| c.provider_id == selection.provider.id)
        .and_then(|c| c.routing_conditions.as_ref())
        .and_then(|s| serde_json::from_str::<Value>(s).ok());

    serde_json::json!({
        "route_decision": {
            "profile_id": selection.route_profile_id,
            "profile_name": selection.route_profile_name,
            "mode": selection.mode,
            "reason": selection.reason,
            "selected_provider_id": selection.provider.id,
            "selected_provider_name": selection.provider.name,
            "selected_model": selection.model,
            "selected_priority": selection.priority,
            "matched_conditions": selected_conditions,
            "candidates": selection.candidates.iter().map(|c| {
                serde_json::json!({
                    "provider_id": c.provider_id,
                    "provider_name": c.provider_name,
                    "priority": c.priority,
                    "model": c.model,
                    "in_cooldown": c.in_cooldown,
                    "supports_vision": c.supports_vision,
                    "has_conditions": c.routing_conditions.is_some(),
                })
            }).collect::<Vec<_>>(),
        }
    })
}

/// Check if we should failover based on error status/message and the candidate's config.
pub fn should_failover(
    status_code: Option<u16>,
    error_msg: &str,
    candidate: &ProviderCandidate,
) -> bool {
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
            routing_conditions: None,
            in_cooldown: false,
            supports_vision: None,
            cooldown_seconds: 60,
            failover_on_status_codes: vec![402, 429, 500, 502, 503, 504],
            failover_on_error_keywords: vec!["rate limit".to_string(), "timeout".to_string()],
        }
    }

    #[test]
    fn route_decision_trace_includes_selected_provider_and_candidates() {
        let mut selection = ProviderSelection {
            route_profile_id: "rp1".to_string(),
            route_profile_name: "Codex Default".to_string(),
            mode: "failover".to_string(),
            provider: mimo_provider_with_matrix("mimo-v2.5-pro"),
            model: "mimo-v2.5-pro".to_string(),
            priority: 1,
            reason: "Failover mode selected first available provider".to_string(),
            candidates: vec![candidate_with_defaults()],
        };
        selection.provider.id = "p1".to_string();
        selection.provider.name = "MiMo".to_string();

        let trace = route_decision_trace(&selection);

        assert_eq!(trace["route_decision"]["profile_id"], "rp1");
        assert_eq!(trace["route_decision"]["profile_name"], "Codex Default");
        assert_eq!(trace["route_decision"]["selected_provider_id"], "p1");
        assert_eq!(trace["route_decision"]["selected_provider_name"], "MiMo");
        assert_eq!(trace["route_decision"]["selected_model"], "mimo-v2.5-pro");
        assert_eq!(
            trace["route_decision"]["candidates"][0]["provider_name"],
            "Test"
        );
    }

    #[test]
    fn sort_cheapest_orders_by_unit_price() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE model_pricing (id TEXT PRIMARY KEY, provider TEXT, model_pattern TEXT,
                input_price REAL, output_price REAL, is_custom INTEGER, updated_at TEXT);
             INSERT INTO model_pricing VALUES ('1','cheap','m', 1.0, 1.0, 0, '');
             INSERT INTO model_pricing VALUES ('2','pricey','m', 50.0, 50.0, 0, '');",
        )
        .unwrap();

        let mk = |name: &str, priority: i64| {
            let mut c = candidate_with_defaults();
            c.provider_id = name.to_string();
            c.provider_name = name.to_string();
            c.model = "m".to_string();
            c.priority = priority;
            c
        };

        // 手工顺序贵的在前；cheapest 把便宜的排到前面。
        let mut cands = vec![mk("pricey", 1), mk("cheap", 2)];
        sort_candidates_by_strategy(&conn, &mut cands, "cheapest");
        assert_eq!(cands[0].provider_name, "cheap");
        assert_eq!(cands[1].provider_name, "pricey");

        // priority 策略不动，保持手工顺序。
        let mut cands2 = vec![mk("pricey", 1), mk("cheap", 2)];
        sort_candidates_by_strategy(&conn, &mut cands2, "priority");
        assert_eq!(cands2[0].provider_name, "pricey");

        // 查不到价的候选排到末尾。
        let mut cands3 = vec![mk("cheap", 1), mk("unknown", 2)];
        sort_candidates_by_strategy(&conn, &mut cands3, "cheapest");
        assert_eq!(cands3[0].provider_name, "cheap");
        assert_eq!(cands3[1].provider_name, "unknown");
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
            input_char_count: chars,
            has_images: images,
            has_tools: tools,
            tool_count: 0,
            system_text: system.to_string(),
            message_count: 1,
        }
    }

    // ── Capability promotion tests ──

    fn mimo_provider_with_matrix(default: &str) -> Provider {
        Provider {
            id: "p".into(),
            name: "MiMo".into(),
            provider_type: "mimo".into(),
            base_url: "https://api.xiaomimimo.com/v1".into(),
            api_key: Some("sk-x".into()),
            default_model: default.into(),
            reasoning_model: None,
            supported_models: Some(
                r#"["mimo-v2.5-pro","mimo-v2.5","mimo-v2-omni","mimo-v2-flash"]"#.into(),
            ),
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            responses_base_url: None,
            protocol: "openai_chat_completions".into(),
            timeout_seconds: 120,
            status: "ok".into(),
            supports_vision: None,
            auto_cache_control: None,
            supports_cache: None,
            model_capabilities: Some(
                r#"{
                "mimo-v2.5-pro":["text","reasoning","tools","web_search"],
                "mimo-v2.5":["text","vision","reasoning","tools","web_search"],
                "mimo-v2-omni":["text","vision","audio_in","video_in","tools"],
                "mimo-v2-flash":["text","reasoning","tools","web_search"]
            }"#
                .into(),
            ),
            provider_quirks: None,
            body_filter_enabled: None,
            thinking_rectifier_enabled: None,
            error_mapper_enabled: None,
            model_degradation_chain: None,
            model_context_windows: None,
            enabled: true,
            is_active: true,
            created_at: "2024-01-01".into(),
            updated_at: "2024-01-01".into(),
        }
    }

    #[test]
    fn promote_swaps_to_vision_model_when_request_has_image() {
        // supported_models order: pro, v2.5, v2-omni, flash. Both v2.5 and v2-omni
        // have vision. v2.5 preserves more of the original (reasoning + web_search)
        // than omni (which has neither), so v2.5 should win even though omni is
        // not listed first.
        let p = mimo_provider_with_matrix("mimo-v2.5-pro");
        let analysis = test_analysis(100, /* images */ true, false, "");
        let promoted = promote_for_capabilities(&p, "mimo-v2.5-pro", &analysis);
        assert_eq!(
            promoted, "mimo-v2.5",
            "should pick v2.5 — preserves reasoning + web_search of original"
        );
    }

    #[test]
    fn promote_prefers_capability_overlap_over_list_order() {
        // Sanity check: even if v2-omni is listed FIRST in supported_models,
        // the ranking should still pick v2.5 because of higher overlap.
        let mut p = mimo_provider_with_matrix("mimo-v2.5-pro");
        p.supported_models =
            Some(r#"["mimo-v2-omni","mimo-v2.5","mimo-v2.5-pro","mimo-v2-flash"]"#.into());
        let analysis = test_analysis(100, true, false, "");
        let promoted = promote_for_capabilities(&p, "mimo-v2.5-pro", &analysis);
        assert_eq!(
            promoted, "mimo-v2.5",
            "list order doesn't override overlap score"
        );
    }

    #[test]
    fn promote_falls_back_to_list_order_on_tied_overlap() {
        // If two vision models have identical overlap, supported_models order breaks the tie.
        // Build a matrix where two models have identical caps.
        let mut p = mimo_provider_with_matrix("mimo-v2.5-pro");
        p.supported_models = Some(r#"["mimo-v2-omni","mimo-v2.5","mimo-v2.5-pro"]"#.into());
        p.model_capabilities = Some(
            r#"{
            "mimo-v2.5-pro":["text","reasoning"],
            "mimo-v2.5":["text","vision","reasoning"],
            "mimo-v2-omni":["text","vision","reasoning"]
        }"#
            .into(),
        );
        let analysis = test_analysis(100, true, false, "");
        let promoted = promote_for_capabilities(&p, "mimo-v2.5-pro", &analysis);
        assert_eq!(promoted, "mimo-v2-omni", "ties → first in supported_models");
    }

    #[test]
    fn promote_keeps_model_when_already_vision_capable() {
        let p = mimo_provider_with_matrix("mimo-v2.5");
        let analysis = test_analysis(100, true, false, "");
        let promoted = promote_for_capabilities(&p, "mimo-v2.5", &analysis);
        assert_eq!(promoted, "mimo-v2.5");
    }

    #[test]
    fn promote_keeps_model_when_no_image_in_request() {
        let p = mimo_provider_with_matrix("mimo-v2.5-pro");
        let analysis = test_analysis(100, /* images */ false, false, "");
        let promoted = promote_for_capabilities(&p, "mimo-v2.5-pro", &analysis);
        assert_eq!(promoted, "mimo-v2.5-pro");
    }

    #[test]
    fn promote_noop_when_matrix_missing() {
        let mut p = mimo_provider_with_matrix("mimo-v2.5-pro");
        p.model_capabilities = None;
        let analysis = test_analysis(100, true, false, "");
        let promoted = promote_for_capabilities(&p, "mimo-v2.5-pro", &analysis);
        assert_eq!(promoted, "mimo-v2.5-pro", "no matrix → no promotion");
    }

    #[test]
    fn promote_handles_1m_qualifier() {
        let p = mimo_provider_with_matrix("mimo-v2.5-pro[1m]");
        let analysis = test_analysis(100, true, false, "");
        let promoted = promote_for_capabilities(&p, "mimo-v2.5-pro[1m]", &analysis);
        assert_eq!(
            promoted, "mimo-v2.5",
            "[1m] qualifier should be stripped before lookup"
        );
    }

    // Verify the supports_vision derivation precedence: matrix must WIN
    // over the legacy single-boolean flag, otherwise a stale `false` from
    // the per-provider probe (run against a non-vision default model)
    // would mask a perfectly capable sibling model in the matrix.
    #[test]
    fn supports_vision_derivation_matrix_overrides_legacy_false() {
        let p = mimo_provider_with_matrix("mimo-v2.5-pro");
        // simulate the old buggy probe that wrote false
        let mut p = p;
        p.supports_vision = Some(false);

        let caps = p.parse_capabilities();
        let from_matrix = caps.values().any(|c| c.iter().any(|x| x == "vision"));
        // The matrix says: yes, *some* model has vision.
        assert!(
            from_matrix,
            "matrix should report vision-capable model present"
        );
        // After the fix, the derived flag for the candidate must trust the matrix.
        // (This mirrors the production code's chain of .and_then().or_else())
        let derived = if !caps.is_empty() {
            Some(from_matrix)
        } else {
            p.supports_vision
        };
        assert_eq!(derived, Some(true));
    }

    #[test]
    fn promote_no_swap_when_no_vision_model_exists() {
        let mut p = mimo_provider_with_matrix("deepseek-v4-pro");
        p.provider_type = "deepseek".into();
        p.supported_models = Some(r#"["deepseek-v4-pro","deepseek-v4-flash"]"#.into());
        p.model_capabilities = Some(
            r#"{
            "deepseek-v4-pro":["text","reasoning","tools","web_search"],
            "deepseek-v4-flash":["text","tools"]
        }"#
            .into(),
        );
        let analysis = test_analysis(100, true, false, "");
        let promoted = promote_for_capabilities(&p, "deepseek-v4-pro", &analysis);
        assert_eq!(
            promoted, "deepseek-v4-pro",
            "no vision model → leave alone, let upstream surface error"
        );
    }

    #[test]
    fn test_matches_conditions_empty() {
        let cond = RoutingConditions::default();
        let analysis = test_analysis(100, false, false, "");
        assert!(matches_conditions(&cond, &analysis));
    }

    #[test]
    fn test_matches_conditions_min_chars() {
        let cond = RoutingConditions {
            min_input_chars: Some(1000),
            ..Default::default()
        };
        assert!(!matches_conditions(
            &cond,
            &test_analysis(500, false, false, "")
        ));
        assert!(matches_conditions(
            &cond,
            &test_analysis(1000, false, false, "")
        ));
        assert!(matches_conditions(
            &cond,
            &test_analysis(5000, false, false, "")
        ));
    }

    #[test]
    fn test_matches_conditions_max_chars() {
        let cond = RoutingConditions {
            max_input_chars: Some(1000),
            ..Default::default()
        };
        assert!(matches_conditions(
            &cond,
            &test_analysis(500, false, false, "")
        ));
        assert!(matches_conditions(
            &cond,
            &test_analysis(1000, false, false, "")
        ));
        assert!(!matches_conditions(
            &cond,
            &test_analysis(5000, false, false, "")
        ));
    }

    #[test]
    fn test_matches_conditions_has_images() {
        let cond = RoutingConditions {
            has_images: Some(true),
            ..Default::default()
        };
        assert!(!matches_conditions(
            &cond,
            &test_analysis(100, false, false, "")
        ));
        assert!(matches_conditions(
            &cond,
            &test_analysis(100, true, false, "")
        ));
    }

    #[test]
    fn test_matches_conditions_has_tools() {
        let cond = RoutingConditions {
            has_tools: Some(true),
            ..Default::default()
        };
        assert!(!matches_conditions(
            &cond,
            &test_analysis(100, false, false, "")
        ));
        assert!(matches_conditions(
            &cond,
            &test_analysis(100, false, true, "")
        ));
    }

    #[test]
    fn test_matches_conditions_system_keywords() {
        let cond = RoutingConditions {
            system_keywords: Some(vec!["background".to_string(), "subagent".to_string()]),
            ..Default::default()
        };
        assert!(!matches_conditions(
            &cond,
            &test_analysis(100, false, false, "You are a helpful assistant")
        ));
        assert!(matches_conditions(
            &cond,
            &test_analysis(100, false, false, "Run this in background mode")
        ));
        assert!(matches_conditions(
            &cond,
            &test_analysis(100, false, false, "This is a SUBAGENT task")
        ));
    }

    #[test]
    fn test_matches_conditions_combined() {
        let cond = RoutingConditions {
            min_input_chars: Some(1000),
            has_images: Some(true),
            ..Default::default()
        };
        assert!(!matches_conditions(
            &cond,
            &test_analysis(500, true, false, "")
        )); // chars too low
        assert!(!matches_conditions(
            &cond,
            &test_analysis(2000, false, false, "")
        )); // no images
        assert!(matches_conditions(
            &cond,
            &test_analysis(2000, true, false, "")
        )); // both match
    }

    #[test]
    fn test_matches_conditions_parse_json() {
        let json = r#"{"has_images": true, "system_keywords": ["background"]}"#;
        let cond: RoutingConditions = serde_json::from_str(json).unwrap();
        assert!(matches_conditions(
            &cond,
            &test_analysis(100, true, false, "background task")
        ));
        assert!(!matches_conditions(
            &cond,
            &test_analysis(100, false, false, "background task")
        ));
    }

    #[test]
    fn build_candidates_skips_invalid_routing_conditions() {
        let conn = Connection::open_in_memory().unwrap();
        let provider = RouteProfileProviderView {
            id: "rpp1".to_string(),
            provider_id: "p1".to_string(),
            provider_name: "BrokenConditions".to_string(),
            provider_type: "openai".to_string(),
            provider_protocol: "openai_responses".to_string(),
            has_anthropic_url: false,
            supports_vision: None,
            model_capabilities: None,
            priority: 1,
            enabled: true,
            model_override: Some("gpt-4".to_string()),
            cooldown_seconds: 600,
            failover_on_status_codes: None,
            failover_on_error_keywords: None,
            routing_conditions: Some("{bad-json".to_string()),
            runtime_available: true,
            cooldown_until: None,
            consecutive_failures: 0,
        };
        let analysis = test_analysis(100, false, false, "");

        let candidates = build_candidates(&conn, &[provider], None, Some(&analysis)).unwrap();

        assert!(
            candidates.is_empty(),
            "invalid routing_conditions must not widen the provider match"
        );
    }

    #[test]
    fn test_should_failover_keyword_case_insensitive() {
        let c = candidate_with_defaults();
        assert!(should_failover(None, "RATE LIMIT", &c));
        assert!(should_failover(None, "Timeout", &c));
        assert!(should_failover(None, "TIMEOUT", &c));
    }

    // ── Degradation chain tests ──

    fn provider_with_chain(chain_json: Option<&str>) -> Provider {
        Provider {
            id: "p".into(),
            name: "P".into(),
            provider_type: "openai".into(),
            base_url: "https://api.openai.com".into(),
            api_key: Some("sk-x".into()),
            default_model: "gpt-5-codex".into(),
            reasoning_model: None,
            supported_models: None,
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            responses_base_url: None,
            protocol: "openai_responses".into(),
            timeout_seconds: 120,
            status: "ok".into(),
            supports_vision: None,
            auto_cache_control: None,
            supports_cache: None,
            model_capabilities: None,
            provider_quirks: None,
            body_filter_enabled: None,
            thinking_rectifier_enabled: None,
            error_mapper_enabled: None,
            model_degradation_chain: chain_json.map(|s| s.to_string()),
            model_context_windows: None,
            enabled: true,
            is_active: true,
            created_at: "2024".into(),
            updated_at: "2024".into(),
        }
    }

    #[test]
    fn degradation_chain_returns_requested_then_fallbacks() {
        let p = provider_with_chain(Some(r#"{"gpt-5-codex":["gpt-5-mini","gpt-4o"]}"#));
        assert_eq!(
            degradation_chain_for_model(&p, "gpt-5-codex"),
            vec!["gpt-5-codex", "gpt-5-mini", "gpt-4o"]
        );
    }

    #[test]
    fn degradation_chain_returns_just_requested_when_no_entry() {
        let p = provider_with_chain(Some(r#"{"gpt-5-codex":["gpt-5-mini"]}"#));
        assert_eq!(
            degradation_chain_for_model(&p, "claude-sonnet-4"),
            vec!["claude-sonnet-4"]
        );
    }

    #[test]
    fn degradation_chain_handles_missing_config() {
        let p = provider_with_chain(None);
        assert_eq!(
            degradation_chain_for_model(&p, "gpt-5-codex"),
            vec!["gpt-5-codex"]
        );
    }

    #[test]
    fn degradation_chain_dedupes_and_skips_self_reference() {
        // Pathological config: chain includes the requested model and a dup.
        // The walker should not loop or revisit a model.
        let p = provider_with_chain(Some(
            r#"{"gpt-5-codex":["gpt-5-codex","gpt-5-mini","gpt-5-mini","gpt-4o"]}"#,
        ));
        assert_eq!(
            degradation_chain_for_model(&p, "gpt-5-codex"),
            vec!["gpt-5-codex", "gpt-5-mini", "gpt-4o"]
        );
    }

    #[test]
    fn degradation_chain_handles_invalid_json() {
        let p = provider_with_chain(Some("not-json-at-all"));
        assert_eq!(
            degradation_chain_for_model(&p, "anything"),
            vec!["anything"]
        );
    }
}
