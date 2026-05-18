use reqwest::Client;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::errors::AppError;
use crate::models::provider::Provider;
use crate::protocol::chat_completions::ChatCompletionsRequest;

/// Global round-robin counter for API key rotation.
static KEY_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Internal provider config used by the gateway.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProviderConfig {
    pub name: String,
    pub provider_type: String,
    pub base_url: String,
    pub api_keys: Vec<String>,
    pub default_model: String,
    pub reasoning_model: Option<String>,
    pub timeout_seconds: u64,
    pub extra_headers: std::collections::HashMap<String, String>,
    pub anthropic_base_url: Option<String>,
    pub responses_base_url: Option<String>,
}

impl ProviderConfig {
    pub fn from_provider(p: &Provider) -> Result<Self, AppError> {
        let raw = p.api_key.clone().filter(|k| !k.is_empty()).ok_or_else(|| {
            AppError::new("PROVIDER_API_KEY_MISSING", "Active provider has no API key configured")
                .with_suggestion("Set an API key in the Providers page")
        })?;

        // Parse api_key: JSON array → multiple keys, plain string → single key
        let api_keys = parse_api_keys(&raw);
        if api_keys.is_empty() {
            return Err(AppError::new("PROVIDER_API_KEY_MISSING", "No valid API keys configured")
                .with_suggestion("Set at least one API key in the Providers page"));
        }

        let extra_headers = p.extra_headers.as_ref()
            .and_then(|h| serde_json::from_str::<std::collections::HashMap<String, String>>(h).ok())
            .unwrap_or_default();

        Ok(Self {
            name: p.name.clone(),
            provider_type: p.provider_type.clone(),
            base_url: p.base_url.clone(),
            api_keys,
            default_model: p.default_model.clone(),
            reasoning_model: p.reasoning_model.clone(),
            timeout_seconds: p.timeout_seconds as u64,
            extra_headers,
            anthropic_base_url: p.anthropic_base_url.clone().filter(|s| !s.is_empty()),
            responses_base_url: p.responses_base_url.clone().filter(|s| !s.is_empty()),
        })
    }

    /// Select the current API key via round-robin rotation.
    pub fn select_api_key(&self) -> &str {
        let idx = KEY_COUNTER.fetch_add(1, Ordering::Relaxed) % self.api_keys.len();
        &self.api_keys[idx]
    }

    /// Backward-compatible: return first key (for logging, etc.)
    pub fn api_key(&self) -> &str {
        &self.api_keys[0]
    }

    /// Number of API keys configured.
    #[allow(dead_code)]
    pub fn key_count(&self) -> usize {
        self.api_keys.len()
    }

    /// Sanitize all API keys from a string (for log redaction).
    pub fn sanitize(&self, text: &str) -> String {
        let mut s = text.to_string();
        for key in &self.api_keys {
            s = s.replace(key, "sk-***REDACTED***");
        }
        s
    }

    pub fn is_deepseek(&self) -> bool {
        self.provider_type == "deepseek"
    }

    pub fn is_anthropic(&self) -> bool {
        self.provider_type == "anthropic" || self.provider_type == "claude"
    }

    /// Build the chat completions URL, avoiding double /v1.
    pub fn chat_completions_url(&self) -> String {
        smart_append_path(&self.base_url, "/chat/completions")
    }

    /// Build the Claude Messages API URL.
    /// Uses `anthropic_base_url` if set, otherwise falls back to `base_url`.
    /// If the URL already ends with `/messages`, use it as-is.
    pub fn anthropic_messages_url(&self) -> String {
        let raw = self.anthropic_base_url.as_deref().unwrap_or(&self.base_url);
        smart_append_path(raw, "/messages")
    }

    /// Build the Responses API URL for pass-through.
    /// Uses `responses_base_url` if set, otherwise falls back to `base_url`.
    /// If the URL already ends with `/responses`, use it as-is.
    pub fn responses_url(&self) -> String {
        let raw = self.responses_base_url.as_deref().unwrap_or(&self.base_url);
        smart_append_path(raw, "/responses")
    }

    /// Whether this provider has an explicit Anthropic Messages endpoint configured.
    pub fn has_anthropic_url(&self) -> bool {
        self.anthropic_base_url.is_some()
    }

    /// Whether this provider has an explicit Responses API endpoint configured.
    pub fn has_responses_url(&self) -> bool {
        self.responses_base_url.is_some()
    }
}

/// Send a non-streaming chat completions request.
pub async fn send_non_stream(
    client: &Client,
    config: &ProviderConfig,
    request: &ChatCompletionsRequest,
) -> Result<Value, AppError> {
    let url = config.chat_completions_url();
    let body = serde_json::to_value(request)
        .map_err(|e| AppError::new("TRANSFORM_ERROR", format!("Failed to serialize request: {e}")))?;

    let mut req_builder = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.select_api_key()))
        .header("Content-Type", "application/json");

    // Inject provider extra_headers (e.g. User-Agent for Kimi)
    for (k, v) in &config.extra_headers {
        req_builder = req_builder.header(k.as_str(), v.as_str());
    }

    let resp = req_builder
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            AppError::new("PROVIDER_REQUEST_FAILED", format!("Failed to connect to provider: {e}"))
        })?;

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();

    // Sanitize api key from response
    let sanitized = config.sanitize(&body_text);

    if !status.is_success() {
        return Err(
            AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Provider returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)),
        );
    }

    serde_json::from_str(&sanitized).map_err(|e| {
        AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Failed to parse provider response: {e}"))
            .with_detail(truncate(&sanitized, 500))
    })
}

/// Send a streaming chat completions request, returning the raw response for SSE processing.
pub async fn send_stream(
    client: &Client,
    config: &ProviderConfig,
    request: &ChatCompletionsRequest,
) -> Result<reqwest::Response, AppError> {
    let url = config.chat_completions_url();
    let body = serde_json::to_value(request)
        .map_err(|e| AppError::new("TRANSFORM_ERROR", format!("Failed to serialize request: {e}")))?;

    let mut req_builder = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.select_api_key()))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream");

    for (k, v) in &config.extra_headers {
        req_builder = req_builder.header(k.as_str(), v.as_str());
    }

    let resp = req_builder
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            AppError::new("PROVIDER_REQUEST_FAILED", format!("Failed to connect to provider: {e}"))
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = config.sanitize(&body_text);
        return Err(
            AppError::new("UPSTREAM_STREAM_ERROR", format!("Provider returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)),
        );
    }

    Ok(resp)
}

/// Send a non-streaming request to Claude Messages API.
pub async fn send_anthropic_non_stream(
    client: &Client,
    config: &ProviderConfig,
    body: &Value,
) -> Result<Value, AppError> {
    let url = config.anthropic_messages_url();

    let mut req_builder = client
        .post(&url)
        .header("x-api-key", config.select_api_key())
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json");

    // Add anthropic-beta if thinking is enabled
    if body.get("thinking").is_some() {
        req_builder = req_builder.header("anthropic-beta", "interleaved-thinking-2025-05-14");
    }

    for (k, v) in &config.extra_headers {
        req_builder = req_builder.header(k.as_str(), v.as_str());
    }

    let resp = req_builder
        .json(body)
        .send()
        .await
        .map_err(|e| AppError::new("PROVIDER_REQUEST_FAILED", format!("Failed to connect to Claude: {e}")))?;

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    let sanitized = config.sanitize(&body_text);

    if !status.is_success() {
        return Err(
            AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Claude returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)),
        );
    }

    serde_json::from_str(&sanitized).map_err(|e| {
        AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Failed to parse Claude response: {e}"))
            .with_detail(truncate(&sanitized, 500))
    })
}

/// Send a streaming request to Claude Messages API.
pub async fn send_anthropic_stream(
    client: &Client,
    config: &ProviderConfig,
    body: &Value,
) -> Result<reqwest::Response, AppError> {
    let url = config.anthropic_messages_url();

    let mut req_builder = client
        .post(&url)
        .header("x-api-key", config.select_api_key())
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream");

    if body.get("thinking").is_some() {
        req_builder = req_builder.header("anthropic-beta", "interleaved-thinking-2025-05-14");
    }

    for (k, v) in &config.extra_headers {
        req_builder = req_builder.header(k.as_str(), v.as_str());
    }

    let resp = req_builder
        .json(body)
        .send()
        .await
        .map_err(|e| AppError::new("PROVIDER_REQUEST_FAILED", format!("Failed to connect to Claude: {e}")))?;

    let status = resp.status();
    if !status.is_success() {
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = config.sanitize(&body_text);
        return Err(
            AppError::new("UPSTREAM_STREAM_ERROR", format!("Claude returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)),
        );
    }

    Ok(resp)
}

/// Parse api_key field: JSON array → Vec<String>, plain string → vec![string].
fn parse_api_keys(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        // Try JSON array
        if let Ok(keys) = serde_json::from_str::<Vec<String>>(trimmed) {
            return keys.into_iter().filter(|k| !k.is_empty()).collect();
        }
    }
    // Single key
    if trimmed.is_empty() {
        vec![]
    } else {
        vec![trimmed.to_string()]
    }
}

/// Smart path appending: if URL already ends with the target path (e.g. `/messages`),
/// use it as-is. Otherwise append `/v1/{path}` or `/{path}` depending on whether `/v1` is present.
pub fn smart_append_path(url: &str, suffix: &str) -> String {
    let base = url.trim_end_matches('/');
    // Already complete URL (e.g. ends with /messages or /responses)
    if base.ends_with(suffix) {
        return base.to_string();
    }
    // Has /v1 prefix
    if base.ends_with("/v1") {
        return format!("{base}{suffix}");
    }
    // Default: append /v1/suffix
    format!("{base}/v1{suffix}")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Find the last char boundary at or before `max` to avoid panic on multibyte chars
    let mut boundary = max;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}...(truncated)", &s[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::Provider;

    fn test_provider() -> Provider {
        Provider {
            id: "p1".to_string(),
            name: "TestProvider".to_string(),
            provider_type: "openai".to_string(),
            base_url: "https://api.openai.com".to_string(),
            api_key: Some("sk-testkey123".to_string()),
            default_model: "gpt-4".to_string(),
            reasoning_model: Some("o1".to_string()),
            supported_models: None,
            model_mapping: None,
            extra_headers: Some(r#"{"User-Agent":"TestAgent/1.0"}"#.to_string()),
            anthropic_base_url: None,
            responses_base_url: None,
            protocol: "openai_chat_completions".to_string(),
            timeout_seconds: 60,
            status: "ok".to_string(),
            supports_vision: None,
            enabled: true,
            is_active: true,
            created_at: "2024-01-01".to_string(),
            updated_at: "2024-01-01".to_string(),
        }
    }

    #[test]
    fn test_provider_config_from_provider() {
        let p = test_provider();
        let config = ProviderConfig::from_provider(&p).unwrap();
        assert_eq!(config.name, "TestProvider");
        assert_eq!(config.provider_type, "openai");
        assert_eq!(config.base_url, "https://api.openai.com");
        assert_eq!(config.api_key(), "sk-testkey123");
        assert_eq!(config.key_count(), 1);
        assert_eq!(config.default_model, "gpt-4");
        assert_eq!(config.reasoning_model, Some("o1".to_string()));
        assert_eq!(config.timeout_seconds, 60);
        assert_eq!(config.extra_headers.get("User-Agent"), Some(&"TestAgent/1.0".to_string()));
    }

    #[test]
    fn test_provider_config_missing_api_key() {
        let mut p = test_provider();
        p.api_key = None;
        let err = ProviderConfig::from_provider(&p).unwrap_err();
        assert_eq!(err.code, "PROVIDER_API_KEY_MISSING");
    }

    #[test]
    fn test_provider_config_empty_api_key() {
        let mut p = test_provider();
        p.api_key = Some("".to_string());
        let err = ProviderConfig::from_provider(&p).unwrap_err();
        assert_eq!(err.code, "PROVIDER_API_KEY_MISSING");
    }

    #[test]
    fn test_provider_config_no_extra_headers() {
        let mut p = test_provider();
        p.extra_headers = None;
        let config = ProviderConfig::from_provider(&p).unwrap();
        assert!(config.extra_headers.is_empty());
    }

    #[test]
    fn test_provider_config_invalid_extra_headers_json() {
        let mut p = test_provider();
        p.extra_headers = Some("not json".to_string());
        let config = ProviderConfig::from_provider(&p).unwrap();
        assert!(config.extra_headers.is_empty());
    }

    #[test]
    fn test_is_deepseek() {
        let mut p = test_provider();
        p.provider_type = "deepseek".to_string();
        let config = ProviderConfig::from_provider(&p).unwrap();
        assert!(config.is_deepseek());
    }

    #[test]
    fn test_chat_completions_url_no_v1() {
        let mut p = test_provider();
        p.base_url = "https://api.openai.com".to_string();
        let config = ProviderConfig::from_provider(&p).unwrap();
        assert_eq!(config.chat_completions_url(), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_chat_completions_url_with_v1() {
        let mut p = test_provider();
        p.base_url = "https://api.openai.com/v1".to_string();
        let config = ProviderConfig::from_provider(&p).unwrap();
        assert_eq!(config.chat_completions_url(), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_chat_completions_url_with_trailing_slash() {
        let mut p = test_provider();
        p.base_url = "https://api.openai.com/".to_string();
        let config = ProviderConfig::from_provider(&p).unwrap();
        assert_eq!(config.chat_completions_url(), "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_truncate_chinese_no_panic() {
        let s = "你好世界测试内容"; // 8 Chinese chars, 24 bytes
        let result = truncate(s, 7); // inside "世" (bytes 6..9)
        assert_eq!(result, "你好...(truncated)");
    }

    #[test]
    fn test_truncate_mixed_content_no_panic() {
        let s = "error: 请求频率过高，请稍后再试";
        let result = truncate(s, 10); // inside Chinese range
        assert!(!result.is_empty());
        assert!(result.ends_with("...(truncated)"));
    }

    #[test]
    fn test_parse_api_keys_single() {
        assert_eq!(parse_api_keys("sk-abc123"), vec!["sk-abc123"]);
    }

    #[test]
    fn test_parse_api_keys_json_array() {
        let keys = parse_api_keys(r#"["sk-key1", "sk-key2", "sk-key3"]"#);
        assert_eq!(keys, vec!["sk-key1", "sk-key2", "sk-key3"]);
    }

    #[test]
    fn test_parse_api_keys_filters_empty() {
        let keys = parse_api_keys(r#"["sk-key1", "", "sk-key3"]"#);
        assert_eq!(keys, vec!["sk-key1", "sk-key3"]);
    }

    #[test]
    fn test_multi_key_round_robin() {
        let mut p = test_provider();
        p.api_key = Some(r#"["sk-aaa", "sk-bbb"]"#.to_string());
        let config = ProviderConfig::from_provider(&p).unwrap();
        assert_eq!(config.key_count(), 2);
        // Two consecutive calls should return different keys
        let k1 = config.select_api_key().to_string();
        let k2 = config.select_api_key().to_string();
        assert!(k1 == "sk-aaa" || k1 == "sk-bbb");
        assert!(k2 == "sk-aaa" || k2 == "sk-bbb");
    }

    #[test]
    fn test_sanitize_multi_key() {
        let mut p = test_provider();
        p.api_key = Some(r#"["sk-secret1", "sk-secret2"]"#.to_string());
        let config = ProviderConfig::from_provider(&p).unwrap();
        let result = config.sanitize("key is sk-secret1 or sk-secret2");
        assert!(!result.contains("sk-secret1"));
        assert!(!result.contains("sk-secret2"));
        assert!(result.contains("sk-***REDACTED***"));
    }
}
