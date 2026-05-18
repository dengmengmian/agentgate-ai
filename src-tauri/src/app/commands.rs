use tauri::State;
use std::time::Instant;

use crate::app::state::AppState;
use crate::errors::AppError;
use crate::models::gateway::{GatewaySettings, GatewayStatus, UpdateGatewaySettingsInput};
use crate::models::provider::{
    CreateProviderInput, ProviderTestResult, ProviderView, UpdateProviderInput,
};
use crate::models::request_log::{RequestLogDetail, RequestLogFilter, RequestLogListItem};
use crate::models::settings::ToolConfigView;
use crate::storage;
use crate::gateway;

// ── Provider Commands ──────────────────────────────────────────

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> Result<Vec<ProviderView>, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let providers = storage::providers::list_all(&conn)?;
    Ok(providers.into_iter().map(|p| p.to_view()).collect())
}

#[tauri::command]
pub fn get_provider(id: String, state: State<'_, AppState>) -> Result<ProviderView, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::get_by_id(&conn, &id)?;
    Ok(provider.to_view())
}

#[tauri::command]
pub fn create_provider(
    input: CreateProviderInput,
    state: State<'_, AppState>,
) -> Result<ProviderView, AppError> {
    if input.name.trim().is_empty() {
        return Err(AppError::validation("Provider name is required"));
    }
    if input.base_url.trim().is_empty() {
        return Err(AppError::validation("Base URL is required"));
    }
    if input.default_model.trim().is_empty() {
        return Err(AppError::validation("Default model is required"));
    }
    if let Some(t) = input.timeout_seconds {
        if t <= 0 {
            return Err(AppError::validation("Timeout must be greater than 0"));
        }
    }

    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::create(&conn, input)?;

    // Auto-add new provider to all existing route profiles
    let profile_ids: Vec<String> = conn
        .prepare("SELECT id FROM route_profiles")?
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    for pid in &profile_ids {
        let _ = storage::route_profiles::add_provider(
            &conn, pid, &provider.id,
            crate::models::route_profile::AddProviderToRouteInput {
                priority: None, model_override: None,
                cooldown_seconds: None, failover_on_status_codes: None,
                failover_on_error_keywords: None,
            },
        );
    }

    Ok(provider.to_view())
}

#[tauri::command]
pub fn update_provider(
    id: String,
    input: UpdateProviderInput,
    state: State<'_, AppState>,
) -> Result<ProviderView, AppError> {
    if let Some(ref name) = input.name {
        if name.trim().is_empty() {
            return Err(AppError::validation("Provider name cannot be empty"));
        }
    }
    if let Some(ref url) = input.base_url {
        if url.trim().is_empty() {
            return Err(AppError::validation("Base URL cannot be empty"));
        }
    }
    if let Some(ref model) = input.default_model {
        if model.trim().is_empty() {
            return Err(AppError::validation("Default model cannot be empty"));
        }
    }
    if let Some(t) = input.timeout_seconds {
        if t <= 0 {
            return Err(AppError::validation("Timeout must be greater than 0"));
        }
    }

    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::update(&conn, &id, input)?;
    Ok(provider.to_view())
}

#[tauri::command]
pub fn delete_provider(id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::providers::delete(&conn, &id)
}

#[tauri::command]
pub fn set_active_provider(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderView, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::set_active(&conn, &id)?;
    Ok(provider.to_view())
}

#[tauri::command]
pub async fn fetch_provider_models(
    id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let (base_url, api_key, timeout_seconds) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let provider = storage::providers::get_by_id(&conn, &id)?;
        (provider.base_url, provider.api_key, provider.timeout_seconds)
    };

    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => return Err(AppError::new("PROVIDER_API_KEY_MISSING", "API key is not set")),
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_seconds as u64))
        .build()
        .map_err(|e| AppError::internal(format!("HTTP client error: {e}")))?;

    // Try /models then /v1/models
    let base = base_url.trim_end_matches('/');
    let urls = vec![format!("{base}/models"), format!("{base}/v1/models")];

    for url in &urls {
        let resp = client.get(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send().await;

        if let Ok(resp) = resp {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let models: Vec<String> = body.get("data")
                        .and_then(|d| d.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(String::from))
                            .collect())
                        .unwrap_or_default();
                    if !models.is_empty() {
                        // Auto-save to provider
                        let models_json = serde_json::to_string(&models).unwrap_or_default();
                        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock"))?;
                        let _ = conn.execute(
                            "UPDATE providers SET supported_models=?1, updated_at=?2 WHERE id=?3",
                            rusqlite::params![&models_json, chrono::Utc::now().to_rfc3339(), &id],
                        );
                        return Ok(models);
                    }
                }
            }
        }
    }

    Err(AppError::new("PROVIDER_REQUEST_FAILED", "Could not fetch models from provider"))
}

#[tauri::command]
pub async fn test_provider(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderTestResult, AppError> {
    let (base_url, api_key, timeout_seconds) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let provider = storage::providers::get_by_id(&conn, &id)?;
        (provider.base_url, provider.api_key, provider.timeout_seconds)
    };

    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => {
            return Ok(ProviderTestResult {
                success: false,
                status: "failed".to_string(),
                message: "API key is not set. Please configure your API key first.".to_string(),
                latency_ms: None,
                supports_vision: None,
            });
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_seconds as u64))
        .build()
        .map_err(|e| AppError::internal(format!("Failed to create HTTP client: {e}")))?;

    let urls = vec![
        format!("{}/models", base_url.trim_end_matches('/')),
        format!("{}/v1/models", base_url.trim_end_matches('/')),
    ];

    let start = Instant::now();
    let mut last_error = String::new();

    for url in &urls {
        let result = client
            .get(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                let latency = start.elapsed().as_millis() as u64;
                {
                    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
                    storage::providers::update_status(&conn, &id, "connected")?;
                }
                return Ok(ProviderTestResult {
                    success: true,
                    status: "connected".to_string(),
                    message: format!("Connection successful via {url}"),
                    latency_ms: Some(latency),
                    supports_vision: None,
                });
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let sanitized = body.replace(&api_key, "sk-***REDACTED***");
                last_error = format!("HTTP {status}: {sanitized}");
            }
            Err(e) => {
                last_error = format!("Connection error: {e}");
            }
        }
    }

    {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::update_status(&conn, &id, "failed")?;
    }

    let latency = start.elapsed().as_millis() as u64;
    Ok(ProviderTestResult {
        success: false,
        status: "failed".to_string(),
        message: last_error,
        latency_ms: Some(latency),
        supports_vision: None,
    })
}

#[tauri::command]
pub async fn detect_provider_vision(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderTestResult, AppError> {
    let provider = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::get_by_id(&conn, &id)?
    };

    let api_key = match provider.api_key {
        Some(ref k) if !k.is_empty() => k.clone(),
        _ => {
            return Ok(ProviderTestResult {
                success: false,
                status: "failed".to_string(),
                message: "API key is not set".to_string(),
                latency_ms: None,
                supports_vision: None,
            });
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(provider.timeout_seconds as u64))
        .build()
        .map_err(|e| AppError::internal(format!("HTTP client error: {e}")))?;

    // 1x1 red PNG, ~68 bytes base64
    let tiny_image = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==";

    let probe_body = serde_json::json!({
        "model": provider.default_model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "hi"},
                {"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{tiny_image}")}}
            ]
        }],
        "max_tokens": 1
    });

    let base = provider.base_url.trim_end_matches('/');
    let url = if provider.protocol == "anthropic_messages" {
        if let Some(ref abu) = provider.anthropic_base_url {
            format!("{}/v1/messages", abu.trim_end_matches('/'))
        } else {
            format!("{}/v1/messages", base)
        }
    } else {
        format!("{}/v1/chat/completions", base)
    };

    let start = Instant::now();

    let mut req_builder = client.post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json");

    // Add extra headers if configured
    if let Some(ref eh) = provider.extra_headers {
        if let Ok(headers) = serde_json::from_str::<std::collections::HashMap<String, String>>(eh) {
            for (k, v) in headers {
                if let (Ok(name), Ok(val)) = (
                    reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                    reqwest::header::HeaderValue::from_str(&v),
                ) {
                    req_builder = req_builder.header(name, val);
                }
            }
        }
    }

    let result = req_builder.json(&probe_body).send().await;

    let latency = start.elapsed().as_millis() as u64;

    let supports_vision = match result {
        Ok(resp) => {
            let status = resp.status();
            if status.is_success() || status.as_u16() == 200 {
                true
            } else {
                // 400 typically means the model doesn't support images
                // Other errors (401, 403, 5xx) are inconclusive
                status.as_u16() != 400
            }
        }
        Err(_) => {
            // Network error — inconclusive, don't update
            return Ok(ProviderTestResult {
                success: false,
                status: "failed".to_string(),
                message: "Vision detection failed: network error".to_string(),
                latency_ms: Some(latency),
                supports_vision: None,
            });
        }
    };

    // Save result
    {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::update_supports_vision(&conn, &id, supports_vision)?;
    }

    let message = if supports_vision {
        "Vision supported".to_string()
    } else {
        "Vision not supported".to_string()
    };

    Ok(ProviderTestResult {
        success: true,
        status: "detected".to_string(),
        message,
        latency_ms: Some(latency),
        supports_vision: Some(supports_vision),
    })
}

#[tauri::command]
pub async fn detect_provider_cache(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderTestResult, AppError> {
    let provider = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::get_by_id(&conn, &id)?
    };

    // Cache probe only works for providers with anthropic_base_url or anthropic type
    let anthropic_url = if provider.provider_type == "anthropic" || provider.provider_type == "claude" {
        let base = provider.base_url.trim_end_matches('/');
        Some(crate::providers::adapter::smart_append_path(base, "/messages"))
    } else if let Some(ref abu) = provider.anthropic_base_url {
        if !abu.is_empty() {
            Some(crate::providers::adapter::smart_append_path(abu, "/messages"))
        } else {
            None
        }
    } else {
        None
    };

    let url = match anthropic_url {
        Some(u) => u,
        None => {
            return Ok(ProviderTestResult {
                success: false,
                status: "skipped".to_string(),
                message: "Cache probe only works for Anthropic or providers with Anthropic endpoint".to_string(),
                latency_ms: None,
                supports_vision: None,
            });
        }
    };

    let api_key = match provider.api_key {
        Some(ref k) if !k.is_empty() => {
            // Parse multi-key, use first
            let trimmed = k.trim();
            if trimmed.starts_with('[') {
                serde_json::from_str::<Vec<String>>(trimmed)
                    .ok()
                    .and_then(|v| v.into_iter().find(|s| !s.is_empty()))
                    .unwrap_or_else(|| trimmed.to_string())
            } else {
                trimmed.to_string()
            }
        }
        _ => {
            return Ok(ProviderTestResult {
                success: false, status: "failed".to_string(),
                message: "API key is not set".to_string(),
                latency_ms: None, supports_vision: None,
            });
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(provider.timeout_seconds as u64))
        .build()
        .map_err(|e| AppError::internal(format!("HTTP client error: {e}")))?;

    // Build a large enough system prompt (>1024 tokens for cache eligibility)
    let long_system = "You are a helpful assistant. ".repeat(100); // ~600 words, >1024 tokens
    let probe_body = serde_json::json!({
        "model": provider.default_model,
        "system": [{"type": "text", "text": long_system, "cache_control": {"type": "ephemeral"}}],
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1
    });

    let is_anthropic_type = provider.provider_type == "anthropic" || provider.provider_type == "claude";

    let build_req = |client: &reqwest::Client| {
        let mut rb = client.post(&url)
            .header("Content-Type", "application/json");
        if is_anthropic_type {
            rb = rb.header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01");
        } else {
            rb = rb.header("Authorization", format!("Bearer {api_key}"));
        }
        if let Some(ref eh) = provider.extra_headers {
            if let Ok(headers) = serde_json::from_str::<std::collections::HashMap<String, String>>(eh) {
                for (k, v) in headers {
                    if let (Ok(name), Ok(val)) = (
                        reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                        reqwest::header::HeaderValue::from_str(&v),
                    ) {
                        rb = rb.header(name, val);
                    }
                }
            }
        }
        rb.json(&probe_body)
    };

    let start = Instant::now();

    // First request: creates cache
    let resp1 = build_req(&client).send().await;
    if let Err(e) = resp1 {
        return Ok(ProviderTestResult {
            success: false, status: "failed".to_string(),
            message: format!("Cache probe request 1 failed: {e}"),
            latency_ms: Some(start.elapsed().as_millis() as u64), supports_vision: None,
        });
    }
    let resp1 = resp1.unwrap();
    if !resp1.status().is_success() {
        let body = resp1.text().await.unwrap_or_default();
        let sanitized = body.replace(&api_key, "sk-***REDACTED***");
        return Ok(ProviderTestResult {
            success: false, status: "failed".to_string(),
            message: format!("Cache probe request 1 HTTP error: {}", &sanitized[..sanitized.len().min(500)]),
            latency_ms: Some(start.elapsed().as_millis() as u64), supports_vision: None,
        });
    }
    // Consume body
    let _ = resp1.text().await;

    // Second request: should hit cache
    let resp2 = build_req(&client).send().await;
    let latency = start.elapsed().as_millis() as u64;

    let supports_cache = match resp2 {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            // Check for cache_read_input_tokens > 0
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                let cache_read = json.get("usage")
                    .and_then(|u| u.get("cache_read_input_tokens"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                cache_read > 0
            } else {
                false
            }
        }
        _ => false,
    };

    // Save result
    {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::update_supports_cache(&conn, &id, supports_cache)?;
    }

    let message = if supports_cache {
        "Cache supported — cache_read_input_tokens > 0 on second request".to_string()
    } else {
        "Cache not detected — cache_read_input_tokens was 0 on second request".to_string()
    };

    Ok(ProviderTestResult {
        success: true,
        status: "detected".to_string(),
        message,
        latency_ms: Some(latency),
        supports_vision: None, // reuse struct, this field unused here
    })
}

// ── Gateway Commands ───────────────────────────────────────────

#[tauri::command]
pub fn get_gateway_status(state: State<'_, AppState>) -> Result<GatewayStatus, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    let runtime = state
        .gateway_runtime
        .lock()
        .map_err(|_| AppError::internal("Runtime lock failed"))?;

    let active_provider = if let Some(ref pid) = settings.active_provider_id {
        storage::providers::get_by_id(&conn, pid)
            .ok()
            .map(|p| p.name)
    } else {
        None
    };

    Ok(GatewayStatus {
        running: runtime.running,
        host: if runtime.running { runtime.host.clone() } else { settings.host },
        port: if runtime.running { runtime.port as i64 } else { settings.port },
        active_provider,
        input_protocol: settings.input_protocol,
        output_protocol: settings.output_protocol,
        started_at: runtime.started_at.clone(),
    })
}

#[tauri::command]
pub fn get_gateway_settings(state: State<'_, AppState>) -> Result<GatewaySettings, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::gateway_settings::get(&conn)
}

#[tauri::command]
pub fn update_gateway_settings(
    input: UpdateGatewaySettingsInput,
    state: State<'_, AppState>,
) -> Result<GatewaySettings, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::gateway_settings::update(&conn, input)
}

#[tauri::command]
pub async fn start_gateway(state: State<'_, AppState>) -> Result<GatewayStatus, AppError> {
    // Check if already running
    {
        let runtime = state.gateway_runtime.lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        if runtime.running {
            return Err(AppError::new("GATEWAY_ALREADY_RUNNING", "Gateway is already running"));
        }
    }

    // Read settings
    let (host, port) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port as u16)
    };

    // Start real HTTP server
    let (shutdown_tx, server_handle) =
        gateway::server::start(&host, port, state.db.clone()).await?;

    // Update runtime state
    {
        let mut runtime = state.gateway_runtime.lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        runtime.running = true;
        runtime.host = host;
        runtime.port = port;
        runtime.started_at = Some(chrono::Utc::now().to_rfc3339());
        runtime.shutdown_tx = Some(shutdown_tx);
        runtime.server_handle = Some(server_handle);
    }

    get_gateway_status(state)
}

#[tauri::command]
pub async fn stop_gateway(state: State<'_, AppState>) -> Result<GatewayStatus, AppError> {
    let (shutdown_tx, server_handle) = {
        let mut runtime = state.gateway_runtime.lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        if !runtime.running {
            return Err(AppError::new("GATEWAY_NOT_RUNNING", "Gateway is not running"));
        }
        runtime.running = false;
        runtime.started_at = None;
        (runtime.shutdown_tx.take(), runtime.server_handle.take())
    };

    // Send shutdown signal
    if let Some(tx) = shutdown_tx {
        let _ = tx.send(());
    }

    // Wait for server to finish (with timeout)
    if let Some(handle) = server_handle {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
    }

    get_gateway_status(state)
}

#[tauri::command]
pub async fn restart_gateway(state: State<'_, AppState>) -> Result<GatewayStatus, AppError> {
    // Stop if running
    {
        let is_running = {
            let runtime = state.gateway_runtime.lock()
                .map_err(|_| AppError::internal("Runtime lock failed"))?;
            runtime.running
        };
        if is_running {
            let (shutdown_tx, server_handle) = {
                let mut runtime = state.gateway_runtime.lock()
                    .map_err(|_| AppError::internal("Runtime lock failed"))?;
                runtime.running = false;
                runtime.started_at = None;
                (runtime.shutdown_tx.take(), runtime.server_handle.take())
            };
            if let Some(tx) = shutdown_tx {
                let _ = tx.send(());
            }
            if let Some(handle) = server_handle {
                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
            }
        }
    }

    // Small delay for port release
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Start again
    start_gateway(state).await
}

// ── Logs Commands ──────────────────────────────────────────────

#[tauri::command]
pub fn list_request_logs(
    filter: RequestLogFilter,
    state: State<'_, AppState>,
) -> Result<Vec<RequestLogListItem>, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::list(&conn, filter)
}

#[tauri::command]
pub fn get_request_log_detail(
    id: String,
    state: State<'_, AppState>,
) -> Result<RequestLogDetail, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::get_detail(&conn, &id)
}

#[tauri::command]
pub fn clear_request_logs(state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::clear(&conn)
}

// ── Tool Commands ──────────────────────────────────────────────

#[tauri::command]
pub fn list_tools() -> Result<Vec<ToolConfigView>, AppError> {
    let home = dirs_next().unwrap_or_default();

    let tools = vec![
        ToolConfigView {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            slug: "claude-code".to_string(),
            icon: "terminal".to_string(),
            config_path: format!("{}/.claude/settings.json", home),
            description: "Anthropic's CLI for Claude. Agentic coding tool with terminal integration.".to_string(),
            config_exists: std::path::Path::new(&format!("{}/.claude/settings.json", home)).exists(),
        },
        ToolConfigView {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            slug: "codex".to_string(),
            icon: "code".to_string(),
            config_path: format!("{}/.codex/config.toml", home),
            description: "OpenAI's CLI coding agent. Supports OpenAI Responses API and chat completions.".to_string(),
            config_exists: std::path::Path::new(&format!("{}/.codex/config.toml", home)).exists(),
        },
        ToolConfigView {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            slug: "opencode".to_string(),
            icon: "braces".to_string(),
            config_path: format!("{}/.config/opencode/opencode.json", home),
            description: "Open-source terminal AI coding assistant. Supports multiple providers.".to_string(),
            config_exists: std::path::Path::new(&format!("{}/.config/opencode/opencode.json", home)).exists(),
        },
        ToolConfigView {
            id: "atomcode".to_string(),
            name: "AtomCode".to_string(),
            slug: "atomcode".to_string(),
            icon: "atom".to_string(),
            config_path: format!("{}/.atomcode/config.toml", home),
            description: "Open-source AI coding agent in your terminal. Uses OpenAI-compatible API.".to_string(),
            config_exists: std::path::Path::new(&format!("{}/.atomcode/config.toml", home)).exists(),
        },
        ToolConfigView {
            id: "gemini_cli".to_string(),
            name: "Gemini CLI".to_string(),
            slug: "gemini-cli".to_string(),
            icon: "sparkles".to_string(),
            config_path: format!("{}/.gemini/settings.json", home),
            description: "Google's AI coding CLI. Uses Gemini API with OpenAI-compatible endpoint support.".to_string(),
            config_exists: std::path::Path::new(&format!("{}/.gemini/settings.json", home)).exists(),
        },
    ];

    Ok(tools)
}

#[tauri::command]
pub fn generate_codex_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::codex::generate_snippet(&settings.host, settings.port))
}

// ── Route Profile Commands ─────────────────────────────────────

#[tauri::command]
pub fn list_route_profiles(state: State<'_, AppState>) -> Result<Vec<crate::models::route_profile::RouteProfileView>, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::list_all(&conn)
}

#[tauri::command]
pub fn get_route_profile(id: String, state: State<'_, AppState>) -> Result<crate::models::route_profile::RouteProfileDetail, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let profile = storage::route_profiles::get_by_id(&conn, &id)?;
    let providers = storage::route_profiles::list_providers(&conn, &id)?;
    let view = {
        let active_name: Option<String> = profile.active_provider_id.as_ref().and_then(|pid| {
            storage::providers::get_by_id(&conn, pid).ok().map(|p| p.name)
        });
        crate::models::route_profile::RouteProfileView {
            id: profile.id.clone(), name: profile.name,             input_protocol: profile.input_protocol, mode: profile.mode,
            active_provider_id: profile.active_provider_id, active_provider_name: active_name,
            enabled: profile.enabled, is_default: profile.is_default,
            providers_count: providers.len() as i64,
            created_at: profile.created_at, updated_at: profile.updated_at,
        }
    };
    Ok(crate::models::route_profile::RouteProfileDetail { profile: view, providers })
}

#[tauri::command]
pub fn create_route_profile(input: crate::models::route_profile::CreateRouteProfileInput, state: State<'_, AppState>) -> Result<crate::models::route_profile::RouteProfileView, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let profile = storage::route_profiles::create(&conn, input)?;
    Ok(crate::models::route_profile::RouteProfileView {
        id: profile.id, name: profile.name,         input_protocol: profile.input_protocol, mode: profile.mode,
        active_provider_id: profile.active_provider_id, active_provider_name: None,
        enabled: profile.enabled, is_default: profile.is_default, providers_count: 0,
        created_at: profile.created_at, updated_at: profile.updated_at,
    })
}

#[tauri::command]
pub fn update_route_profile(id: String, input: crate::models::route_profile::UpdateRouteProfileInput, state: State<'_, AppState>) -> Result<crate::models::route_profile::RouteProfileView, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let profile = storage::route_profiles::update(&conn, &id, input)?;
    let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM route_profile_providers WHERE route_profile_id=?1", [&id], |r| r.get(0))?;
    Ok(crate::models::route_profile::RouteProfileView {
        id: profile.id, name: profile.name,         input_protocol: profile.input_protocol, mode: profile.mode,
        active_provider_id: profile.active_provider_id, active_provider_name: None,
        enabled: profile.enabled, is_default: profile.is_default, providers_count: cnt,
        created_at: profile.created_at, updated_at: profile.updated_at,
    })
}

#[tauri::command]
pub fn delete_route_profile(id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::delete(&conn, &id)
}

#[tauri::command]
pub fn set_default_route_profile(id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::set_default(&conn, &id)?;
    Ok(true)
}

#[tauri::command]
pub fn set_route_profile_mode(id: String, mode: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::update(&conn, &id, crate::models::route_profile::UpdateRouteProfileInput {
        name: None, mode: Some(mode), enabled: None,
    })?;
    Ok(true)
}

#[tauri::command]
pub fn set_route_active_provider(route_profile_id: String, provider_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::set_active_provider(&conn, &route_profile_id, &provider_id)?;
    Ok(true)
}

#[tauri::command]
pub fn add_provider_to_route(route_profile_id: String, provider_id: String, input: crate::models::route_profile::AddProviderToRouteInput, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::add_provider(&conn, &route_profile_id, &provider_id, input)?;
    Ok(true)
}

#[tauri::command]
pub fn remove_provider_from_route(route_profile_id: String, provider_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::remove_provider(&conn, &route_profile_id, &provider_id)?;
    Ok(true)
}

#[tauri::command]
pub fn reorder_route_providers(route_profile_id: String, provider_ids: Vec<String>, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::reorder_providers(&conn, &route_profile_id, &provider_ids)?;
    Ok(true)
}

#[tauri::command]
pub fn list_provider_runtime_status(state: State<'_, AppState>) -> Result<Vec<crate::models::route_profile::ProviderRuntimeStatus>, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::provider_runtime_status::list_all(&conn)
}

#[tauri::command]
pub fn reset_provider_runtime_status(provider_id: String, state: State<'_, AppState>) -> Result<crate::models::route_profile::ProviderRuntimeStatus, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::provider_runtime_status::reset(&conn, &provider_id)
}

#[tauri::command]
pub fn reset_all_provider_runtime_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::provider_runtime_status::reset_all(&conn)?;
    Ok(true)
}

// ── Gateway Auth Commands ──────────────────────────────────────

#[tauri::command]
pub fn get_gateway_auth_settings() -> Result<crate::security::local_token::GatewayAuthSettings, AppError> {
    Ok(crate::security::local_token::get_auth_settings())
}

#[tauri::command]
pub fn regenerate_local_access_token() -> Result<crate::security::local_token::GatewayAuthSettings, AppError> {
    crate::security::local_token::regenerate_token()?;
    Ok(crate::security::local_token::get_auth_settings())
}

#[tauri::command]
pub fn ensure_local_access_token() -> Result<crate::security::local_token::GatewayAuthSettings, AppError> {
    crate::security::local_token::ensure_token()?;
    Ok(crate::security::local_token::get_auth_settings())
}

#[tauri::command]
pub fn get_local_access_token() -> Result<String, AppError> {
    crate::security::local_token::read_token()
}

#[tauri::command]
pub fn open_token_folder() -> Result<bool, AppError> {
    let dir = crate::security::local_token::token_dir();
    open::that(&dir).map_err(|e| AppError::internal(format!("Failed to open folder: {e}")))?;
    Ok(true)
}

// ── Codex Config Commands ──────────────────────────────────────

#[tauri::command]
pub fn detect_codex_config() -> Result<crate::tools::codex::CodexConfigStatus, AppError> {
    Ok(crate::tools::codex::detect())
}

#[tauri::command]
pub fn apply_codex_config(state: State<'_, AppState>) -> Result<crate::tools::codex::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };

    crate::tools::codex::apply(&host, port)
}

#[tauri::command]
pub fn toggle_codex_provider(state: State<'_, AppState>) -> Result<crate::tools::codex::ToggleResult, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    crate::tools::codex::toggle_provider(&settings.host, settings.port)
}

#[tauri::command]
pub fn open_codex_config() -> Result<bool, AppError> {
    crate::tools::codex::open_config()?;
    Ok(true)
}

// ── Claude Code Commands ──────────────────────────────────────

#[tauri::command]
pub fn detect_claude_code_env() -> Result<crate::tools::claude_code::ClaudeCodeEnvStatus, AppError> {
    Ok(crate::tools::claude_code::detect_env())
}

#[tauri::command]
pub fn apply_claude_code_config(state: State<'_, AppState>) -> Result<crate::tools::claude_code::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    crate::tools::claude_code::apply_config(&host, port, "claude-sonnet-4-6")
}

#[tauri::command]
pub fn toggle_claude_code_provider(state: State<'_, AppState>) -> Result<crate::tools::claude_code::ToggleResult, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    crate::tools::claude_code::toggle_provider(&settings.host, settings.port, "claude-sonnet-4-6")
}

#[tauri::command]
pub fn open_claude_code_config() -> Result<bool, AppError> {
    crate::tools::claude_code::open_config()?;
    Ok(true)
}

#[tauri::command]
pub fn generate_claude_code_env(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::claude_code::generate_env_snippet(&settings.host, settings.port, "claude-sonnet-4-6"))
}

// ── OpenCode Commands ─────────────────────────────────────────

#[tauri::command]
pub fn detect_opencode_config() -> Result<crate::tools::opencode::OpenCodeConfigStatus, AppError> {
    Ok(crate::tools::opencode::detect())
}

#[tauri::command]
pub fn apply_opencode_config(state: State<'_, AppState>) -> Result<crate::tools::opencode::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    crate::tools::opencode::apply(&host, port)
}

#[tauri::command]
pub fn generate_opencode_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::opencode::generate_snippet(&settings.host, settings.port))
}

#[tauri::command]
pub fn open_opencode_config() -> Result<bool, AppError> {
    crate::tools::opencode::open_config()?;
    Ok(true)
}

// ── Gemini CLI Config Commands ─────────────────────────────────

#[tauri::command]
pub fn detect_gemini_config() -> Result<crate::tools::gemini_cli::GeminiCliConfigStatus, AppError> {
    Ok(crate::tools::gemini_cli::detect())
}

#[tauri::command]
pub fn apply_gemini_config(state: State<'_, AppState>) -> Result<crate::tools::gemini_cli::ApplyConfigResult, AppError> {
    let (host, port, model) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider.map(|p| p.default_model).unwrap_or_else(|| "gemini-2.5-flash".to_string());
        (settings.host, settings.port, model)
    };
    crate::tools::gemini_cli::apply(&host, port, &model)
}

#[tauri::command]
pub fn generate_gemini_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::gemini_cli::generate_snippet(&settings.host, settings.port, "gemini-2.5-flash"))
}

#[tauri::command]
pub fn toggle_gemini_provider(state: State<'_, AppState>) -> Result<crate::tools::gemini_cli::ToggleResult, AppError> {
    let (host, port, model) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider.map(|p| p.default_model).unwrap_or_else(|| "gemini-2.5-flash".to_string());
        (settings.host, settings.port, model)
    };
    crate::tools::gemini_cli::toggle(&host, port, &model)
}

#[tauri::command]
pub fn open_gemini_config() -> Result<bool, AppError> {
    crate::tools::gemini_cli::open_config()?;
    Ok(true)
}

// ── AtomCode Config Commands ──────────────────────────────────

#[tauri::command]
pub fn detect_atomcode_config() -> Result<crate::tools::atomcode::AtomCodeConfigStatus, AppError> {
    Ok(crate::tools::atomcode::detect())
}

#[tauri::command]
pub fn apply_atomcode_config(state: State<'_, AppState>) -> Result<crate::tools::atomcode::ApplyConfigResult, AppError> {
    let (host, port, model) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider.map(|p| p.default_model).unwrap_or_else(|| "deepseek-chat".to_string());
        (settings.host, settings.port, model)
    };
    crate::tools::atomcode::apply(&host, port, &model)
}

#[tauri::command]
pub fn generate_atomcode_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::atomcode::generate_snippet(&settings.host, settings.port, "deepseek-chat"))
}

#[tauri::command]
pub fn toggle_atomcode_provider(state: State<'_, AppState>) -> Result<crate::tools::atomcode::ToggleResult, AppError> {
    let (host, port, model) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider.map(|p| p.default_model).unwrap_or_else(|| "deepseek-chat".to_string());
        (settings.host, settings.port, model)
    };
    crate::tools::atomcode::toggle(&host, port, &model)
}

#[tauri::command]
pub fn open_atomcode_config() -> Result<bool, AppError> {
    crate::tools::atomcode::open_config()?;
    Ok(true)
}

// ── Provider Health Commands ──────────────────────────────────

#[tauri::command]
pub fn get_provider_health(state: State<'_, AppState>, provider: String) -> Result<crate::storage::request_logs::ProviderHealth, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::request_logs::get_provider_health(&conn, &provider)
}

// ── Pricing Commands ──────────────────────────────────────────

#[tauri::command]
pub fn list_model_pricing(state: State<'_, AppState>) -> Result<Vec<crate::storage::pricing::ModelPricing>, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::list_all(&conn)
}

#[tauri::command]
pub fn upsert_model_pricing(state: State<'_, AppState>, provider: String, model_pattern: String, input_price: f64, output_price: f64) -> Result<crate::storage::pricing::ModelPricing, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::upsert_custom(&conn, &provider, &model_pattern, input_price, output_price)
}

#[tauri::command]
pub fn delete_model_pricing(state: State<'_, AppState>, id: String) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::delete_custom(&conn, &id)
}

// ── Stats Commands ─────────────────────────────────────────────

#[tauri::command]
pub fn get_request_stats(state: State<'_, AppState>) -> Result<crate::storage::request_logs::RequestStats, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::get_stats(&conn)
}

// ── Diagnostics Commands ───────────────────────────────────────

#[tauri::command]
pub fn run_health_check(state: State<'_, AppState>) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::health_check(&state.db))
}

#[tauri::command]
pub fn run_database_check(state: State<'_, AppState>) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::database_check(&state.db))
}

#[tauri::command]
pub fn run_gateway_auth_check(state: State<'_, AppState>) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::gateway_auth_check(&state.db))
}

#[tauri::command]
pub fn run_provider_check(state: State<'_, AppState>) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::provider_check(&state.db))
}

#[tauri::command]
pub fn run_codex_config_check(state: State<'_, AppState>) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::codex_config_check(&state.db))
}

#[tauri::command]
pub fn run_claude_code_config_check(state: State<'_, AppState>) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::claude_code_config_check(&state.db))
}

#[tauri::command]
pub fn run_route_profile_check(state: State<'_, AppState>) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::route_profile_check(&state.db))
}

#[tauri::command]
pub fn run_full_self_test(state: State<'_, AppState>) -> Result<crate::diagnostics::report::FullSelfTestReport, AppError> {
    Ok(crate::diagnostics::checks::full_self_test(&state.db))
}

#[tauri::command]
pub fn export_diagnostic_bundle(
    include_logs: Option<bool>,
    max_logs: Option<u32>,
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::ExportResult, AppError> {
    crate::diagnostics::checks::export_bundle(
        &state.db,
        include_logs.unwrap_or(true),
        max_logs.unwrap_or(50) as usize,
    )
}

#[tauri::command]
pub fn open_app_data_dir() -> Result<bool, AppError> {
    let dir = crate::security::local_token::token_dir();
    open::that(&dir).map_err(|e| AppError::internal(format!("Cannot open: {e}")))?;
    Ok(true)
}

fn dirs_next() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok()
    }
}
