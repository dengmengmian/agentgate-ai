use std::time::Instant;
use tauri::{Emitter, Manager, State};

use crate::app::state::AppState;
use crate::errors::AppError;
use crate::gateway;
use crate::models::gateway::{GatewaySettings, GatewayStatus, UpdateGatewaySettingsInput};
use crate::models::provider::{
    CreateProviderInput, ProviderTestResult, ProviderView, UpdateProviderInput,
};
use crate::models::request_log::{RequestLogDetail, RequestLogFilter, RequestLogListItem};
use crate::models::settings::ToolConfigView;
use crate::storage;

// ── Provider Commands ──────────────────────────────────────────

/// Auto-derive capabilities for a list of model IDs given a provider type.
/// Used by the "Auto-detect" button in the capability matrix editor to fill
/// in sensible defaults without forcing the user to tick every box.
#[tauri::command]
pub fn seed_model_capabilities(
    provider_type: String,
    model_ids: Vec<String>,
) -> Result<std::collections::HashMap<String, Vec<String>>, AppError> {
    Ok(crate::providers::capabilities::seed_for_models(
        &provider_type,
        &model_ids,
    ))
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
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::get_by_id(&conn, &id)?;

    // Existing matrix — preserve user edits.
    let mut matrix: std::collections::HashMap<String, Vec<String>> = provider
        .model_capabilities
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    // Target models: supported_models ∪ default_model ∪ reasoning_model.
    let mut targets: std::collections::BTreeSet<String> = Default::default();
    if let Some(ref sm) = provider.supported_models {
        if let Ok(list) = serde_json::from_str::<Vec<String>>(sm) {
            for m in list {
                targets.insert(m);
            }
        }
    }
    if !provider.default_model.is_empty() {
        targets.insert(provider.default_model.clone());
    }
    if let Some(ref rm) = provider.reasoning_model {
        if !rm.is_empty() {
            targets.insert(rm.clone());
        }
    }

    let mut filled = 0usize;
    for model in targets {
        if !matrix.contains_key(&model) {
            let caps =
                crate::providers::capabilities::seed_for_model(&provider.provider_type, &model);
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
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let providers = storage::providers::list_all(&conn)?;
    Ok(providers.into_iter().map(|p| p.to_view()).collect())
}

#[tauri::command]
pub fn get_provider(id: String, state: State<'_, AppState>) -> Result<ProviderView, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::get_by_id(&conn, &id)?;
    Ok(provider.to_view())
}

/// Return the plain-text api keys for a provider, in storage order.
///
/// Used by the edit form to repopulate every key slot so users can see
/// which key is which (the masked view alone hides that). Calling code
/// must keep the keys in memory only as long as the dialog is open;
/// they're not redacted in any subsequent log path.
#[tauri::command]
pub fn get_provider_keys(id: String, state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::get_by_id(&conn, &id)?;
    let raw = match provider.api_key {
        Some(k) if !k.trim().is_empty() => k,
        _ => return Ok(Vec::new()),
    };
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        if let Ok(keys) = serde_json::from_str::<Vec<String>>(trimmed) {
            return Ok(keys.into_iter().filter(|k| !k.trim().is_empty()).collect());
        }
    }
    Ok(vec![trimmed.to_string()])
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

    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::create(&conn, input)?;

    // Auto-add new provider to all existing route profiles
    let profile_ids: Vec<String> = conn
        .prepare("SELECT id FROM route_profiles")?
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    for pid in &profile_ids {
        let _ = storage::route_profiles::add_provider(
            &conn,
            pid,
            &provider.id,
            crate::models::route_profile::AddProviderToRouteInput {
                priority: None,
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
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

    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let provider = storage::providers::update(&conn, &id, input)?;
    Ok(provider.to_view())
}

#[tauri::command]
pub fn delete_provider(id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::providers::delete(&conn, &id)
}

#[tauri::command]
pub fn set_active_provider(
    app_handle: tauri::AppHandle,
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderView, AppError> {
    let provider = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
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
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let provider = storage::providers::get_by_id(&conn, &id)?;
        (
            provider.base_url,
            provider.api_key,
            provider.timeout_seconds,
        )
    };

    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => {
            return Err(AppError::new(
                "PROVIDER_API_KEY_MISSING",
                "API key is not set",
            ))
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_seconds as u64))
        .build()
        .map_err(|e| AppError::internal(format!("HTTP client error: {e}")))?;

    // Try /models then /v1/models
    let base = base_url.trim_end_matches('/');
    let urls = vec![format!("{base}/models"), format!("{base}/v1/models")];

    for url in &urls {
        let resp = client
            .get(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .send()
            .await;

        if let Ok(resp) = resp {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let models: Vec<String> = body
                        .get("data")
                        .and_then(|d| d.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|m| {
                                    m.get("id").and_then(|id| id.as_str()).map(String::from)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    if !models.is_empty() {
                        // Auto-save to provider
                        let models_json = serde_json::to_string(&models).unwrap_or_default();
                        let conn = state.db.get().map_err(|_| AppError::internal("DB lock"))?;
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

    Err(AppError::new(
        "PROVIDER_REQUEST_FAILED",
        "Could not fetch models from provider",
    ))
}

/// Speedtest a single provider — sends a 1-token probe request and reports
/// connect / TTFB / total latency. User-triggered only (never automatic) to
/// avoid burning tokens.
#[tauri::command]
pub async fn provider_speedtest(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::speedtest::ProviderSpeedReport, AppError> {
    let provider = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::get_by_id(&conn, &id)?
    };
    Ok(crate::diagnostics::speedtest::probe(&provider).await)
}

/// Speedtest every enabled provider in parallel. Heavier than single-provider
/// probe — confirm the user wants this in UI before calling.
#[tauri::command]
pub async fn provider_speedtest_all(
    state: State<'_, AppState>,
) -> Result<Vec<crate::diagnostics::speedtest::ProviderSpeedReport>, AppError> {
    let providers = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::list_all(&conn)?
            .into_iter()
            .filter(|p| p.enabled)
            .collect::<Vec<_>>()
    };
    crate::diagnostics::speedtest::probe_many(&providers).await
}

#[tauri::command]
pub async fn test_provider(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderTestResult, AppError> {
    let (base_url, api_key, timeout_seconds) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let provider = storage::providers::get_by_id(&conn, &id)?;
        (
            provider.base_url,
            provider.api_key,
            provider.timeout_seconds,
        )
    };

    let (provider_type, extra_headers) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let p = storage::providers::get_by_id(&conn, &id)?;
        (p.provider_type, p.extra_headers)
    };

    let api_key = match api_key {
        Some(k) if !k.is_empty() => {
            // Multi-key field stores JSON array `["sk-a","sk-b"]`; pick the
            // first non-empty entry — sending the raw `[...]` string as a
            // bearer token would 401. Mirrors detect_provider_cache.
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
            let raw = "API key is not set. Please configure your API key first.";
            return Ok(ProviderTestResult {
                success: false,
                status: "failed".to_string(),
                message: raw.to_string(),
                latency_ms: None,
                supports_vision: None,
                diagnostic: Some(crate::diagnostics::test_failure::TestDiagnostic {
                    code: "missing_api_key".to_string(),
                    title: "还没配置 API key".to_string(),
                    hint: "在「API key」字段粘贴 Provider 控制台拿到的 key 再测连接。".to_string(),
                    action_url: None,
                    action_label: None,
                    raw: raw.to_string(),
                }),
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
    let mut last_status: Option<u16> = None;
    let mut last_body = String::new();

    for url in &urls {
        // 必须复用 provider 的 extra_headers——Kimi catalog 默认注入
        // `User-Agent: KimiCLI/1.40.0`，Moonshot 服务端对部分 plan key
        // 做 UA 校验，UA 不对一律 401，看起来像 "key 无效" 但其实是 UA。
        // 同理给 Anthropic-beta 等自定义 header 留口子。
        let mut req_builder = client
            .get(url)
            .header("Authorization", format!("Bearer {api_key}"));
        if let Some(ref eh) = extra_headers {
            if let Ok(headers) =
                serde_json::from_str::<std::collections::HashMap<String, String>>(eh)
            {
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
        let result = req_builder.send().await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                let latency = start.elapsed().as_millis() as u64;
                {
                    let conn = state
                        .db
                        .get()
                        .map_err(|_| AppError::internal("DB lock failed"))?;
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
                    diagnostic: None,
                });
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let sanitized = body.replace(&api_key, "sk-***REDACTED***");
                last_status = Some(status.as_u16());
                last_body = sanitized.clone();
                last_error = format!("HTTP {status}: {sanitized}");
            }
            Err(e) => {
                last_status = None;
                last_body.clear();
                last_error = format!("Connection error: {e}");
            }
        }
    }

    {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::update_status(&conn, &id, "failed")?;
    }

    let latency = start.elapsed().as_millis() as u64;
    let diagnostic = Some(crate::diagnostics::test_failure::diagnose(
        &provider_type,
        last_status,
        &last_body,
        &last_error,
    ));
    Ok(ProviderTestResult {
        success: false,
        status: "failed".to_string(),
        message: last_error,
        latency_ms: Some(latency),
        supports_vision: None,
        diagnostic,
    })
}

#[tauri::command]
pub async fn detect_provider_vision(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderTestResult, AppError> {
    let provider = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
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
                diagnostic: None,
            });
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(
            provider.timeout_seconds as u64,
        ))
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

    let mut req_builder = client
        .post(&url)
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
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::update_supports_vision(&conn, &id, v)?;
    }

    let (status_str, message) = match supports_vision {
        Some(true) => ("detected", "Vision supported".to_string()),
        Some(false) => ("detected", "Vision not supported".to_string()),
        None => (
            "inconclusive",
            "Vision detection inconclusive (auth / server error / network)".to_string(),
        ),
    };

    Ok(ProviderTestResult {
        success: supports_vision.is_some(),
        status: status_str.to_string(),
        message,
        latency_ms: Some(latency),
        supports_vision,
        diagnostic: None,
    })
}

#[tauri::command]
pub async fn detect_provider_cache(
    id: String,
    state: State<'_, AppState>,
) -> Result<ProviderTestResult, AppError> {
    let provider = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        storage::providers::get_by_id(&conn, &id)?
    };

    // Cache probe only works for providers with anthropic_base_url or anthropic type
    let anthropic_url =
        if provider.provider_type == "anthropic" || provider.provider_type == "claude" {
            let base = provider.base_url.trim_end_matches('/');
            Some(crate::providers::adapter::smart_append_path(
                base,
                "/messages",
            ))
        } else if let Some(ref abu) = provider.anthropic_base_url {
            if !abu.is_empty() {
                Some(crate::providers::adapter::smart_append_path(
                    abu,
                    "/messages",
                ))
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
                message:
                    "Cache probe only works for Anthropic or providers with Anthropic endpoint"
                        .to_string(),
                latency_ms: None,
                supports_vision: None,
                diagnostic: None,
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
                success: false,
                status: "failed".to_string(),
                message: "API key is not set".to_string(),
                latency_ms: None,
                supports_vision: None,
                diagnostic: None,
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

    let is_anthropic_type =
        provider.provider_type == "anthropic" || provider.provider_type == "claude";

    let build_req = |client: &reqwest::Client| {
        let mut rb = client.post(&url).header("Content-Type", "application/json");
        if is_anthropic_type {
            rb = rb
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01");
        } else {
            rb = rb.header("Authorization", format!("Bearer {api_key}"));
        }
        if let Some(ref eh) = provider.extra_headers {
            if let Ok(headers) =
                serde_json::from_str::<std::collections::HashMap<String, String>>(eh)
            {
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
            success: false,
            status: "failed".to_string(),
            message: format!("Cache probe request 1 failed: {e}"),
            latency_ms: Some(start.elapsed().as_millis() as u64),
            supports_vision: None,
            diagnostic: None,
        });
    }
    let resp1 = resp1.unwrap();
    if !resp1.status().is_success() {
        let body = resp1.text().await.unwrap_or_default();
        let sanitized = body.replace(&api_key, "sk-***REDACTED***");
        return Ok(ProviderTestResult {
            success: false,
            status: "failed".to_string(),
            message: format!(
                "Cache probe request 1 HTTP error: {}",
                &sanitized[..sanitized.len().min(500)]
            ),
            latency_ms: Some(start.elapsed().as_millis() as u64),
            supports_vision: None,
            diagnostic: None,
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
                let cache_read = json
                    .get("usage")
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
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
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
        diagnostic: None,
    })
}

// ── Gateway Commands ───────────────────────────────────────────

#[tauri::command]
pub fn get_gateway_status(state: State<'_, AppState>) -> Result<GatewayStatus, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
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
        host: if runtime.running {
            runtime.host.clone()
        } else {
            settings.host
        },
        port: if runtime.running {
            runtime.port as i64
        } else {
            settings.port
        },
        active_provider,
        input_protocol: settings.input_protocol,
        output_protocol: settings.output_protocol,
        started_at: runtime.started_at.clone(),
    })
}

#[tauri::command]
pub fn get_gateway_settings(state: State<'_, AppState>) -> Result<GatewaySettings, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::gateway_settings::get(&conn)
}

#[tauri::command]
pub fn update_gateway_settings(
    input: UpdateGatewaySettingsInput,
    state: State<'_, AppState>,
) -> Result<GatewaySettings, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::gateway_settings::update(&conn, input)
}

#[tauri::command]
pub async fn start_gateway(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<GatewayStatus, AppError> {
    // Check if already running
    {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        if runtime.running {
            return Err(AppError::new(
                "GATEWAY_ALREADY_RUNNING",
                "Gateway is already running",
            ));
        }
    }

    // Read settings
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port as u16)
    };

    // Start real HTTP server. GUI 走纯 HTTP（127.0.0.1 本地通信，无 TLS 需求）。
    let (shutdown_tx, server_handle, active_requests, _bound_port) =
        gateway::server::start(&host, port, state.db.clone(), None).await?;

    // Update runtime state
    {
        let mut runtime = state
            .gateway_runtime
            .lock()
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
    let _ = app_handle.emit("pet-gateway-state-changed", "running");
    crate::app::tray::refresh_tray(&app_handle);
    get_gateway_status(state)
}

#[tauri::command]
pub async fn stop_gateway(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<GatewayStatus, AppError> {
    let (shutdown_tx, server_handle) = {
        let mut runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        if !runtime.running {
            return Err(AppError::new(
                "GATEWAY_NOT_RUNNING",
                "Gateway is not running",
            ));
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

    let _ = app_handle.emit(
        "pet-bubble",
        serde_json::json!({ "text": "Gateway stopped", "text_zh": "网关已停止", "type": "info" }),
    );
    let _ = app_handle.emit("pet-gateway-state-changed", "stopped");
    crate::app::tray::refresh_tray(&app_handle);
    get_gateway_status(state)
}

#[tauri::command]
pub async fn restart_gateway(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<GatewayStatus, AppError> {
    // Stop if running
    {
        let is_running = {
            let runtime = state
                .gateway_runtime
                .lock()
                .map_err(|_| AppError::internal("Runtime lock failed"))?;
            runtime.running
        };
        if is_running {
            let (shutdown_tx, server_handle) = {
                let mut runtime = state
                    .gateway_runtime
                    .lock()
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
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::list(&conn, filter)
}

/// 日志里出现过的去重模型名——Logs 页「模型」筛选下拉用。
#[tauri::command]
pub fn list_log_models(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::distinct_models(&conn)
}

/// 读取某个会话的完整对话（会话详情视图用）。直接读本地 jsonl，不走 DB。
/// 先试 Claude Code 日志，找不到再试 Codex 日志。
#[tauri::command]
pub fn get_session_conversation(
    session_id: String,
) -> Result<Vec<crate::session_sync::claude::ConversationMessage>, AppError> {
    if let Ok(msgs) = crate::session_sync::claude::read_conversation(&session_id) {
        if !msgs.is_empty() {
            return Ok(msgs);
        }
    }
    crate::session_sync::codex::read_conversation(&session_id)
}

/// 删除某个会话：删 request_logs 行 + 删 Claude/Codex 本地 jsonl 文件。
/// 一个会话只在一处客户端，另一处 delete_session_file 返回 Ok(false)；删除失败传播 Err。
#[tauri::command]
pub fn delete_session(session_id: String, state: State<'_, AppState>) -> Result<(), AppError> {
    {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        storage::request_logs::delete_by_session(&conn, &session_id)?;
    }
    crate::session_sync::claude::delete_session_file(&session_id)?;
    crate::session_sync::codex::delete_session_file(&session_id)?;
    Ok(())
}

#[tauri::command]
pub fn count_request_logs(
    filter: RequestLogFilter,
    state: State<'_, AppState>,
) -> Result<i64, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::count(&conn, &filter)
}

#[tauri::command]
pub fn get_request_log_detail(
    id: String,
    state: State<'_, AppState>,
) -> Result<RequestLogDetail, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::get_detail(&conn, &id)
}

#[tauri::command]
pub fn clear_request_logs(state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::clear(&conn)
}

/// 按 session_id 聚合用量：Logs 页「按会话分组」视图用。
/// 返回最近 `limit` 个会话，按最后活跃时间倒序排列。
#[tauri::command]
pub fn aggregate_request_logs_by_session(
    filter: RequestLogFilter,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::request_log::SessionUsageSummary>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::aggregate_by_session(&conn, &filter, limit.unwrap_or(100))
}

/// days 为 None 时统计全量；Some(n) 时只统计近 n 天（与 Dashboard rangeDays 对齐）。
fn cost_since(days: Option<i64>) -> Option<String> {
    days.map(|d| (chrono::Utc::now() - chrono::Duration::days(d.max(1))).to_rfc3339())
}

/// 按模型聚合成本——成本仪表盘「钱花在哪个模型」用。
#[tauri::command]
pub fn aggregate_cost_by_model(
    days: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::request_log::CostBreakdown>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let since = cost_since(days);
    storage::request_logs::aggregate_cost_by_model(&conn, since.as_deref(), limit.unwrap_or(50))
}

/// 按客户端聚合成本——成本仪表盘「哪个客户端花得多」用。
#[tauri::command]
pub fn aggregate_cost_by_client(
    days: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::request_log::CostBreakdown>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let since = cost_since(days);
    storage::request_logs::aggregate_cost_by_client(&conn, since.as_deref(), limit.unwrap_or(50))
}

/// Provider 详情页：按模型聚合成功率/成本，并返回最近延迟点。
#[tauri::command]
pub fn aggregate_provider_detail_stats(
    provider: String,
    days: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<crate::models::request_log::ProviderDetailStats, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let since = cost_since(days);
    storage::request_logs::aggregate_provider_detail_stats(
        &conn,
        &provider,
        since.as_deref(),
        limit.unwrap_or(50),
    )
}

#[tauri::command]
pub fn aggregate_route_profile_stats(
    days: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::route_profile::RouteProfileStats>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let since = cost_since(days);
    storage::request_logs::aggregate_route_profile_stats(&conn, since.as_deref())
}

/// 扫描 ~/.claude/projects 下的 Claude Code 会话日志并写入 request_logs。
/// 幂等：已同步过的 message_id 会被跳过。
#[tauri::command]
pub async fn sync_claude_sessions(
    state: State<'_, AppState>,
) -> Result<crate::session_sync::SyncResult, AppError> {
    crate::session_sync::claude::sync(&state.db)
}

/// 扫描 ~/.codex/sessions 下的 Codex 会话日志并写入 request_logs。
/// 幂等：external_id = "{session_id}:{event_index}" 保证再次同步只写新增。
#[tauri::command]
pub async fn sync_codex_sessions(
    state: State<'_, AppState>,
) -> Result<crate::session_sync::SyncResult, AppError> {
    crate::session_sync::codex::sync(&state.db)
}

/// 扫描 ~/.gemini/tmp/*/chats 下的 Gemini CLI 会话日志并写入 request_logs。
/// 幂等：event 自带 UUID id 作 external_id。
#[tauri::command]
pub async fn sync_gemini_sessions(
    state: State<'_, AppState>,
) -> Result<crate::session_sync::SyncResult, AppError> {
    crate::session_sync::gemini::sync(&state.db)
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
            description:
                "Anthropic's CLI for Claude. Agentic coding tool with terminal integration."
                    .to_string(),
            config_exists: std::path::Path::new(&format!("{}/.claude/settings.json", home))
                .exists(),
        },
        ToolConfigView {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            slug: "codex".to_string(),
            icon: "code".to_string(),
            config_path: format!("{}/.codex/config.toml", home),
            description:
                "OpenAI's CLI coding agent. Supports OpenAI Responses API and chat completions."
                    .to_string(),
            config_exists: std::path::Path::new(&format!("{}/.codex/config.toml", home)).exists(),
        },
        ToolConfigView {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            slug: "opencode".to_string(),
            icon: "braces".to_string(),
            config_path: format!("{}/.config/opencode/opencode.json", home),
            description: "Open-source terminal AI coding assistant. Supports multiple providers."
                .to_string(),
            config_exists: std::path::Path::new(&format!(
                "{}/.config/opencode/opencode.json",
                home
            ))
            .exists(),
        },
        ToolConfigView {
            id: "atomcode".to_string(),
            name: "AtomCode".to_string(),
            slug: "atomcode".to_string(),
            icon: "atom".to_string(),
            config_path: format!("{}/.atomcode/config.toml", home),
            description:
                "Open-source AI coding agent in your terminal. Uses OpenAI-compatible API."
                    .to_string(),
            config_exists: std::path::Path::new(&format!("{}/.atomcode/config.toml", home))
                .exists(),
        },
        ToolConfigView {
            id: "gemini_cli".to_string(),
            name: "Gemini CLI".to_string(),
            slug: "gemini-cli".to_string(),
            icon: "sparkles".to_string(),
            config_path: format!("{}/.gemini/settings.json", home),
            description:
                "Google's AI coding CLI. Uses Gemini API with OpenAI-compatible endpoint support."
                    .to_string(),
            config_exists: std::path::Path::new(&format!("{}/.gemini/settings.json", home))
                .exists(),
        },
    ];

    Ok(tools)
}

#[tauri::command]
pub fn generate_codex_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    let token = crate::security::local_token::ensure_token()?;
    Ok(crate::tools::codex::generate_snippet(
        &settings.host,
        settings.port,
        &token,
    ))
}

// ── Route Profile Commands ─────────────────────────────────────

#[tauri::command]
pub fn list_route_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::route_profile::RouteProfileView>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::list_all(&conn)
}

#[tauri::command]
pub fn get_route_profile(
    id: String,
    state: State<'_, AppState>,
) -> Result<crate::models::route_profile::RouteProfileDetail, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let profile = storage::route_profiles::get_by_id(&conn, &id)?;
    let providers = storage::route_profiles::list_providers(&conn, &id)?;
    let view = {
        let active_name: Option<String> = profile.active_provider_id.as_ref().and_then(|pid| {
            storage::providers::get_by_id(&conn, pid)
                .ok()
                .map(|p| p.name)
        });
        crate::models::route_profile::RouteProfileView {
            id: profile.id.clone(),
            name: profile.name,
            input_protocol: profile.input_protocol,
            mode: profile.mode,
            selection_strategy: profile.selection_strategy,
            active_provider_id: profile.active_provider_id,
            active_provider_name: active_name,
            enabled: profile.enabled,
            is_default: profile.is_default,
            providers_count: providers.len() as i64,
            created_at: profile.created_at,
            updated_at: profile.updated_at,
        }
    };
    Ok(crate::models::route_profile::RouteProfileDetail {
        profile: view,
        providers,
    })
}

#[tauri::command]
pub fn create_route_profile(
    input: crate::models::route_profile::CreateRouteProfileInput,
    state: State<'_, AppState>,
) -> Result<crate::models::route_profile::RouteProfileView, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let profile = storage::route_profiles::create(&conn, input)?;
    Ok(crate::models::route_profile::RouteProfileView {
        id: profile.id,
        name: profile.name,
        input_protocol: profile.input_protocol,
        mode: profile.mode,
        selection_strategy: profile.selection_strategy,
        active_provider_id: profile.active_provider_id,
        active_provider_name: None,
        enabled: profile.enabled,
        is_default: profile.is_default,
        providers_count: 0,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    })
}

#[tauri::command]
pub fn update_route_profile(
    id: String,
    input: crate::models::route_profile::UpdateRouteProfileInput,
    state: State<'_, AppState>,
) -> Result<crate::models::route_profile::RouteProfileView, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let profile = storage::route_profiles::update(&conn, &id, input)?;
    let cnt: i64 = conn.query_row(
        "SELECT COUNT(*) FROM route_profile_providers WHERE route_profile_id=?1",
        [&id],
        |r| r.get(0),
    )?;
    Ok(crate::models::route_profile::RouteProfileView {
        id: profile.id,
        name: profile.name,
        input_protocol: profile.input_protocol,
        mode: profile.mode,
        selection_strategy: profile.selection_strategy,
        active_provider_id: profile.active_provider_id,
        active_provider_name: None,
        enabled: profile.enabled,
        is_default: profile.is_default,
        providers_count: cnt,
        created_at: profile.created_at,
        updated_at: profile.updated_at,
    })
}

#[tauri::command]
pub fn delete_route_profile(id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::delete(&conn, &id)
}

#[tauri::command]
pub fn set_default_route_profile(id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::set_default(&conn, &id)?;
    Ok(true)
}

#[tauri::command]
pub fn set_route_profile_mode(
    id: String,
    mode: String,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::update(
        &conn,
        &id,
        crate::models::route_profile::UpdateRouteProfileInput {
            name: None,
            mode: Some(mode),
            selection_strategy: None,
            enabled: None,
        },
    )?;
    Ok(true)
}

#[tauri::command]
pub fn set_route_active_provider(
    route_profile_id: String,
    provider_id: String,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::set_active_provider(&conn, &route_profile_id, &provider_id)?;
    Ok(true)
}

#[tauri::command]
pub fn add_provider_to_route(
    route_profile_id: String,
    provider_id: String,
    input: crate::models::route_profile::AddProviderToRouteInput,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::add_provider(&conn, &route_profile_id, &provider_id, input)?;
    Ok(true)
}

#[tauri::command]
pub fn remove_provider_from_route(
    route_profile_id: String,
    provider_id: String,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::remove_provider(&conn, &route_profile_id, &provider_id)?;
    Ok(true)
}

#[tauri::command]
pub fn reorder_route_providers(
    route_profile_id: String,
    provider_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
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
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::update_provider_conditions(
        &conn,
        &route_profile_id,
        &provider_id,
        routing_conditions.as_deref(),
    )?;
    Ok(true)
}

#[tauri::command]
pub fn list_provider_runtime_status(
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::route_profile::ProviderRuntimeStatus>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::provider_runtime_status::list_all(&conn)
}

#[tauri::command]
pub fn reset_provider_runtime_status(
    provider_id: String,
    state: State<'_, AppState>,
) -> Result<crate::models::route_profile::ProviderRuntimeStatus, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::provider_runtime_status::reset(&conn, &provider_id)
}

#[tauri::command]
pub fn reset_all_provider_runtime_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::provider_runtime_status::reset_all(&conn)?;
    Ok(true)
}

// ── Gateway Auth Commands ──────────────────────────────────────

#[tauri::command]
pub fn get_gateway_auth_settings(
) -> Result<crate::security::local_token::GatewayAuthSettings, AppError> {
    Ok(crate::security::local_token::get_auth_settings())
}

#[tauri::command]
pub fn regenerate_local_access_token(
) -> Result<crate::security::local_token::GatewayAuthSettings, AppError> {
    crate::security::local_token::regenerate_token()?;
    Ok(crate::security::local_token::get_auth_settings())
}

#[tauri::command]
pub fn ensure_local_access_token(
) -> Result<crate::security::local_token::GatewayAuthSettings, AppError> {
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

// ── Client apply history helper ────────────────────────────────

/// Snapshot the client's on-disk config files **before** the apply/disable/
/// toggle path rewrites them, and append one row to `client_apply_history`.
/// Failures are swallowed: losing one rollback point shouldn't break the
/// actual apply.
fn record_pre_apply(
    state: &State<'_, AppState>,
    client_id: &str,
    action: &str,
    paths: Vec<(&'static str, std::path::PathBuf)>,
    summary: &str,
) {
    // Read off disk before acquiring the DB lock — file I/O may be slow.
    let snap = storage::apply_history::snapshot_files_at(&paths);
    let Ok(conn) = state.db.get() else { return };
    let _ = storage::apply_history::record(&conn, client_id, action, &snap, summary);
}

// ── Codex Config Commands ──────────────────────────────────────

#[tauri::command]
pub fn detect_codex_config() -> Result<crate::tools::codex::CodexConfigStatus, AppError> {
    Ok(crate::tools::codex::detect())
}

#[tauri::command]
pub fn apply_codex_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::codex::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::Codex,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };

    record_pre_apply(
        &state,
        "codex",
        "apply",
        crate::tools::codex::snapshot_paths(),
        "apply",
    );
    crate::tools::codex::apply(&host, port)
}

#[tauri::command]
pub fn toggle_codex_provider(
    state: State<'_, AppState>,
) -> Result<crate::tools::codex::ToggleResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::Codex,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    record_pre_apply(
        &state,
        "codex",
        "toggle",
        crate::tools::codex::snapshot_paths(),
        "toggle",
    );
    crate::tools::codex::toggle_provider(&host, port)
}

/// Restore Codex to its pre-AgentGate state — the saved config.toml is
/// copied back so the user gets the official `[plugins.*]` / `[mcp_servers.*]`
/// blocks alive again. Used by the UI's "Switch to native mode" button.
#[tauri::command]
pub fn disable_codex_agentgate(
    state: State<'_, AppState>,
) -> Result<crate::tools::codex::ApplyConfigResult, AppError> {
    record_pre_apply(
        &state,
        "codex",
        "disable",
        crate::tools::codex::snapshot_paths(),
        "disable",
    );
    crate::tools::codex::disable()
}

#[tauri::command]
pub fn open_codex_config() -> Result<bool, AppError> {
    crate::tools::codex::open_config()?;
    Ok(true)
}

// ── Claude Desktop Commands（第一阶段：只读 detect + profile 预览，不写盘）──

#[tauri::command]
pub fn detect_claude_desktop() -> crate::tools::claude_desktop::ClaudeDesktopStatus {
    crate::tools::claude_desktop::detect()
}

/// 生成指向 AgentGate 网关的 3p profile JSON（pretty），仅供和用户机器上实际的
/// Claude Desktop 3p 配置对比、确认 schema，不写任何文件。
#[tauri::command]
pub fn preview_claude_desktop_profile(state: State<'_, AppState>) -> Result<String, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let s = storage::gateway_settings::get(&conn)?;
        (s.host, s.port)
    };
    let token = crate::security::local_token::ensure_token()?;
    let profile = crate::tools::claude_desktop::generate_profile(&host, port, &token);
    serde_json::to_string_pretty(&profile)
        .map_err(|e| AppError::internal(format!("serialize profile failed: {e}")))
}

/// 接入 Claude Desktop：写 3p profile + 切 appliedId 到 AgentGate。apply 前先经
/// apply_history 快照 profile/_meta，用户可在客户端历史里一键回滚。
#[tauri::command]
pub fn apply_claude_desktop_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::claude_desktop::ClaudeDesktopApplyResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    let token = crate::security::local_token::ensure_token()?;
    record_pre_apply(
        &state,
        "claude_desktop",
        "apply",
        crate::tools::claude_desktop::snapshot_paths(),
        "apply",
    );
    crate::tools::claude_desktop::apply(&host, port, &token)
}

// ── Claude Code Commands ──────────────────────────────────────

#[tauri::command]
pub fn detect_claude_code_env() -> Result<crate::tools::claude_code::ClaudeCodeEnvStatus, AppError>
{
    Ok(crate::tools::claude_code::detect_env())
}

#[tauri::command]
pub fn apply_claude_code_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::claude_code::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::ClaudeCode,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    record_pre_apply(
        &state,
        "claude_code",
        "apply",
        crate::tools::claude_code::snapshot_paths(),
        "apply",
    );
    crate::tools::claude_code::apply_config(&host, port, "claude-sonnet-4-7")
}

#[tauri::command]
pub fn toggle_claude_code_provider(
    state: State<'_, AppState>,
) -> Result<crate::tools::claude_code::ToggleResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::ClaudeCode,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    record_pre_apply(
        &state,
        "claude_code",
        "toggle",
        crate::tools::claude_code::snapshot_paths(),
        "toggle",
    );
    crate::tools::claude_code::toggle_provider(&host, port, "claude-sonnet-4-7")
}

#[tauri::command]
pub fn open_claude_code_config() -> Result<bool, AppError> {
    crate::tools::claude_code::open_config()?;
    Ok(true)
}

#[tauri::command]
pub fn generate_claude_code_env(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::claude_code::generate_env_snippet(
        &settings.host,
        settings.port,
        "claude-sonnet-4-7",
    ))
}

// ── OpenCode Commands ─────────────────────────────────────────

#[tauri::command]
pub fn detect_opencode_config() -> Result<crate::tools::opencode::OpenCodeConfigStatus, AppError> {
    Ok(crate::tools::opencode::detect())
}

#[tauri::command]
pub fn apply_opencode_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::opencode::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    record_pre_apply(
        &state,
        "opencode",
        "apply",
        crate::tools::opencode::snapshot_paths(),
        "apply",
    );
    crate::tools::opencode::apply(&host, port)
}

#[tauri::command]
pub fn generate_opencode_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::opencode::generate_snippet(
        &settings.host,
        settings.port,
    ))
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
pub fn apply_gemini_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::gemini_cli::ApplyConfigResult, AppError> {
    let (host, port, model) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gemini-2.5-flash".to_string());
        (settings.host, settings.port, model)
    };
    record_pre_apply(
        &state,
        "gemini",
        "apply",
        crate::tools::gemini_cli::snapshot_paths(),
        "apply",
    );
    crate::tools::gemini_cli::apply(&host, port, &model)
}

#[tauri::command]
pub fn generate_gemini_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::gemini_cli::generate_snippet(
        &settings.host,
        settings.port,
        "gemini-2.5-flash",
    ))
}

#[tauri::command]
pub fn toggle_gemini_provider(
    state: State<'_, AppState>,
) -> Result<crate::tools::gemini_cli::ToggleResult, AppError> {
    let (host, port, model) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gemini-2.5-flash".to_string());
        (settings.host, settings.port, model)
    };
    record_pre_apply(
        &state,
        "gemini",
        "toggle",
        crate::tools::gemini_cli::snapshot_paths(),
        "toggle",
    );
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
pub fn apply_atomcode_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::atomcode::ApplyConfigResult, AppError> {
    let (host, port, model) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gpt-5.5".to_string());
        (settings.host, settings.port, model)
    };
    record_pre_apply(
        &state,
        "atomcode",
        "apply",
        crate::tools::atomcode::snapshot_paths(),
        "apply",
    );
    crate::tools::atomcode::apply(&host, port, &model)
}

#[tauri::command]
pub fn generate_atomcode_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    let provider_id = settings.active_provider_id.clone().unwrap_or_default();
    let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
    let model = provider
        .map(|p| p.default_model)
        .unwrap_or_else(|| "gpt-5.5".to_string());
    Ok(crate::tools::atomcode::generate_snippet(
        &settings.host,
        settings.port,
        &model,
    ))
}

/// After a client's config is rewritten, look up matching live processes
/// so the UI can warn the user that the existing session needs to be
/// restarted to pick up the new config. Each `client_id` maps to one or
/// more process basenames (e.g. `codex` matches both the CLI and the
/// macOS desktop app). Returns an empty list on Windows (pgrep-only
/// detection); the caller treats empty as "couldn't detect", not "OK".
#[tauri::command]
pub fn detect_client_running(
    client_id: String,
) -> Result<Vec<crate::tools::process_detect::RunningProcess>, AppError> {
    let needles: &[&str] = match client_id.as_str() {
        "codex" => &["codex"],
        "claude_code" => &["claude"],
        "opencode" => &["opencode"],
        "gemini" => &["gemini"],
        "atomcode" => &["atomcode"],
        _ => return Err(AppError::validation("unknown client_id")),
    };
    Ok(crate::tools::process_detect::find_running(needles))
}

/// Restart Codex Desktop so freshly-written config.toml / auth.json take
/// effect. macOS only at the moment — `restart_codex_desktop` returns
/// `supported: false` on other platforms and the UI hides the button.
/// Never called automatically; only fires when the user clicks the button in
/// PostApplyDialog.
#[tauri::command]
pub fn restart_codex_desktop() -> Result<crate::tools::codex_restart::CodexRestartResult, AppError>
{
    crate::tools::codex_restart::restart()
}

/// 读取各客户端(Codex / Claude Code)现有的 MCP server 配置，汇总展示。
/// 以客户端文件为真相源，只读不写；env 只返回 key 不返回 value。
#[tauri::command]
pub fn list_mcp_servers() -> Result<Vec<crate::tools::mcp::McpServer>, AppError> {
    Ok(crate::tools::mcp::list_all())
}

/// 添加或更新指定客户端的 MCP server。只写入一个客户端配置文件，不做跨客户端同步。
#[tauri::command]
pub fn upsert_mcp_server(
    input: crate::tools::mcp::UpsertMcpServerInput,
) -> Result<crate::tools::mcp::McpServer, AppError> {
    crate::tools::mcp::upsert(input)
}

/// 删除指定客户端的 MCP server。文件或 server 不存在时返回 false。
#[tauri::command]
pub fn delete_mcp_server(client: String, name: String) -> Result<bool, AppError> {
    crate::tools::mcp::delete(&client, &name)
}

/// 将一个客户端里的 MCP server 显式同步到一个或多个目标客户端。
#[tauri::command]
pub fn sync_mcp_server(
    input: crate::tools::mcp::SyncMcpServerInput,
) -> Result<Vec<crate::tools::mcp::McpServer>, AppError> {
    crate::tools::mcp::sync(input)
}

/// 导出 MCP server 配置。默认由前端传 include_secrets=false，不导出 env value。
#[tauri::command]
pub fn export_mcp_servers(include_secrets: bool) -> Result<String, AppError> {
    crate::tools::mcp::export_config(include_secrets)
}

/// 从 JSON 文本导入 MCP server 配置到指定客户端。
#[tauri::command]
pub fn import_mcp_servers(
    payload: String,
    target_clients: Vec<String>,
) -> Result<Vec<crate::tools::mcp::McpServer>, AppError> {
    crate::tools::mcp::import_config(&payload, target_clients)
}

/// 曾经 apply 过配置的客户端 id 列表。前端用来判断「配置漂移」：客户端 detected
/// 但 id 在这个列表里，说明接入过又被改回去了，提示重新应用。
#[tauri::command]
pub fn clients_with_apply_history(
    state: State<'_, AppState>,
) -> Result<Vec<String>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::apply_history::distinct_clients(&conn)
}

/// 列出某客户端的 apply/disable/toggle 历史（按时间倒序）。前端用来
/// 渲染历史抽屉。
#[tauri::command]
pub fn list_client_apply_history(
    state: State<'_, AppState>,
    client_id: String,
) -> Result<Vec<storage::apply_history::HistoryEntry>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::apply_history::list(&conn, &client_id)
}

/// 回滚到某条历史记录所代表的盘上状态。snapshot 反序列化后按 file 写回原
/// absolute_path（不存在的文件被删除）。回滚本身**不**记录新历史，避免反复
/// 回滚把保留窗撑满。
#[tauri::command]
pub fn rollback_client_apply(
    state: State<'_, AppState>,
    history_id: String,
) -> Result<storage::apply_history::HistoryEntry, AppError> {
    let entry = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        storage::apply_history::get(&conn, &history_id)?
    };
    let snapshot: storage::apply_history::ClientSnapshot =
        serde_json::from_str(&entry.snapshot_json)
            .map_err(|e| AppError::internal(format!("snapshot deserialise failed: {e}")))?;
    storage::apply_history::restore_files(&snapshot)?;
    Ok(entry)
}

#[tauri::command]
pub fn toggle_atomcode_provider(
    state: State<'_, AppState>,
) -> Result<crate::tools::atomcode::ToggleResult, AppError> {
    let (host, port, model) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gpt-5.5".to_string());
        (settings.host, settings.port, model)
    };
    record_pre_apply(
        &state,
        "atomcode",
        "toggle",
        crate::tools::atomcode::snapshot_paths(),
        "toggle",
    );
    crate::tools::atomcode::toggle(&host, port, &model)
}

#[tauri::command]
pub fn open_atomcode_config() -> Result<bool, AppError> {
    crate::tools::atomcode::open_config()?;
    Ok(true)
}

// ── Global Instructions (CLAUDE.md / AGENTS.md) ────────────────

/// 列出内置模板。模板是只读静态资源，不需要参数也不消耗 DB。
#[tauri::command]
pub fn list_instructions_templates(
) -> Result<Vec<crate::tools::instructions_templates::InstructionsTemplate>, AppError> {
    // 静态 slice → Vec 让 Tauri 能序列化。
    Ok(crate::tools::instructions_templates::TEMPLATES
        .iter()
        .cloned()
        .collect())
}

/// 读取某 scope（claude_global / codex_global）的全局指令文件原文。
/// 文件不存在时返回 `exists=false, content=""`，让前端 textarea 仍可编辑。
#[tauri::command]
pub fn read_global_instructions(
    scope: String,
) -> Result<crate::tools::instructions::InstructionsStatus, AppError> {
    let s = crate::tools::instructions::InstructionsScope::from_str(&scope).ok_or_else(|| {
        AppError::new("INSTRUCTIONS_BAD_SCOPE", format!("unknown scope: {scope}"))
    })?;
    Ok(crate::tools::instructions::read(s))
}

/// 手动编辑后保存。和 5 个客户端的 apply 流程一致：写盘前 snapshot 一次磁盘
/// 原文，便于事后回滚。
#[tauri::command]
pub fn write_global_instructions(
    state: State<'_, AppState>,
    scope: String,
    content: String,
) -> Result<crate::tools::instructions::InstructionsStatus, AppError> {
    let s = crate::tools::instructions::InstructionsScope::from_str(&scope).ok_or_else(|| {
        AppError::new("INSTRUCTIONS_BAD_SCOPE", format!("unknown scope: {scope}"))
    })?;
    record_pre_apply(
        &state,
        s.history_client_id(),
        "write",
        crate::tools::instructions::snapshot_paths(s),
        "manual edit",
    );
    crate::tools::instructions::write(s, &content)
}

/// 把模板按 overwrite / append 写入目标 scope。同样在写盘前打 snapshot。
#[tauri::command]
pub fn apply_instructions_template(
    state: State<'_, AppState>,
    scope: String,
    template_id: String,
    mode: String,
) -> Result<crate::tools::instructions::InstructionsStatus, AppError> {
    let s = crate::tools::instructions::InstructionsScope::from_str(&scope).ok_or_else(|| {
        AppError::new("INSTRUCTIONS_BAD_SCOPE", format!("unknown scope: {scope}"))
    })?;
    let m = crate::tools::instructions::ApplyMode::from_str(&mode)
        .ok_or_else(|| AppError::new("INSTRUCTIONS_BAD_MODE", format!("unknown mode: {mode}")))?;
    let summary = format!("template {template_id} ({mode})");
    record_pre_apply(
        &state,
        s.history_client_id(),
        "apply_template",
        crate::tools::instructions::snapshot_paths(s),
        &summary,
    );
    crate::tools::instructions::apply_template(s, &template_id, m)
}

/// 导出两个 scope 的全局指令为一份 JSON 备份（6.5）。
#[tauri::command]
pub fn export_instructions() -> Result<crate::tools::instructions::InstructionsBackup, AppError> {
    Ok(crate::tools::instructions::export_backup())
}

/// 从备份 JSON 恢复全局指令。每个非空 scope overwrite 写入，写盘前各打一次
/// snapshot，复用现有回滚机制。返回恢复后的两个 scope 状态。
#[tauri::command]
pub fn import_instructions(
    state: State<'_, AppState>,
    payload: String,
) -> Result<Vec<crate::tools::instructions::InstructionsStatus>, AppError> {
    let backup: crate::tools::instructions::InstructionsBackup = serde_json::from_str(&payload)
        .map_err(|e| AppError::new("INSTRUCTIONS_IMPORT_BAD_JSON", format!("invalid json: {e}")))?;
    use crate::tools::instructions::InstructionsScope;
    let mut out = Vec::new();
    for (scope, content) in [
        (InstructionsScope::ClaudeGlobal, backup.claude),
        (InstructionsScope::CodexGlobal, backup.codex),
    ] {
        if content.trim().is_empty() {
            continue;
        }
        record_pre_apply(
            &state,
            scope.history_client_id(),
            "import_backup",
            crate::tools::instructions::snapshot_paths(scope),
            "restore from backup",
        );
        out.push(crate::tools::instructions::write(scope, &content)?);
    }
    Ok(out)
}

// ── Local Skills (~/.claude/skills) ────────────────────────────

/// 列出本地 skill（读 frontmatter + 启用状态）。
#[tauri::command]
pub fn list_skills() -> Result<Vec<crate::tools::skills::Skill>, AppError> {
    Ok(crate::tools::skills::list_skills())
}

/// 启用/禁用一个 skill（重命名 manifest）。source 为 claude / codex。
#[tauri::command]
pub fn set_skill_enabled(
    source: String,
    id: String,
    enabled: bool,
) -> Result<crate::tools::skills::Skill, AppError> {
    crate::tools::skills::set_skill_enabled(&source, &id, enabled)
}

/// 删除一个 skill 目录（强确认在前端）。
#[tauri::command]
pub fn delete_skill(source: String, id: String) -> Result<bool, AppError> {
    crate::tools::skills::delete_skill(&source, &id)
}

/// 从本地 ZIP 字节安装一个 skill 到指定来源客户端（前端读文件成字节传入）。
#[tauri::command]
pub fn import_skill_from_zip(
    source: String,
    bytes: Vec<u8>,
) -> Result<crate::tools::skills::Skill, AppError> {
    crate::tools::skills::import_skill_from_zip(&source, &bytes)
}

/// 导出所有 skill 为可备份 JSON（6.5）。
#[tauri::command]
pub fn export_skills() -> Result<crate::tools::skills::SkillsExport, AppError> {
    Ok(crate::tools::skills::export_skills())
}

/// 从备份 JSON 恢复 skill（已存在的目录跳过，不覆盖）。
#[tauri::command]
pub fn import_skills(payload: String) -> Result<Vec<crate::tools::skills::Skill>, AppError> {
    crate::tools::skills::import_skills(&payload)
}

// ── Provider Health Commands ──────────────────────────────────

#[tauri::command]
pub fn get_provider_health(
    state: State<'_, AppState>,
    provider: String,
) -> Result<crate::storage::request_logs::ProviderHealth, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::request_logs::get_provider_health(&conn, &provider)
}

// ── Pricing Commands ──────────────────────────────────────────

#[tauri::command]
pub fn list_model_pricing(
    state: State<'_, AppState>,
) -> Result<Vec<crate::storage::pricing::ModelPricing>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::list_all(&conn)
}

#[tauri::command]
pub fn upsert_model_pricing(
    state: State<'_, AppState>,
    provider: String,
    model_pattern: String,
    input_price: f64,
    output_price: f64,
) -> Result<crate::storage::pricing::ModelPricing, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::upsert_custom(
        &conn,
        &provider,
        &model_pattern,
        input_price,
        output_price,
    )
}

#[tauri::command]
pub fn delete_model_pricing(state: State<'_, AppState>, id: String) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::delete_custom(&conn, &id)
}

// ── Stats Commands ─────────────────────────────────────────────

#[tauri::command]
pub fn get_request_stats(
    state: State<'_, AppState>,
) -> Result<crate::storage::request_logs::RequestStats, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::request_logs::get_stats(&conn)
}

/// Stats over a configurable window (in days). Dashboard date-range tabs
/// (今天/7天/14天/30天) call this with 1/7/14/30 respectively.
#[tauri::command]
pub fn get_request_stats_range(
    days: i64,
    state: State<'_, AppState>,
) -> Result<crate::storage::request_logs::RequestStats, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
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
    let runtime = state
        .gateway_runtime
        .lock()
        .map_err(|_| AppError::internal("Runtime lock failed"))?;
    let active_requests = runtime
        .active_requests
        .as_ref()
        .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(0);
    let gateway_running = runtime.running;
    let gateway_port = runtime.port;
    let uptime_seconds = runtime
        .started_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|started| (chrono::Utc::now() - started.with_timezone(&chrono::Utc)).num_seconds())
        .unwrap_or(0);
    drop(runtime);

    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
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
pub fn run_health_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::health_check(&state.db))
}

#[tauri::command]
pub fn run_database_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::database_check(&state.db))
}

#[tauri::command]
pub fn run_gateway_auth_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::gateway_auth_check(&state.db))
}

#[tauri::command]
pub fn run_provider_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::provider_check(&state.db))
}

#[tauri::command]
pub fn run_codex_config_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::codex_config_check(&state.db))
}

#[tauri::command]
pub fn run_claude_code_config_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::claude_code_config_check(
        &state.db,
    ))
}

#[tauri::command]
pub fn run_route_profile_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::route_profile_check(&state.db))
}

#[tauri::command]
pub fn run_full_self_test(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::FullSelfTestReport, AppError> {
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
pub async fn test_tool_connection(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, AppError> {
    // Step 1: Check gateway is running
    let (running, host, port) = {
        let runtime = state
            .gateway_runtime
            .lock()
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
    let gateway_ok = client
        .get(&health_url)
        .send()
        .await
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
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let route_model =
            storage::route_profiles::get_default_for_protocol(&conn, "openai_chat_completions")?
                .and_then(|profile| profile.active_provider_id)
                .and_then(|provider_id| storage::providers::get_by_id(&conn, &provider_id).ok())
                .map(|provider| provider.default_model);
        route_model
            .or_else(|| {
                storage::providers::list_all(&conn)
                    .ok()
                    .and_then(|providers| {
                        providers
                            .into_iter()
                            .find(|provider| provider.enabled && provider.is_active)
                            .map(|provider| provider.default_model)
                    })
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

    let resp = client
        .post(&test_url)
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
                    (
                        false,
                        Some(format!(
                            "Provider error: {} {}",
                            status.as_u16(),
                            body.chars().take(100).collect::<String>()
                        )),
                    )
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
pub fn get_pet_settings(
    state: State<'_, AppState>,
) -> Result<crate::models::pet::PetSettings, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::pet_settings::get(&conn)
}

#[tauri::command]
pub fn update_pet_settings(
    input: crate::models::pet::UpdatePetSettingsInput,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::models::pet::PetSettings, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let result = storage::pet_settings::update(&conn, input)?;
    let _ = app_handle.emit("pet-settings-changed", &result);
    Ok(result)
}

#[tauri::command]
pub fn set_pet_visible(
    visible: bool,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::models::pet::PetSettings, AppError> {
    if let Some(pet_win) = app_handle.get_webview_window("pet") {
        if visible {
            crate::move_pet_to_visible_area(&app_handle, &pet_win);
            let _ = pet_win.show();
            let _ = pet_win.set_focus();
        } else {
            let _ = pet_win.hide();
        }
    }
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::pet_settings::update(
        &conn,
        crate::models::pet::UpdatePetSettingsInput {
            pet_type: None,
            visible: Some(visible),
            pos_x: None,
            pos_y: None,
        },
    )
}

/// 轻量版:只返回 state + last_error,**不**做全表 stats 聚合。
/// 给 10s 轮询用,频次高所以必须便宜。
/// last_error 走 idx_request_logs_timestamp 索引,O(log n) 几乎免费。
/// stats 数据用单独的 `get_pet_gateway_state`(原命令)在 30 分钟 stats bubble 触发前调一次。
#[tauri::command]
pub fn get_pet_gateway_state_lite(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, AppError> {
    let (running, active) = {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        let active = runtime
            .active_requests
            .as_ref()
            .map(|c| c.load(std::sync::atomic::Ordering::Relaxed) > 0)
            .unwrap_or(false);
        (runtime.running, active)
    };

    let gw_state = if !running {
        "stopped"
    } else if active {
        "active"
    } else {
        "running"
    };

    let last_error = if running {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
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
        )
        .ok()
    } else {
        None
    };

    Ok(serde_json::json!({
        "state": gw_state,
        "last_error": last_error,
    }))
}

#[tauri::command]
pub fn get_pet_gateway_state(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    let (running, active, runtime_host, runtime_port) = {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        let active = runtime
            .active_requests
            .as_ref()
            .map(|c| c.load(std::sync::atomic::Ordering::Relaxed) > 0)
            .unwrap_or(false);
        (
            runtime.running,
            active,
            runtime.host.clone(),
            runtime.port as i64,
        )
    };

    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    let active_provider = settings
        .active_provider_id
        .as_ref()
        .and_then(|pid| storage::providers::get_by_id(&conn, pid).ok())
        .map(
            |p| serde_json::json!({ "id": p.id, "name": p.name, "default_model": p.default_model }),
        );

    let gw_state = if !running {
        "stopped"
    } else if active {
        "active"
    } else {
        "running"
    };

    let last_error = if running {
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
        )
        .ok()
    } else {
        None
    };

    let stats = storage::request_logs::get_stats(&conn).ok();
    let today_stats = stats
        .as_ref()
        .map(|s| {
            serde_json::json!({
                "requests": s.today_total,
                "errors": s.today_errors,
                "input_tokens": s.today_input_tokens,
                "output_tokens": s.today_output_tokens,
                "cache_read_tokens": s.today_cache_read_tokens,
                "cache_write_tokens": s.today_cache_write_tokens,
                "cost": s.today_cost,
            })
        })
        .unwrap_or_else(|| {
            serde_json::json!({
                "requests": 0,
                "errors": 0,
                "input_tokens": 0,
                "output_tokens": 0,
                "cache_read_tokens": 0,
                "cache_write_tokens": 0,
                "cost": 0.0,
            })
        });

    let latest_model = conn
        .query_row(
            "SELECT model FROM request_logs
             WHERE source = 'gateway' AND model IS NOT NULL AND model != ''
             ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();

    Ok(serde_json::json!({
        "state": gw_state,
        "running": running,
        "host": if running { runtime_host } else { settings.host },
        "port": if running { runtime_port } else { settings.port },
        "active_provider": active_provider,
        "latest_model": latest_model,
        "last_error": last_error,
        "today": today_stats,
    }))
}

// ── Pet Chat Commands ─────────────────────────────────────────

#[tauri::command]
pub fn get_pet_memory(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    Ok(storage::app_settings::get(&conn, "pet_memory")?.unwrap_or_else(|| "{}".to_string()))
}

/// 原生右键菜单(替代 HTML 实现)——HTML 菜单画在宠物窗口里,菜单展开
/// 期间窗口区域全部接事件,挡底层应用。换成 OS 弹出菜单完全脱离 webview,
/// 不挡也不需要 resize 窗口。
///
/// 9 个角色用子菜单 + checked 标记当前选中。鼠标穿透用 CheckMenuItem。
/// 菜单事件统一在 lib.rs 的 on_menu_event 里处理(pet_ 前缀)。
#[tauri::command]
pub fn show_pet_context_menu(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    use tauri::menu::{
        CheckMenuItemBuilder, ContextMenu, MenuBuilder, MenuItemBuilder, SubmenuBuilder,
    };

    let pet_win = app_handle
        .get_webview_window("pet")
        .ok_or_else(|| AppError::internal("pet window not found"))?;

    let zh = crate::is_chinese_locale_pub();
    let click_through = *state
        .pet_click_through
        .lock()
        .map_err(|_| AppError::internal("ct lock"))?;

    let (current_pet_type, active_provider_name, today_total) = {
        let conn = state.db.get().map_err(|_| AppError::internal("db lock"))?;
        let current_pet_type = storage::pet_settings::get(&conn)
            .map(|s| s.pet_type)
            .unwrap_or_else(|_| "robot".into());
        let settings = storage::gateway_settings::get(&conn)?;
        let active_provider_name = settings
            .active_provider_id
            .as_ref()
            .and_then(|pid| storage::providers::get_by_id(&conn, pid).ok())
            .map(|p| p.name);
        let today_total = storage::request_logs::get_stats(&conn)
            .map(|s| s.today_total)
            .unwrap_or(0);
        (current_pet_type, active_provider_name, today_total)
    };
    let (gateway_running, gateway_port) = {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("runtime lock"))?;
        (runtime.running, runtime.port)
    };

    let pet_types: &[(&str, &str, &str)] = &[
        ("robot", "网关机器人", "Gateway Bot"),
        ("pixel-cat", "像素猫", "Pixel Cat"),
        ("slime", "史莱姆", "Slime"),
        ("fox", "CEO", "CEO"),
        ("octopus", "章鱼", "Octopus"),
        ("ghost", "麻凡", "MaFan"),
        ("ox", "奎奎", "KuiKui"),
        ("soldier", "分总", "FenZong"),
        ("coder", "振振", "ZhenZhen"),
    ];

    let mut switch_builder =
        SubmenuBuilder::new(&app_handle, if zh { "切换角色" } else { "Switch Pet" });
    for (id, zh_n, en_n) in pet_types {
        let label = if zh { *zh_n } else { *en_n };
        let item = CheckMenuItemBuilder::with_id(format!("pet_switch:{id}"), label)
            .checked(current_pet_type == *id)
            .build(&app_handle)
            .map_err(|e| AppError::internal(format!("menu: {e}")))?;
        switch_builder = switch_builder.item(&item);
    }
    let switch_submenu = switch_builder
        .build()
        .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let click_through_item = CheckMenuItemBuilder::with_id(
        "pet_toggle_click_through",
        if zh { "鼠标穿透" } else { "Click-through" },
    )
    .checked(click_through)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let gateway_status_label = if gateway_running {
        if zh {
            format!("网关运行中 · :{gateway_port}")
        } else {
            format!("Gateway running · :{gateway_port}")
        }
    } else if zh {
        "网关已停止".to_string()
    } else {
        "Gateway stopped".to_string()
    };
    let gateway_status_item = MenuItemBuilder::with_id("pet_info_gateway", gateway_status_label)
        .enabled(false)
        .build(&app_handle)
        .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let provider_label = active_provider_name
        .map(|name| {
            if zh {
                format!("当前供应商：{name}")
            } else {
                format!("Active: {name}")
            }
        })
        .unwrap_or_else(|| {
            if zh {
                "未选择供应商".to_string()
            } else {
                "No active provider".to_string()
            }
        });
    let provider_item = MenuItemBuilder::with_id("pet_info_provider", provider_label)
        .enabled(false)
        .build(&app_handle)
        .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let today_item = MenuItemBuilder::with_id(
        "pet_info_today",
        if zh {
            format!("今日请求：{today_total}")
        } else {
            format!("Today: {today_total} requests")
        },
    )
    .enabled(false)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let start_gateway_item = MenuItemBuilder::with_id(
        "pet_start_gateway",
        if zh { "启动网关" } else { "Start Gateway" },
    )
    .enabled(!gateway_running)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let stop_gateway_item = MenuItemBuilder::with_id(
        "pet_stop_gateway",
        if zh { "停止网关" } else { "Stop Gateway" },
    )
    .enabled(gateway_running)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let restart_gateway_item = MenuItemBuilder::with_id(
        "pet_restart_gateway",
        if zh {
            "重启网关"
        } else {
            "Restart Gateway"
        },
    )
    .enabled(gateway_running)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let open_gateway_item = MenuItemBuilder::with_id(
        "pet_open_gateway",
        if zh {
            "打开网关页"
        } else {
            "Open Gateway"
        },
    )
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let open_logs_item =
        MenuItemBuilder::with_id("pet_open_logs", if zh { "打开日志" } else { "Open Logs" })
            .build(&app_handle)
            .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let open_settings_item = MenuItemBuilder::with_id(
        "pet_open_settings",
        if zh { "打开设置" } else { "Open Settings" },
    )
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let reset_memory_item = MenuItemBuilder::with_id(
        "pet_reset_memory",
        if zh { "清空记忆" } else { "Reset Memory" },
    )
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let hide_pet_item =
        MenuItemBuilder::with_id("pet_hide", if zh { "隐藏宠物" } else { "Hide Pet" })
            .build(&app_handle)
            .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let menu = MenuBuilder::new(&app_handle)
        .item(&gateway_status_item)
        .item(&provider_item)
        .item(&today_item)
        .separator()
        .item(&start_gateway_item)
        .item(&stop_gateway_item)
        .item(&restart_gateway_item)
        .separator()
        .item(&open_gateway_item)
        .item(&open_logs_item)
        .separator()
        .item(&switch_submenu)
        .separator()
        .item(&click_through_item)
        .item(&open_settings_item)
        .item(&reset_memory_item)
        .separator()
        .item(&hide_pet_item)
        .build()
        .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    // popup 要的是 Window 不是 WebviewWindow——从 WebviewWindow 拿底层 window 句柄。
    menu.popup(pet_win.as_ref().window().clone())
        .map_err(|e| AppError::internal(format!("popup: {e}")))?;

    Ok(())
}

/// 宠物窗口的鼠标穿透状态。三个入口(右键菜单 / tray / Settings)都改这里,
/// emit `pet-click-through-changed` 让所有 webview 同步。
#[tauri::command]
pub fn get_pet_click_through(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(*state
        .pet_click_through
        .lock()
        .map_err(|_| AppError::internal("lock failed"))?)
}

#[tauri::command]
pub fn set_pet_click_through(
    value: bool,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    *state
        .pet_click_through
        .lock()
        .map_err(|_| AppError::internal("lock failed"))? = value;
    let _ = app_handle.emit("pet-click-through-changed", value);
    Ok(value)
}

/// 从宠物右键菜单触发:把主窗口拉起来 + 通知前端导航到「宠物」设置页。
/// 主窗口可能被最小化/隐藏,所以先 unminimize 再 show + set_focus。
#[tauri::command]
pub fn pet_open_settings(app_handle: tauri::AppHandle) -> Result<bool, AppError> {
    if let Some(w) = app_handle.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app_handle.emit("pet-open-settings", ());
    Ok(true)
}

#[tauri::command]
pub fn save_pet_memory(memory: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::app_settings::set(&conn, "pet_memory", &memory)?;
    Ok(true)
}

#[tauri::command]
pub async fn pet_chat(
    messages: Vec<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let (host, port) = {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        if !runtime.running {
            return Err(AppError::new(
                "GATEWAY_NOT_RUNNING",
                "Gateway is not running",
            )
            .with_suggestion("Start the gateway from the pet menu or Gateway page"));
        }
        (runtime.host.clone(), runtime.port)
    };

    let token = crate::security::local_token::ensure_token()?;
    let host = gateway_client_host(&host);
    let url = format!("http://{}:{}/v1/chat/completions", format_host_for_url(&host), port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::internal(format!("HTTP client error: {e}")))?;

    let body = serde_json::json!({
        "model": "agentgate",
        "messages": messages,
        "max_tokens": 200,
        "temperature": 0.8,
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("User-Agent", "AgentGate-Pet/1.0")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(pet_gateway_error(status.as_u16(), &text));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::internal(format!("Parse error: {e}")))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("...")
        .to_string();

    Ok(content)
}

fn gateway_client_host(host: &str) -> String {
    match host.trim() {
        "" | "0.0.0.0" | "::" => "127.0.0.1".to_string(),
        other => other.to_string(),
    }
}

fn format_host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') && !host.ends_with(']') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn pet_gateway_error(status: u16, body: &str) -> AppError {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let err = parsed.as_ref().and_then(|v| v.get("error"));
    let code = err
        .and_then(|e| e.get("code"))
        .and_then(|v| v.as_str())
        .unwrap_or("PET_GATEWAY_CHAT_ERROR");
    let message = err
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Gateway chat failed with HTTP {status}"));
    let detail = err
        .and_then(|e| e.get("detail"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.chars().take(1000).collect())
            }
        });
    let suggestion = err
        .and_then(|e| e.get("suggestion"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty());

    let mut app_error = AppError::new(code, message);
    if let Some(detail) = detail {
        app_error = app_error.with_detail(detail);
    }
    if let Some(suggestion) = suggestion {
        app_error = app_error.with_suggestion(suggestion);
    }
    app_error
}

#[cfg(test)]
mod pet_chat_tests {
    use super::*;

    #[test]
    fn gateway_client_host_uses_loopback_for_wildcard_bind() {
        assert_eq!(gateway_client_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(gateway_client_host("::"), "127.0.0.1");
        assert_eq!(gateway_client_host("127.0.0.1"), "127.0.0.1");
    }

    #[test]
    fn format_host_for_url_wraps_ipv6() {
        assert_eq!(format_host_for_url("127.0.0.1"), "127.0.0.1");
        assert_eq!(format_host_for_url("::1"), "[::1]");
        assert_eq!(format_host_for_url("[::1]"), "[::1]");
    }

    #[test]
    fn pet_gateway_error_extracts_openai_error_shape() {
        let err = pet_gateway_error(
            503,
            r#"{"error":{"message":"No active provider configured","code":"ACTIVE_PROVIDER_NOT_FOUND","detail":"none","suggestion":"pick one"}}"#,
        );
        assert_eq!(err.code, "ACTIVE_PROVIDER_NOT_FOUND");
        assert_eq!(err.message, "No active provider configured");
        assert_eq!(err.detail, Some("none".to_string()));
        assert_eq!(err.suggestion, Some("pick one".to_string()));
    }

    #[test]
    fn pet_gateway_error_keeps_plain_body_as_detail() {
        let err = pet_gateway_error(500, "plain failure");
        assert_eq!(err.code, "PET_GATEWAY_CHAT_ERROR");
        assert_eq!(err.message, "Gateway chat failed with HTTP 500");
        assert_eq!(err.detail, Some("plain failure".to_string()));
    }
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
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
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
    let payload: storage::config_backups::ConfigExport =
        serde_json::from_str(&json).map_err(|e| {
            AppError::new(
                "CONFIG_IMPORT_PARSE_ERROR",
                format!("Invalid config JSON: {e}"),
            )
            .with_suggestion(
                "Make sure the file is an AgentGate config export, not a different JSON file.",
            )
        })?;
    let mut conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::config_backups::import(&mut conn, &payload)
}
