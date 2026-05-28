use tauri::{Emitter, Manager, State};
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

/// Auto-derive capabilities for a list of model IDs given a provider type.
/// Used by the "Auto-detect" button in the capability matrix editor to fill
/// in sensible defaults without forcing the user to tick every box.
#[tauri::command]
pub fn seed_model_capabilities(
    provider_type: String,
    model_ids: Vec<String>,
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    Ok(crate::providers::capabilities::seed_for_models(&provider_type, &model_ids))
}

/// Seed-fill the model_capabilities matrix for a provider and persist it.
/// Only fills models that are missing from the existing matrix — never
/// overwrites manual edits. Used by the "测试" button after connectivity
/// succeeds, so newly added models pick up sensible defaults without the
/// user needing to open the form dialog.
#[tauri::command]
pub fn autofill_provider_capabilities(
    id: String,
    state: State<'_, AppState>,
) -> Result<usize, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::get_by_id(&conn, &id)?;

    // Existing matrix — preserve user edits.
    let mut matrix: std::collections::HashMap<String, Vec<String>> = provider
        .model_capabilities.as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    // Target models: supported_models ∪ default_model ∪ reasoning_model.
    let mut targets: std::collections::BTreeSet<String> = Default::default();
    if let Some(ref sm) = provider.supported_models {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(sm) {
            for m in list { targets.insert(m); }
        }
    }
    if !provider.default_model.is_empty() { targets.insert(provider.default_model.clone()); }
    if let Some(ref rm) = provider.reasoning_model {
        if !rm.is_empty() { targets.insert(rm.clone()); }
    }

    let mut filled = 0usize;
    for model in targets {
        if !matrix.contains_key(&model) {
            let caps = crate::providers::capabilities::seed_for_model(&provider.provider_type, &model);
            if !caps.is_empty() {
                matrix.insert(model, caps);
                filled += 1;
            }
        }
    }

    if filled > 0 {
        let json = serde_json::to_string(&matrix)
            .map_err(|e| AppError::internal(format!("serialize matrix: {e}")))?;
        storage::providers::update_model_capabilities(&conn, &id, &json)?;
    }

    Ok(filled)
}

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
    mut input: CreateProviderInput,
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
    storage::recommended_mappings::apply_to_create_input(&mut input);

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
                failover_on_error_keywords: None, routing_conditions: None,
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
    app_handle: tauri::AppHandle,
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderView, AppError> {
    let provider = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::set_active(&conn, &id)?
    };
    crate::app::tray::refresh_tray(&app_handle);
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
                        let _ = storage::recommended_mappings::supplement_provider(
                            &conn,
                            &id,
                            storage::recommended_mappings::MappingProfile::All,
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
                    let _ = storage::recommended_mappings::supplement_provider(
                        &conn,
                        &id,
                        storage::recommended_mappings::MappingProfile::All,
                    );
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

    // Vision detection by status code:
    //   2xx                → supported (model accepted image input)
    //   401 / 403          → inconclusive (auth issue, can't tell capability)
    //   5xx, network error → inconclusive (server-side issue)
    //   any other 4xx      → NOT supported (model rejected image input)
    //
    // The "any 4xx" rule is critical: providers reject image input with
    // different status codes — OpenAI/DeepSeek use 400 ("invalid image"),
    // MiMo's pro/flash variants use 404 ("No endpoints found that support
    // image input"), some return 422. Treating only 400 as "not supported"
    // (the previous logic) false-positived MiMo's 404 as supports_vision.
    let supports_vision: Option<bool> = match result {
        Ok(resp) => {
            let code = resp.status().as_u16();
            if resp.status().is_success() {
                Some(true)
            } else if code == 401 || code == 403 || code >= 500 {
                None
            } else {
                Some(false)
            }
        }
        Err(_) => None,
    };

    // Persist only when conclusive — keep stale state out of the DB on
    // auth/server errors so the user retries with a fresh signal.
    if let Some(v) = supports_vision {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::update_supports_vision(&conn, &id, v)?;
    }

    let (status_str, message) = match supports_vision {
        Some(true) => ("detected", "Vision supported".to_string()),
        Some(false) => ("detected", "Vision not supported".to_string()),
        None => ("inconclusive", "Vision detection inconclusive (auth / server error / network)".to_string()),
    };

    Ok(ProviderTestResult {
        success: supports_vision.is_some(),
        status: status_str.to_string(),
        message,
        latency_ms: Some(latency),
        supports_vision,
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

    // Cache 探测每次跑 2 次 HTTP，timeout 硬上限 15s：
    // 上游若不支持 Anthropic 格式（如错配了 anthropic_base_url 的 OpenAI 系 provider），
    // 否则会卡满 provider 默认 timeout（往往 120s+），前端 dialog 永远在转。
    let probe_timeout = std::cmp::min(provider.timeout_seconds as u64, 15);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(probe_timeout))
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
pub async fn start_gateway(app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<GatewayStatus, AppError> {
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
    let (shutdown_tx, server_handle, active_requests, _bound_port) =
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
        runtime.active_requests = Some(active_requests);
    }

    let _ = app_handle.emit("pet-bubble", serde_json::json!({ "text": "Gateway started", "text_zh": "网关已启动", "type": "success" }));
    crate::app::tray::refresh_tray(&app_handle);
    get_gateway_status(state)
}

#[tauri::command]
pub async fn stop_gateway(app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<GatewayStatus, AppError> {
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

    let _ = app_handle.emit("pet-bubble", serde_json::json!({ "text": "Gateway stopped", "text_zh": "网关已停止", "type": "info" }));
    crate::app::tray::refresh_tray(&app_handle);
    get_gateway_status(state)
}

#[tauri::command]
pub async fn restart_gateway(app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<GatewayStatus, AppError> {
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
    start_gateway(app_handle, state).await
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
pub fn count_request_logs(
    filter: RequestLogFilter,
    state: State<'_, AppState>,
) -> Result<i64, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::count(&conn, &filter)
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
    let token = crate::security::local_token::ensure_token()?;
    Ok(crate::tools::codex::generate_snippet(&settings.host, settings.port, &token))
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
pub fn update_route_provider_conditions(
    route_profile_id: String,
    provider_id: String,
    routing_conditions: Option<String>,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::update_provider_conditions(&conn, &route_profile_id, &provider_id, routing_conditions.as_deref())?;
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
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::Codex,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };

    crate::tools::codex::apply(&host, port)
}

#[tauri::command]
pub fn toggle_codex_provider(state: State<'_, AppState>) -> Result<crate::tools::codex::ToggleResult, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let _ = storage::recommended_mappings::supplement_active_provider(
        &conn,
        storage::recommended_mappings::MappingProfile::Codex,
    );
    let settings = storage::gateway_settings::get(&conn)?;
    crate::tools::codex::toggle_provider(&settings.host, settings.port)
}

/// Restore Codex to its pre-AgentGate state — the saved config.toml is
/// copied back so the user gets the official `[plugins.*]` / `[mcp_servers.*]`
/// blocks alive again. Used by the UI's "Switch to native mode" button.
#[tauri::command]
pub fn disable_codex_agentgate() -> Result<crate::tools::codex::ApplyConfigResult, AppError> {
    crate::tools::codex::disable()
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
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::ClaudeCode,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    crate::tools::claude_code::apply_config(&host, port, "claude-sonnet-4-6")
}

#[tauri::command]
pub fn toggle_claude_code_provider(state: State<'_, AppState>) -> Result<crate::tools::claude_code::ToggleResult, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let _ = storage::recommended_mappings::supplement_active_provider(
        &conn,
        storage::recommended_mappings::MappingProfile::ClaudeCode,
    );
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

/// Stats over a configurable window (in days). Dashboard date-range tabs
/// (今天/7天/14天/30天) call this with 1/7/14/30 respectively.
#[tauri::command]
pub fn get_request_stats_range(
    days: i64,
    state: State<'_, AppState>,
) -> Result<crate::storage::request_logs::RequestStats, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::get_stats_for_range(&conn, days)
}

/// Live runtime KPIs surfaced in the bottom footer of Dashboard / Routes.
/// Combines runtime-only state (active_requests, uptime) with lifetime
/// aggregate metrics that used to live in a separate "累计" strip. Today
/// stats are intentionally NOT included here — the Dashboard's "今日"
/// strip already covers them, the footer focuses on the long-running view.
#[derive(serde::Serialize)]
pub struct RuntimeKpis {
    /// Currently in-flight requests at the proxy layer.
    pub active_requests: u64,
    /// Seconds since the gateway was started; 0 when stopped.
    pub uptime_seconds: i64,
    pub gateway_running: bool,
    pub gateway_port: u16,
    /// Lifetime totals — folded in from the old "累计" strip so the footer
    /// is the single source of truth for "long-running scoreboard" info.
    pub total_requests: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub success_rate_lifetime: f64,
}

#[tauri::command]
pub fn get_runtime_kpis(state: State<'_, AppState>) -> Result<RuntimeKpis, AppError> {
    let runtime = state.gateway_runtime.lock()
        .map_err(|_| AppError::internal("Runtime lock failed"))?;
    let active_requests = runtime.active_requests.as_ref()
        .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    let gateway_running = runtime.running;
    let gateway_port = runtime.port;
    let uptime_seconds = runtime.started_at.as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|started| (chrono::Utc::now() - started.with_timezone(&chrono::Utc)).num_seconds())
        .unwrap_or(0);
    drop(runtime);

    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let stats = storage::request_logs::get_stats(&conn)?;
    Ok(RuntimeKpis {
        active_requests,
        uptime_seconds,
        gateway_running,
        gateway_port,
        total_requests: stats.total,
        total_tokens: stats.total_input_tokens + stats.total_output_tokens,
        total_cost: stats.total_cost,
        success_rate_lifetime: stats.success_rate,
    })
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

// ── Tool Connection Test ──────────────────────────────────────

#[tauri::command]
pub async fn test_tool_connection(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    // Step 1: Check gateway is running
    let (running, host, port) = {
        let runtime = state.gateway_runtime.lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        (runtime.running, runtime.host.clone(), runtime.port)
    };

    if !running {
        return Ok(serde_json::json!({
            "config_ok": true,
            "gateway_ok": false,
            "provider_ok": false,
            "error": "Gateway not running",
        }));
    }

    // Step 2: Check gateway health
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::internal(format!("HTTP client error: {e}")))?;

    let health_url = format!("http://{}:{}/health", host, port);
    let gateway_ok = client.get(&health_url).send().await
        .map(|r| r.status().is_success())
        .unwrap_or(false);

    if !gateway_ok {
        return Ok(serde_json::json!({
            "config_ok": true,
            "gateway_ok": false,
            "provider_ok": false,
            "error": "Gateway health check failed",
        }));
    }

    // Step 3: Test provider with a minimal request
    let test_model = {
        let conn = state.db.lock()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let route_model = storage::route_profiles::get_default_for_protocol(&conn, "openai_chat_completions")?
            .and_then(|profile| profile.active_provider_id)
            .and_then(|provider_id| storage::providers::get_by_id(&conn, &provider_id).ok())
            .map(|provider| provider.default_model);
        route_model
            .or_else(|| {
                storage::providers::list_all(&conn).ok()
                    .and_then(|providers| providers.into_iter()
                        .find(|provider| provider.enabled && provider.is_active)
                        .map(|provider| provider.default_model))
            })
            .unwrap_or_else(|| "test".to_string())
    };
    let token = crate::security::local_token::read_token().unwrap_or_default();
    let test_url = format!("http://{}:{}/v1/chat/completions", host, port);
    let test_body = serde_json::json!({
        "model": test_model,
        "messages": [{"role": "user", "content": "hi"}],
        "max_tokens": 1,
    });

    let resp = client.post(&test_url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&test_body)
        .send()
        .await;

    let (provider_ok, error) = match resp {
        Ok(r) => {
            let status = r.status();
            if status.is_success() || status.as_u16() == 200 {
                (true, None)
            } else {
                let body = r.text().await.unwrap_or_default();
                // 400 with model error is ok — means provider is reachable
                if status.as_u16() == 400 || status.as_u16() == 404 {
                    (true, None)
                } else {
                    (false, Some(format!("Provider error: {} {}", status.as_u16(), body.chars().take(100).collect::<String>())))
                }
            }
        }
        Err(e) => (false, Some(format!("Request failed: {e}"))),
    };

    Ok(serde_json::json!({
        "config_ok": true,
        "gateway_ok": true,
        "provider_ok": provider_ok,
        "test_model": test_body.get("model").and_then(|v| v.as_str()).unwrap_or("test"),
        "error": error,
    }))
}

// ── Pet Commands ──────────────────────────────────────────────

#[tauri::command]
pub fn get_pet_settings(state: State<'_, AppState>) -> Result<crate::models::pet::PetSettings, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::pet_settings::get(&conn)
}

#[tauri::command]
pub fn update_pet_settings(
    input: crate::models::pet::UpdatePetSettingsInput,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::models::pet::PetSettings, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let result = storage::pet_settings::update(&conn, input)?;
    let _ = app_handle.emit("pet-settings-changed", &result);
    Ok(result)
}

#[tauri::command]
pub fn set_pet_visible(visible: bool, app_handle: tauri::AppHandle, state: State<'_, AppState>) -> Result<crate::models::pet::PetSettings, AppError> {
    if let Some(pet_win) = app_handle.get_webview_window("pet") {
        if visible {
            crate::move_pet_to_visible_area(&app_handle, &pet_win);
            let _ = pet_win.show();
            let _ = pet_win.set_focus();
        } else {
            let _ = pet_win.hide();
        }
    }
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::pet_settings::update(&conn, crate::models::pet::UpdatePetSettingsInput {
        pet_type: None,
        visible: Some(visible),
        pos_x: None,
        pos_y: None,
    })
}

#[tauri::command]
pub fn get_pet_gateway_state(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    let runtime = state.gateway_runtime.lock()
        .map_err(|_| AppError::internal("Runtime lock failed"))?;

    let gw_state = if !runtime.running {
        "stopped"
    } else if runtime.active_requests.as_ref()
        .map(|c| c.load(std::sync::atomic::Ordering::Relaxed) > 0)
        .unwrap_or(false)
    {
        "active"
    } else {
        "running"
    };

    // Query recent errors (last 5 seconds)
    let last_error = if runtime.running {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        conn.query_row(
            "SELECT error_message, provider, timestamp FROM request_logs
             WHERE error_message IS NOT NULL AND error_message != ''
             ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| {
                let msg: String = row.get(0)?;
                let provider: Option<String> = row.get(1)?;
                let ts: String = row.get(2)?;
                Ok(serde_json::json!({ "message": msg, "provider": provider, "timestamp": ts }))
            },
        ).ok()
    } else {
        None
    };

    // Today's stats for pet bubble
    let today_stats = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM request_logs WHERE timestamp >= ?1",
            [&today], |row| row.get(0),
        ).unwrap_or(0);
        let cost: f64 = conn.query_row(
            "SELECT COALESCE(SUM(cost), 0) FROM request_logs WHERE timestamp >= ?1",
            [&today], |row| row.get(0),
        ).unwrap_or(0.0);
        serde_json::json!({ "requests": count, "cost": cost })
    };

    Ok(serde_json::json!({
        "state": gw_state,
        "last_error": last_error,
        "today": today_stats,
    }))
}

// ── Pet Chat Commands ─────────────────────────────────────────

#[tauri::command]
pub fn get_pet_memory(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    Ok(storage::app_settings::get(&conn, "pet_memory")?.unwrap_or_else(|| "{}".to_string()))
}

#[tauri::command]
pub fn save_pet_memory(memory: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::app_settings::set(&conn, "pet_memory", &memory)?;
    Ok(true)
}

#[tauri::command]
pub async fn pet_chat(
    messages: Vec<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    // Find the active provider
    let (base_url, api_key, model, timeout) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id
            .ok_or_else(|| AppError::new("NO_ACTIVE_PROVIDER", "No active provider configured"))?;
        let provider = storage::providers::get_by_id(&conn, &provider_id)?;
        let api_key = provider.api_key
            .ok_or_else(|| AppError::new("NO_API_KEY", "Active provider has no API key"))?;
        // Parse multi-key, use first
        let key = if api_key.trim().starts_with('[') {
            serde_json::from_str::<Vec<String>>(api_key.trim())
                .ok()
                .and_then(|v| v.into_iter().find(|s| !s.is_empty()))
                .unwrap_or(api_key)
        } else {
            api_key
        };
        (provider.base_url, key, provider.default_model, provider.timeout_seconds)
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout as u64))
        .build()
        .map_err(|e| AppError::internal(format!("HTTP client error: {e}")))?;

    let base = base_url.trim_end_matches('/');
    let url = format!("{base}/v1/chat/completions");

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "max_tokens": 200,
        "temperature": 0.8,
    });

    let resp = client.post(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::internal(format!("Request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(AppError::new("CHAT_API_ERROR", format!("API error {status}"))
            .with_detail(text));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| AppError::internal(format!("Parse error: {e}")))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("...")
        .to_string();

    Ok(content)
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

// ── Config Import / Export ────────────────────────────────────

/// 导出当前配置为 JSON 字符串。前端拿到后用 Tauri dialog 保存到磁盘。
///
/// `include_secrets = false`（默认）会把 api_key 字段全部置空——导出文件可以
/// 安全分享/截图；用户在新机器导入后重新填密钥即可。`include_secrets = true`
/// 会把明文密钥写入文件，仅用于自己换机迁移这种场景，前端需要明确警告。
#[tauri::command]
pub fn export_config_json(
    include_secrets: bool,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let dump = storage::config_backups::export(&conn, include_secrets)?;
    serde_json::to_string_pretty(&dump)
        .map_err(|e| AppError::internal(format!("Serialize export: {e}")))
}

/// 从前端拿到的 JSON 字符串还原配置。**replace 语义**：providers / route_profiles
/// / route_profile_providers 三张表会被先清空再重建。运行时状态（provider_runtime_status）
/// 一并清空（指向已不存在的 provider_id 没意义）；request_logs / pricing 等
/// 历史数据不受影响。
#[tauri::command]
pub fn import_config_json(
    json: String,
    state: State<'_, AppState>,
) -> Result<storage::config_backups::ImportSummary, AppError> {
    let payload: storage::config_backups::ConfigExport = serde_json::from_str(&json)
        .map_err(|e| {
            AppError::new("CONFIG_IMPORT_PARSE_ERROR", format!("Invalid config JSON: {e}"))
                .with_suggestion("Make sure the file is an AgentGate config export, not a different JSON file.")
        })?;
    let mut conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::config_backups::import(&mut conn, &payload)
}
