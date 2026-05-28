use crate::models::provider::{CreateProviderInput, UpdateProviderInput};

const MIMO_PAYG_BASE_URL: &str = "https://api.xiaomimimo.com/v1";
const MIMO_PAYG_ANTHROPIC_URL: &str = "https://api.xiaomimimo.com/anthropic";
const MIMO_TOKEN_PLAN_BASE_URL: &str = "https://token-plan-cn.xiaomimimo.com/v1";
const MIMO_TOKEN_PLAN_ANTHROPIC_URL: &str = "https://token-plan-cn.xiaomimimo.com/anthropic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EndpointUrls {
    base_url: &'static str,
    anthropic_base_url: &'static str,
}

pub fn apply_to_create_input(input: &mut CreateProviderInput) {
    if let Some(urls) = endpoints_for_provider_key(&input.provider_type, input.api_key.as_deref()) {
        if should_replace_mimo_url(&input.base_url) {
            input.base_url = urls.base_url.to_string();
        }
        if input
            .anthropic_base_url
            .as_deref()
            .map_or(true, should_replace_mimo_url)
        {
            input.anthropic_base_url = Some(urls.anthropic_base_url.to_string());
        }
    }
}

pub fn apply_to_update_input(
    provider_type: &str,
    effective_api_key: Option<&str>,
    current_base_url: &str,
    current_anthropic_base_url: Option<&str>,
    input: &mut UpdateProviderInput,
) {
    if let Some(urls) = endpoints_for_provider_key(provider_type, effective_api_key) {
        let base_candidate = input.base_url.as_deref().unwrap_or(current_base_url);
        if should_replace_mimo_url(base_candidate) {
            input.base_url = Some(urls.base_url.to_string());
        }
        let anthropic_candidate = input
            .anthropic_base_url
            .as_deref()
            .or(current_anthropic_base_url);
        if anthropic_candidate.map_or(true, should_replace_mimo_url) {
            input.anthropic_base_url = Some(urls.anthropic_base_url.to_string());
        }
    }
}

fn endpoints_for_provider_key(provider_type: &str, api_key: Option<&str>) -> Option<EndpointUrls> {
    if !is_mimo_provider_type(provider_type) {
        return None;
    }
    let key = first_api_key(api_key?)?;
    if key.starts_with("tp-") {
        Some(EndpointUrls {
            base_url: MIMO_TOKEN_PLAN_BASE_URL,
            anthropic_base_url: MIMO_TOKEN_PLAN_ANTHROPIC_URL,
        })
    } else if key.starts_with("sk-") {
        Some(EndpointUrls {
            base_url: MIMO_PAYG_BASE_URL,
            anthropic_base_url: MIMO_PAYG_ANTHROPIC_URL,
        })
    } else {
        None
    }
}

fn is_mimo_provider_type(provider_type: &str) -> bool {
    let pt = provider_type.trim().to_lowercase();
    pt == "mimo" || pt == "xiaomi" || pt.contains("mimo")
}

fn should_replace_mimo_url(url: &str) -> bool {
    let url = url.trim();
    url.is_empty()
        || url == MIMO_PAYG_BASE_URL
        || url == MIMO_PAYG_ANTHROPIC_URL
        || url == MIMO_TOKEN_PLAN_BASE_URL
        || url == MIMO_TOKEN_PLAN_ANTHROPIC_URL
}

fn first_api_key(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if value.starts_with('[') {
        if let Ok(keys) = serde_json::from_str::<Vec<String>>(value) {
            return keys
                .into_iter()
                .find(|key| !key.trim().is_empty())
                .map(|key| key.trim().to_string());
        }
    }
    Some(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(api_key: &str) -> CreateProviderInput {
        CreateProviderInput {
            name: "MiMo".into(),
            provider_type: "mimo".into(),
            base_url: MIMO_PAYG_BASE_URL.into(),
            api_key: Some(api_key.into()),
            default_model: "mimo-v2.5-pro".into(),
            reasoning_model: Some("mimo-v2.5-pro".into()),
            supported_models: None,
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: Some(MIMO_PAYG_ANTHROPIC_URL.into()),
            responses_base_url: None,
            protocol: "openai_chat_completions".into(),
            timeout_seconds: Some(120),
            auto_cache_control: Some(true),
            model_capabilities: None,
            enabled: Some(true),
        }
    }

    #[test]
    fn token_plan_key_uses_token_plan_hosts() {
        let mut input = input("tp-xxxxx");
        apply_to_create_input(&mut input);
        assert_eq!(input.base_url, MIMO_TOKEN_PLAN_BASE_URL);
        assert_eq!(
            input.anthropic_base_url.as_deref(),
            Some(MIMO_TOKEN_PLAN_ANTHROPIC_URL)
        );
    }

    #[test]
    fn payg_key_uses_regular_hosts() {
        let mut input = input("sk-xxxxx");
        input.base_url = MIMO_TOKEN_PLAN_BASE_URL.into();
        input.anthropic_base_url = Some(MIMO_TOKEN_PLAN_ANTHROPIC_URL.into());
        apply_to_create_input(&mut input);
        assert_eq!(input.base_url, MIMO_PAYG_BASE_URL);
        assert_eq!(
            input.anthropic_base_url.as_deref(),
            Some(MIMO_PAYG_ANTHROPIC_URL)
        );
    }

    #[test]
    fn custom_mimo_proxy_is_preserved() {
        let mut input = input("tp-xxxxx");
        input.base_url = "https://proxy.example.com/v1".into();
        input.anthropic_base_url = Some("https://proxy.example.com/anthropic".into());
        apply_to_create_input(&mut input);
        assert_eq!(input.base_url, "https://proxy.example.com/v1");
        assert_eq!(
            input.anthropic_base_url.as_deref(),
            Some("https://proxy.example.com/anthropic")
        );
    }

    #[test]
    fn json_key_array_uses_first_key() {
        let mut input = input(r#"["tp-first","sk-second"]"#);
        apply_to_create_input(&mut input);
        assert_eq!(input.base_url, MIMO_TOKEN_PLAN_BASE_URL);
    }

    #[test]
    fn update_preserves_existing_custom_proxy() {
        let mut input = UpdateProviderInput {
            name: Some("Renamed".into()),
            provider_type: None,
            base_url: None,
            api_key: None,
            default_model: None,
            reasoning_model: None,
            supported_models: None,
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            responses_base_url: None,
            auto_cache_control: None,
            model_capabilities: None,
            protocol: None,
            timeout_seconds: None,
            enabled: None,
        };
        apply_to_update_input(
            "mimo",
            Some("tp-xxxxx"),
            "https://proxy.example.com/v1",
            Some("https://proxy.example.com/anthropic"),
            &mut input,
        );
        assert_eq!(input.base_url, None);
        assert_eq!(input.anthropic_base_url, None);
    }
}
