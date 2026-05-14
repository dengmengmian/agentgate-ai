use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub default_model: String,
    pub reasoning_model: Option<String>,
    pub supported_models: Option<String>,
    pub model_mapping: Option<String>,
    pub extra_headers: Option<String>,
    pub anthropic_base_url: Option<String>, // e.g. https://api.deepseek.com/anthropic
    pub protocol: String,
    pub timeout_seconds: i64,
    pub status: String,
    pub enabled: bool,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub masked_api_key: Option<String>,
    pub default_model: String,
    pub reasoning_model: Option<String>,
    pub supported_models: Option<String>,
    pub model_mapping: Option<String>,
    pub extra_headers: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub protocol: String,
    pub timeout_seconds: i64,
    pub status: String,
    pub enabled: bool,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProviderInput {
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub default_model: String,
    pub reasoning_model: Option<String>,
    pub supported_models: Option<String>,
    pub model_mapping: Option<String>,
    pub extra_headers: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub protocol: String,
    pub timeout_seconds: Option<i64>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateProviderInput {
    pub name: Option<String>,
    pub provider_type: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub default_model: Option<String>,
    pub reasoning_model: Option<String>,
    pub supported_models: Option<String>,
    pub model_mapping: Option<String>,
    pub extra_headers: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub protocol: Option<String>,
    pub timeout_seconds: Option<i64>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderTestResult {
    pub success: bool,
    pub status: String,
    pub message: String,
    pub latency_ms: Option<u64>,
}

impl Provider {
    pub fn to_view(&self) -> ProviderView {
        ProviderView {
            id: self.id.clone(),
            name: self.name.clone(),
            provider_type: self.provider_type.clone(),
            base_url: self.base_url.clone(),
            masked_api_key: self.api_key.as_ref().map(|k| mask_api_key(k)),
            default_model: self.default_model.clone(),
            reasoning_model: self.reasoning_model.clone(),
            supported_models: self.supported_models.clone(),
            model_mapping: self.model_mapping.clone(),
            extra_headers: self.extra_headers.clone(),
            anthropic_base_url: self.anthropic_base_url.clone(),
            protocol: self.protocol.clone(),
            timeout_seconds: self.timeout_seconds,
            status: self.status.clone(),
            enabled: self.enabled,
            is_active: self.is_active,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    /// Check if a model name is known to this provider.
    pub fn is_model_supported(&self, model: &str) -> bool {
        if model == self.default_model { return true; }
        if self.reasoning_model.as_deref() == Some(model) { return true; }
        if let Some(ref sm) = self.supported_models {
            if let Ok(list) = serde_json::from_str::<Vec<String>>(sm) {
                if list.iter().any(|m| m == model) { return true; }
            }
        }
        false
    }

    /// Resolve a client model name to the provider's actual model name.
    /// Priority: model_mapping → supported_models match → default_model.
    pub fn resolve_model(&self, requested: &str) -> String {
        // 1. Check model_mapping first
        if let Some(ref mm) = self.model_mapping {
            if let Ok(map) = serde_json::from_str::<std::collections::HashMap<String, String>>(mm) {
                if let Some(mapped) = map.get(requested) {
                    return mapped.clone();
                }
            }
        }
        // 2. If the requested model is natively supported, use it directly
        if self.is_model_supported(requested) {
            return requested.to_string();
        }
        // 3. Fallback to default_model
        self.default_model.clone()
    }
}

pub fn mask_api_key(key: &str) -> String {
    if key.len() <= 8 {
        return "*".repeat(key.len());
    }
    let prefix = &key[..4];
    let suffix = &key[key.len() - 4..];
    format!("{prefix}****{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key_short() {
        assert_eq!(mask_api_key("abc"), "***");
        assert_eq!(mask_api_key("abcdefgh"), "********");
    }

    #[test]
    fn test_mask_api_key_long() {
        let key = "sk-1234567890abcdef";
        let masked = mask_api_key(key);
        assert_eq!(masked, "sk-1****cdef");
    }

    #[test]
    fn test_provider_is_model_supported() {
        let provider = Provider {
            id: "1".to_string(),
            name: "Test".to_string(),
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com".to_string(),
            api_key: None,
            default_model: "gpt-4".to_string(),
            reasoning_model: Some("o1".to_string()),
            supported_models: Some("[\"gpt-3\", \"gpt-4-turbo\"]".to_string()),
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            protocol: "openai_chat_completions".to_string(),
            timeout_seconds: 60,
            status: "ok".to_string(),
            enabled: true,
            is_active: true,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        };

        assert!(provider.is_model_supported("gpt-4"));
        assert!(provider.is_model_supported("o1"));
        assert!(provider.is_model_supported("gpt-3"));
        assert!(provider.is_model_supported("gpt-4-turbo"));
        assert!(!provider.is_model_supported("unknown"));
    }

    #[test]
    fn test_provider_resolve_model_mapping() {
        let provider = Provider {
            id: "1".to_string(),
            name: "Test".to_string(),
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com".to_string(),
            api_key: None,
            default_model: "gpt-4".to_string(),
            reasoning_model: None,
            supported_models: None,
            model_mapping: Some(r#"{"gpt-5": "gpt-4-turbo", "custom": "gpt-3"}"#.to_string()),
            extra_headers: None,
            anthropic_base_url: None,
            protocol: "openai_chat_completions".to_string(),
            timeout_seconds: 60,
            status: "ok".to_string(),
            enabled: true,
            is_active: true,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        };

        assert_eq!(provider.resolve_model("gpt-5"), "gpt-4-turbo");
        assert_eq!(provider.resolve_model("custom"), "gpt-3");
    }

    #[test]
    fn test_provider_resolve_model_supported() {
        let provider = Provider {
            id: "1".to_string(),
            name: "Test".to_string(),
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com".to_string(),
            api_key: None,
            default_model: "gpt-4".to_string(),
            reasoning_model: None,
            supported_models: Some("[\"gpt-3\"]".to_string()),
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            protocol: "openai_chat_completions".to_string(),
            timeout_seconds: 60,
            status: "ok".to_string(),
            enabled: true,
            is_active: true,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        };

        assert_eq!(provider.resolve_model("gpt-3"), "gpt-3");
        assert_eq!(provider.resolve_model("unknown"), "gpt-4"); // fallback
    }

    #[test]
    fn test_provider_to_view() {
        let provider = Provider {
            id: "1".to_string(),
            name: "Test".to_string(),
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com".to_string(),
            api_key: Some("sk-secret123".to_string()),
            default_model: "gpt-4".to_string(),
            reasoning_model: None,
            supported_models: None,
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            protocol: "openai_chat_completions".to_string(),
            timeout_seconds: 60,
            status: "ok".to_string(),
            enabled: true,
            is_active: true,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        };

        let view = provider.to_view();
        assert_eq!(view.masked_api_key, Some("sk-s****t123".to_string()));
    }
}
