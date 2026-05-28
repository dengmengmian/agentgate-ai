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

    pub fn is_gemini(&self) -> bool {
        self.provider_type == "google_gemini"
    }

    /// Build the Gemini generateContent URL (includes model name in path).
    pub fn gemini_generate_url(&self, model: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        // If base already ends with the full path, use as-is
        if base.ends_with(":generateContent") {
            return base.to_string();
        }
        if base.contains("/models/") {
            return format!("{base}:generateContent");
        }
        format!("{base}/v1beta/models/{model}:generateContent")
    }

    /// Build the Gemini streamGenerateContent URL.
    pub fn gemini_stream_url(&self, model: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with(":streamGenerateContent") {
            return base.to_string();
        }
        if base.contains("/models/") {
            return format!("{base}:streamGenerateContent?alt=sse");
        }
        format!("{base}/v1beta/models/{model}:streamGenerateContent?alt=sse")
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

/// Status codes that are safe to retry (transient errors).
const RETRYABLE_STATUS: &[u16] = &[429, 500, 502, 503];
const MAX_RETRIES: usize = 2;
const RETRY_BASE_MS: u64 = 1000;

/// Check if a status code is retryable.
fn is_retryable(status: u16) -> bool {
    RETRYABLE_STATUS.contains(&status)
}

/// Parse Retry-After header (seconds).
fn parse_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers().get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// True for reqwest errors that are typically retryable: connection failure,
/// timeout, or a generic request-building error (often a closed keep-alive
/// connection the pool handed out). Status-code errors are NOT included —
/// they belong to the HTTP retry path.
pub fn is_transient_net_err(e: &reqwest::Error) -> bool {
    e.is_connect() || e.is_timeout() || e.is_request()
}

/// Send a request with retry on transient network errors. The closure must
/// build a fresh RequestBuilder per attempt so it can be re-sent after
/// failure. Returns the first successful response, or the last error after
/// `max_retries` attempts.
pub async fn send_with_net_retry<F>(
    build: F,
    max_retries: usize,
) -> Result<reqwest::Response, reqwest::Error>
where
    F: Fn() -> reqwest::RequestBuilder,
{
    let mut last_err: Option<reqwest::Error> = None;
    for attempt in 0..=max_retries {
        match build().send().await {
            Ok(r) => return Ok(r),
            Err(e) => {
                if is_transient_net_err(&e) && attempt < max_retries {
                    eprintln!(
                        "[net-retry] transient send error attempt {}/{}: {e}",
                        attempt + 1, max_retries,
                    );
                    let backoff_ms = 200_u64 * (attempt as u64 + 1);
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                    last_err = Some(e);
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(last_err.expect("loop only exits via Err when retries exhausted"))
}

/// Build an upstream error with provider-specific enhancement attached.
/// Looks up the provider's transform impl and asks it whether the response
/// body matches a known error pattern that warrants a friendlier suggestion
/// (e.g. MiMo's "webSearchEnabled is false" → activation hint).
fn build_upstream_error(
    config: &ProviderConfig,
    code: &str,
    message: String,
    status_code: u16,
    body_snippet: &str,
) -> AppError {
    // 上游网关有时直接返回 HTML 错误页（openresty / nginx / cloudflare 等
    // 反代，到 origin 的连接挂了时返回 502/503/504 HTML），如果原样塞进 detail
    // 用户看到的是"Provider returned HTTP 500 Internal Server Error: <html>...
    // <h1>502 Bad Gateway</h1>...</html>"——又长又懵。检测到这类页面就压缩成
    // 一句中文描述，把 HTML 留给 raw_response（log row）做完整取证。
    let detail = match summarize_html_gateway_error(body_snippet) {
        Some(s) => s,
        None => truncate(body_snippet, 2000),
    };
    let mut err = AppError::new(code, message).with_detail(detail);
    let transform = crate::transform::providers::for_config(config);
    if let Some(suggestion) = transform.enhance_error(status_code, body_snippet) {
        err = err.with_suggestion(suggestion);
    }
    err
}

/// 识别上游网关 HTML 错误页（openresty / nginx / cloudflare 等），抽出
/// 关键状态码 + 网关守护进程名，返回一句中文摘要。
///
/// 触发条件：body 含 `<html` 且能从中提取 5xx 状态码或经典 marker。
/// 否则返回 None，调用方维持原 body truncated 行为。
fn summarize_html_gateway_error(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    if !lower.contains("<html") {
        return None;
    }
    // 上游网关 5xx 的典型 marker。顺序按出现频率排：openresty 是 MiMo / Kimi /
    // 大量国产 OpenAI 兼容上游的默认反代。
    let upstream_kind = if lower.contains("openresty") {
        "openresty"
    } else if lower.contains("cloudflare") {
        "cloudflare"
    } else if lower.contains("nginx") {
        "nginx"
    } else {
        "上游网关"
    };
    // 从 HTML 里取 502/503/504/500 这几个常见状态码（按优先级，502 最常见）。
    let upstream_status = ["502 Bad Gateway", "504 Gateway Time-out", "504 Gateway Timeout", "503 Service Unavailable", "500 Internal Server Error"]
        .iter()
        .find(|s| body.contains(*s))
        .copied()
        .unwrap_or("5xx 错误");
    Some(format!(
        "{upstream_kind} 返回 HTML 错误页：{upstream_status}。\
         通常是上游网关到其后端 origin 的连接出问题（origin 抖动/重启/超时），\
         不是请求本身的问题。建议立刻重试，或切到 route profile 里下一个 provider。"
    ))
}

/// Send a non-streaming chat completions request with automatic retry.
pub async fn send_non_stream(
    client: &Client,
    config: &ProviderConfig,
    request: &ChatCompletionsRequest,
) -> Result<Value, AppError> {
    let url = config.chat_completions_url();
    let body = serde_json::to_value(request)
        .map_err(|e| AppError::new("TRANSFORM_ERROR", format!("Failed to serialize request: {e}")))?;

    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        let mut req_builder = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.select_api_key()))
            .header("Content-Type", "application/json");

        for (k, v) in &config.extra_headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }

        let resp = match req_builder.json(&body).send().await {
            Ok(r) => r,
            Err(e) if is_transient_net_err(&e) && attempt < MAX_RETRIES => {
                eprintln!("[net-retry] {url} attempt {}/{MAX_RETRIES}: {e}", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1))).await;
                last_err = Some(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Transient connect failure attempt {}: {e}", attempt + 1)));
                continue;
            }
            Err(e) => {
                return Err(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Failed to connect to provider: {e}")));
            }
        };

        let status = resp.status();

        if status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let sanitized = config.sanitize(&body_text);
            return serde_json::from_str(&sanitized).map_err(|e| {
                AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Failed to parse provider response: {e}"))
                    .with_detail(truncate(&sanitized, 500))
            });
        }

        let status_code = status.as_u16();
        let retry_after = parse_retry_after(&resp);
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = config.sanitize(&body_text);

        if is_retryable(status_code) && attempt < MAX_RETRIES {
            let wait = retry_after.unwrap_or(RETRY_BASE_MS * (1 << attempt) / 1000).max(1);
            eprintln!("[retry] {url} HTTP {status_code}, attempt {}/{MAX_RETRIES}, waiting {wait}s", attempt + 1);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            last_err = Some(build_upstream_error(
                config, "UPSTREAM_NON_STREAM_ERROR",
                format!("Provider returned HTTP {status}"), status_code, &sanitized,
            ));
            continue;
        }

        return Err(build_upstream_error(
            config, "UPSTREAM_NON_STREAM_ERROR",
            format!("Provider returned HTTP {status}"), status_code, &sanitized,
        ));
    }

    Err(last_err.unwrap_or_else(|| AppError::new("UPSTREAM_NON_STREAM_ERROR", "All retries exhausted")))
}

/// Send a streaming chat completions request with automatic retry.
pub async fn send_stream(
    client: &Client,
    config: &ProviderConfig,
    request: &ChatCompletionsRequest,
) -> Result<reqwest::Response, AppError> {
    let url = config.chat_completions_url();
    let body = serde_json::to_value(request)
        .map_err(|e| AppError::new("TRANSFORM_ERROR", format!("Failed to serialize request: {e}")))?;

    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        let mut req_builder = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.select_api_key()))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream");

        for (k, v) in &config.extra_headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }

        let resp = match req_builder.json(&body).send().await {
            Ok(r) => r,
            Err(e) if is_transient_net_err(&e) && attempt < MAX_RETRIES => {
                eprintln!("[net-retry] {url} attempt {}/{MAX_RETRIES}: {e}", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1))).await;
                last_err = Some(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Transient connect failure attempt {}: {e}", attempt + 1)));
                continue;
            }
            Err(e) => {
                return Err(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Failed to connect to provider: {e}")));
            }
        };

        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }

        let status_code = status.as_u16();
        let retry_after = parse_retry_after(&resp);
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = config.sanitize(&body_text);

        if is_retryable(status_code) && attempt < MAX_RETRIES {
            let wait = retry_after.unwrap_or(RETRY_BASE_MS * (1 << attempt) / 1000).max(1);
            eprintln!("[retry] {url} HTTP {status_code}, attempt {}/{MAX_RETRIES}, waiting {wait}s", attempt + 1);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            last_err = Some(build_upstream_error(
                config, "UPSTREAM_STREAM_ERROR",
                format!("Provider returned HTTP {status}"), status_code, &sanitized,
            ));
            continue;
        }

        return Err(build_upstream_error(
            config, "UPSTREAM_STREAM_ERROR",
            format!("Provider returned HTTP {status}"), status_code, &sanitized,
        ));
    }

    Err(last_err.unwrap_or_else(|| AppError::new("UPSTREAM_STREAM_ERROR", "All retries exhausted")))
}

/// Send a non-streaming request to Claude Messages API with automatic retry.
pub async fn send_anthropic_non_stream(
    client: &Client,
    config: &ProviderConfig,
    body: &Value,
) -> Result<Value, AppError> {
    let url = config.anthropic_messages_url();
    let has_thinking = body.get("thinking").is_some();
    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        let mut req_builder = client
            .post(&url)
            .header("x-api-key", config.select_api_key())
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json");

        if has_thinking {
            req_builder = req_builder.header("anthropic-beta", "interleaved-thinking-2025-05-14");
        }
        for (k, v) in &config.extra_headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }

        let resp = match req_builder.json(body).send().await {
            Ok(r) => r,
            Err(e) if is_transient_net_err(&e) && attempt < MAX_RETRIES => {
                eprintln!("[net-retry] {url} attempt {}/{MAX_RETRIES}: {e}", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1))).await;
                last_err = Some(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Transient connect failure attempt {}: {e}", attempt + 1)));
                continue;
            }
            Err(e) => {
                return Err(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Failed to connect to Claude: {e}")));
            }
        };

        let status = resp.status();
        if status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let sanitized = config.sanitize(&body_text);
            return serde_json::from_str(&sanitized).map_err(|e| {
                AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Failed to parse Claude response: {e}"))
                    .with_detail(truncate(&sanitized, 500))
            });
        }

        let status_code = status.as_u16();
        let retry_after = parse_retry_after(&resp);
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = config.sanitize(&body_text);

        if is_retryable(status_code) && attempt < MAX_RETRIES {
            let wait = retry_after.unwrap_or(RETRY_BASE_MS * (1 << attempt) / 1000).max(1);
            eprintln!("[retry] {url} HTTP {status_code}, attempt {}/{MAX_RETRIES}, waiting {wait}s", attempt + 1);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            last_err = Some(AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Claude returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)));
            continue;
        }

        return Err(
            AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Claude returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)),
        );
    }

    Err(last_err.unwrap_or_else(|| AppError::new("UPSTREAM_NON_STREAM_ERROR", "All retries exhausted")))
}

/// Send a streaming request to Claude Messages API with automatic retry.
pub async fn send_anthropic_stream(
    client: &Client,
    config: &ProviderConfig,
    body: &Value,
) -> Result<reqwest::Response, AppError> {
    let url = config.anthropic_messages_url();
    let has_thinking = body.get("thinking").is_some();
    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        let mut req_builder = client
            .post(&url)
            .header("x-api-key", config.select_api_key())
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream");

        if has_thinking {
            req_builder = req_builder.header("anthropic-beta", "interleaved-thinking-2025-05-14");
        }
        for (k, v) in &config.extra_headers {
            req_builder = req_builder.header(k.as_str(), v.as_str());
        }

        let resp = match req_builder.json(body).send().await {
            Ok(r) => r,
            Err(e) if is_transient_net_err(&e) && attempt < MAX_RETRIES => {
                eprintln!("[net-retry] {url} attempt {}/{MAX_RETRIES}: {e}", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1))).await;
                last_err = Some(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Transient connect failure attempt {}: {e}", attempt + 1)));
                continue;
            }
            Err(e) => {
                return Err(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Failed to connect to Claude: {e}")));
            }
        };

        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }

        let status_code = status.as_u16();
        let retry_after = parse_retry_after(&resp);
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = config.sanitize(&body_text);

        if is_retryable(status_code) && attempt < MAX_RETRIES {
            let wait = retry_after.unwrap_or(RETRY_BASE_MS * (1 << attempt) / 1000).max(1);
            eprintln!("[retry] {url} HTTP {status_code}, attempt {}/{MAX_RETRIES}, waiting {wait}s", attempt + 1);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            last_err = Some(AppError::new("UPSTREAM_STREAM_ERROR", format!("Claude returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)));
            continue;
        }

        return Err(
            AppError::new("UPSTREAM_STREAM_ERROR", format!("Claude returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)),
        );
    }

    Err(last_err.unwrap_or_else(|| AppError::new("UPSTREAM_STREAM_ERROR", "All retries exhausted")))
}

/// Send a non-streaming request to Gemini API with retry.
pub async fn send_gemini_non_stream(
    client: &Client,
    config: &ProviderConfig,
    body: &Value,
    model: &str,
) -> Result<Value, AppError> {
    let url = config.gemini_generate_url(model);
    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        let resp = match client.post(&url)
            .header("Authorization", format!("Bearer {}", config.select_api_key()))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) if is_transient_net_err(&e) && attempt < MAX_RETRIES => {
                eprintln!("[net-retry] {url} attempt {}/{MAX_RETRIES}: {e}", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1))).await;
                last_err = Some(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Transient connect failure attempt {}: {e}", attempt + 1)));
                continue;
            }
            Err(e) => {
                return Err(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Failed to connect to Gemini: {e}")));
            }
        };

        let status = resp.status();
        if status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let sanitized = config.sanitize(&body_text);
            return serde_json::from_str(&sanitized).map_err(|e| {
                AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Failed to parse Gemini response: {e}"))
                    .with_detail(truncate(&sanitized, 500))
            });
        }

        let status_code = status.as_u16();
        let retry_after = parse_retry_after(&resp);
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = config.sanitize(&body_text);

        if is_retryable(status_code) && attempt < MAX_RETRIES {
            let wait = retry_after.unwrap_or(RETRY_BASE_MS * (1 << attempt) / 1000).max(1);
            eprintln!("[retry] {url} HTTP {status_code}, attempt {}/{MAX_RETRIES}, waiting {wait}s", attempt + 1);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            last_err = Some(AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Gemini returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)));
            continue;
        }

        return Err(AppError::new("UPSTREAM_NON_STREAM_ERROR", format!("Gemini returned HTTP {status}"))
            .with_detail(truncate(&sanitized, 2000)));
    }

    Err(last_err.unwrap_or_else(|| AppError::new("UPSTREAM_NON_STREAM_ERROR", "All retries exhausted")))
}

/// Send a streaming request to Gemini API with retry.
pub async fn send_gemini_stream(
    client: &Client,
    config: &ProviderConfig,
    body: &Value,
    model: &str,
) -> Result<reqwest::Response, AppError> {
    let url = config.gemini_stream_url(model);
    let mut last_err = None;

    for attempt in 0..=MAX_RETRIES {
        let resp = match client.post(&url)
            .header("Authorization", format!("Bearer {}", config.select_api_key()))
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) if is_transient_net_err(&e) && attempt < MAX_RETRIES => {
                eprintln!("[net-retry] {url} attempt {}/{MAX_RETRIES}: {e}", attempt + 1);
                tokio::time::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1))).await;
                last_err = Some(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Transient connect failure attempt {}: {e}", attempt + 1)));
                continue;
            }
            Err(e) => {
                return Err(AppError::new("PROVIDER_REQUEST_FAILED",
                    format!("Failed to connect to Gemini: {e}")));
            }
        };

        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }

        let status_code = status.as_u16();
        let retry_after = parse_retry_after(&resp);
        let body_text = resp.text().await.unwrap_or_default();
        let sanitized = config.sanitize(&body_text);

        if is_retryable(status_code) && attempt < MAX_RETRIES {
            let wait = retry_after.unwrap_or(RETRY_BASE_MS * (1 << attempt) / 1000).max(1);
            eprintln!("[retry] {url} HTTP {status_code}, attempt {}/{MAX_RETRIES}, waiting {wait}s", attempt + 1);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
            last_err = Some(AppError::new("UPSTREAM_STREAM_ERROR", format!("Gemini returned HTTP {status}"))
                .with_detail(truncate(&sanitized, 2000)));
            continue;
        }

        return Err(AppError::new("UPSTREAM_STREAM_ERROR", format!("Gemini returned HTTP {status}"))
            .with_detail(truncate(&sanitized, 2000)));
    }

    Err(last_err.unwrap_or_else(|| AppError::new("UPSTREAM_STREAM_ERROR", "All retries exhausted")))
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

    #[test]
    fn summarize_openresty_502() {
        let body = "<html>\r\n<head><title>502 Bad Gateway</title></head>\r\n<body>\r\n<center><h1>502 Bad Gateway</h1></center>\r\n<hr><center>openresty</center>\r\n</body>\r\n</html>";
        let s = summarize_html_gateway_error(body).expect("should match");
        assert!(s.contains("openresty"));
        assert!(s.contains("502 Bad Gateway"));
        assert!(s.contains("立刻重试"));
    }

    #[test]
    fn summarize_cloudflare_503() {
        let body = "<html><body><h1>503 Service Unavailable</h1>Cloudflare</body></html>";
        let s = summarize_html_gateway_error(body).expect("should match");
        assert!(s.contains("cloudflare"));
        assert!(s.contains("503 Service Unavailable"));
    }

    #[test]
    fn summarize_returns_none_for_json_error() {
        // 普通 OpenAI 风格 JSON 错误体不应被压缩——detail 留全文给 provider 排查。
        let body = r#"{"error":{"code":"insufficient_quota","message":"You exceeded your quota"}}"#;
        assert!(summarize_html_gateway_error(body).is_none());
    }

    #[test]
    fn summarize_handles_mimo_sse_html_wrapper() {
        // MiMo 网关把 HTML 包成 SSE data 帧的真实形态。
        let body = r#"data: {"error":{"code":"500","message":"<html>\r\n<head><title>502 Bad Gateway</title></head>\r\n<body>\r\n<center><h1>502 Bad Gateway</h1></center>\r\n<hr><center>openresty</center>\r\n</body>\r\n</html>\r\n","type":"Internal Server Error"}}"#;
        let s = summarize_html_gateway_error(body).expect("should match");
        assert!(s.contains("openresty"));
        assert!(s.contains("502 Bad Gateway"));
    }

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
            auto_cache_control: None,
            supports_cache: None,
            model_capabilities: None,
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

    // ── Retry logic tests ──

    #[test]
    fn test_is_retryable() {
        assert!(is_retryable(429));
        assert!(is_retryable(500));
        assert!(is_retryable(502));
        assert!(is_retryable(503));
        assert!(!is_retryable(400));
        assert!(!is_retryable(401));
        assert!(!is_retryable(403));
        assert!(!is_retryable(404));
        assert!(!is_retryable(200));
    }

    #[test]
    fn test_retry_constants() {
        assert_eq!(MAX_RETRIES, 2);
        assert_eq!(RETRY_BASE_MS, 1000);
        assert_eq!(RETRYABLE_STATUS.len(), 4);
    }
}
