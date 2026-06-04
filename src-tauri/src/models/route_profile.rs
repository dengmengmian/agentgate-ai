use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfile {
    pub id: String,
    pub name: String,
    pub input_protocol: String,
    pub mode: String, // "manual" | "failover"
    /// failover 候选排序策略："priority"（默认）| "cheapest" | "fastest"
    pub selection_strategy: String,
    pub active_provider_id: Option<String>,
    pub enabled: bool,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_route_profile_serde() {
        let profile = RouteProfile {
            id: "rp1".to_string(),
            name: "Default".to_string(),
            input_protocol: "openai_responses".to_string(),
            mode: "manual".to_string(),
            selection_strategy: "priority".to_string(),
            active_provider_id: Some("p1".to_string()),
            enabled: true,
            is_default: true,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        };
        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("Default"));
        let de: RouteProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(de.id, "rp1");
        assert!(de.is_default);
    }

    #[test]
    fn test_route_profile_view_serde() {
        let view = RouteProfileView {
            id: "rp1".to_string(),
            name: "Default".to_string(),
            input_protocol: "openai_responses".to_string(),
            mode: "manual".to_string(),
            selection_strategy: "priority".to_string(),
            active_provider_id: Some("p1".to_string()),
            active_provider_name: Some("OpenAI".to_string()),
            enabled: true,
            is_default: false,
            providers_count: 2,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        };
        let json = serde_json::to_string(&view).unwrap();
        let de: RouteProfileView = serde_json::from_str(&json).unwrap();
        assert_eq!(de.providers_count, 2);
    }

    #[test]
    fn test_create_route_profile_input_serde() {
        let input = CreateRouteProfileInput {
            name: "New".to_string(),
            input_protocol: "openai_responses".to_string(),
            mode: Some("failover".to_string()),
        };
        let json = serde_json::to_string(&input).unwrap();
        let de: CreateRouteProfileInput = serde_json::from_str(&json).unwrap();
        assert_eq!(de.mode, Some("failover".to_string()));
    }

    #[test]
    fn test_add_provider_to_route_input_serde() {
        let input = AddProviderToRouteInput {
            priority: Some(1),
            model_override: Some("gpt-4o".to_string()),
            cooldown_seconds: Some(300),
            failover_on_status_codes: Some("[429,500]".to_string()),
            failover_on_error_keywords: Some("[\"timeout\"]".to_string()),
            routing_conditions: Some(r#"{"has_images":true}"#.to_string()),
        };
        let json = serde_json::to_string(&input).unwrap();
        let de: AddProviderToRouteInput = serde_json::from_str(&json).unwrap();
        assert_eq!(de.cooldown_seconds, Some(300));
    }

    #[test]
    fn test_provider_runtime_status_serde() {
        let status = ProviderRuntimeStatus {
            provider_id: "p1".to_string(),
            available: true,
            consecutive_failures: 0,
            last_error: None,
            last_error_code: None,
            last_error_at: None,
            cooldown_until: None,
            quota_exhausted: false,
            last_probe_ok: None,
            last_probe_at: None,
            last_probe_latency_ms: None,
            last_probe_error: None,
            updated_at: "2024-01-01".to_string(),
        };
        let json = serde_json::to_string(&status).unwrap();
        let de: ProviderRuntimeStatus = serde_json::from_str(&json).unwrap();
        assert!(de.available);
        assert!(!de.quota_exhausted);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfileView {
    pub id: String,
    pub name: String,
    pub input_protocol: String,
    pub mode: String,
    pub selection_strategy: String,
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
    pub model_capabilities: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRouteProfileInput {
    pub name: String,
    pub input_protocol: String,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRouteProfileInput {
    pub name: Option<String>,
    pub mode: Option<String>,
    pub selection_strategy: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// 主动健康探测结果（后台定期探测，仅展示，不参与路由决策）
    pub last_probe_ok: Option<bool>,
    pub last_probe_at: Option<String>,
    pub last_probe_latency_ms: Option<i64>,
    pub last_probe_error: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteProfileStats {
    pub route_profile_id: String,
    pub request_count: i64,
    pub success_count: i64,
    pub error_count: i64,
    pub success_rate: f64,
    pub avg_latency_ms: i64,
    pub cost: f64,
}
