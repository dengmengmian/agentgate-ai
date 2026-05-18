use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfile {
    pub id: String,
    pub name: String,
    pub input_protocol: String,
    pub mode: String, // "manual" | "failover"
    pub active_provider_id: Option<String>,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfileView {
    pub id: String,
    pub name: String,
    pub input_protocol: String,
    pub mode: String,
    pub active_provider_id: Option<String>,
    pub active_provider_name: Option<String>,
    pub enabled: bool,
    pub is_default: bool,
    pub providers_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfileDetail {
    pub profile: RouteProfileView,
    pub providers: Vec<RouteProfileProviderView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfileProviderView {
    pub id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub provider_type: String,
    pub provider_protocol: String,
    pub has_anthropic_url: bool,
    pub supports_vision: Option<bool>,
    pub priority: i64,
    pub enabled: bool,
    pub model_override: Option<String>,
    pub cooldown_seconds: i64,
    pub failover_on_status_codes: Option<String>,
    pub failover_on_error_keywords: Option<String>,
    pub routing_conditions: Option<String>,
    pub runtime_available: bool,
    pub cooldown_until: Option<String>,
    pub consecutive_failures: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRouteProfileInput {
    pub name: String,
    pub input_protocol: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRouteProfileInput {
    pub name: Option<String>,
    pub mode: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddProviderToRouteInput {
    pub priority: Option<i64>,
    pub model_override: Option<String>,
    pub cooldown_seconds: Option<i64>,
    pub failover_on_status_codes: Option<String>,
    pub failover_on_error_keywords: Option<String>,
    pub routing_conditions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRuntimeStatus {
    pub provider_id: String,
    pub available: bool,
    pub consecutive_failures: i64,
    pub last_error: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_at: Option<String>,
    pub cooldown_until: Option<String>,
    pub quota_exhausted: bool,
    pub updated_at: String,
}
