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
            config_path: format!("{}/.config/opencode/config.json", home),
            description: "Open-source terminal AI coding assistant. Supports multiple providers.".to_string(),
            config_exists: std::path::Path::new(&format!("{}/.config/opencode/config.json", home)).exists(),
        },
    ];

    Ok(tools)
}

#[tauri::command]
pub fn generate_codex_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;

    let config = format!(
        r#"model = "gpt-5"
model_provider = "agentgate"

[model_providers.agentgate]
name = "AgentGate"
base_url = "http://{}:{}/v1"
wire_api = "responses""#,
        settings.host, settings.port
    );

    Ok(config)
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
            id: profile.id.clone(), name: profile.name, client_type: profile.client_type,
            input_protocol: profile.input_protocol, mode: profile.mode,
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
        id: profile.id, name: profile.name, client_type: profile.client_type,
        input_protocol: profile.input_protocol, mode: profile.mode,
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
        id: profile.id, name: profile.name, client_type: profile.client_type,
        input_protocol: profile.input_protocol, mode: profile.mode,
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
pub fn preview_codex_config(state: State<'_, AppState>) -> Result<crate::tools::codex::ConfigPreview, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::codex::preview(&settings.host, settings.port))
}

#[tauri::command]
pub fn apply_codex_config(state: State<'_, AppState>) -> Result<crate::tools::codex::ApplyConfigResult, AppError> {
    let (host, port, backup_dir) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let app_data = std::env::var("HOME").unwrap_or_default();
        let backup_dir = std::path::PathBuf::from(&app_data).join(".agentgate").join("backups");
        (settings.host, settings.port, backup_dir)
    };

    let result = crate::tools::codex::apply(&host, port, &backup_dir)?;

    // Record backup in DB
    if let Some(ref bp) = result.backup_path {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::config_backups::insert(&conn, "codex", &result.config_path, bp, "config_file", None);
    }

    Ok(result)
}

#[tauri::command]
pub fn backup_codex_config(state: State<'_, AppState>) -> Result<crate::storage::config_backups::ConfigBackup, AppError> {
    let app_data = std::env::var("HOME").unwrap_or_default();
    let backup_dir = std::path::PathBuf::from(&app_data).join(".agentgate").join("backups");

    let result = crate::tools::codex::backup(&backup_dir)?;

    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let backup = storage::config_backups::insert(&conn, "codex", &result.source_path, &result.backup_path, "config_file", None)?;
    Ok(backup)
}

#[tauri::command]
pub fn list_codex_backups(state: State<'_, AppState>) -> Result<Vec<crate::storage::config_backups::ConfigBackup>, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::config_backups::list_by_tool(&conn, "codex")
}

#[tauri::command]
pub fn restore_codex_backup(backup_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let backup = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::config_backups::get_by_id(&conn, &backup_id)?
    };

    let app_data = std::env::var("HOME").unwrap_or_default();
    let backup_dir = std::path::PathBuf::from(&app_data).join(".agentgate").join("backups");
    crate::tools::codex::restore(&backup.backup_path, &backup_dir)?;
    Ok(true)
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
pub fn preview_claude_code_config(state: State<'_, AppState>) -> Result<crate::tools::claude_code::ClaudeCodeConfigPreview, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    crate::tools::claude_code::preview_config(&settings.host, settings.port, "claude-sonnet-4-6")
}

#[tauri::command]
pub fn apply_claude_code_config(state: State<'_, AppState>) -> Result<crate::tools::claude_code::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    let backup_dir = crate::security::local_token::token_dir().join("backups");
    let result = crate::tools::claude_code::apply_config(&host, port, "claude-sonnet-4-6", &backup_dir)?;

    if let Some(ref bp) = result.backup_path {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::config_backups::insert(&conn, "claude_code", &result.config_path, bp, "settings_file", None);
    }

    Ok(result)
}

#[tauri::command]
pub fn backup_claude_code_config(state: State<'_, AppState>) -> Result<crate::storage::config_backups::ConfigBackup, AppError> {
    let backup_dir = crate::security::local_token::token_dir().join("backups");
    let result = crate::tools::claude_code::backup_config(&backup_dir)?;
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    let backup = storage::config_backups::insert(&conn, "claude_code", &result.source_path, &result.backup_path, "settings_file", None)?;
    Ok(backup)
}

#[tauri::command]
pub fn list_claude_code_backups(state: State<'_, AppState>) -> Result<Vec<crate::storage::config_backups::ConfigBackup>, AppError> {
    let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
    storage::config_backups::list_by_tool(&conn, "claude_code")
}

#[tauri::command]
pub fn restore_claude_code_backup(backup_id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let backup = {
        let conn = state.db.lock().map_err(|_| AppError::internal("DB lock failed"))?;
        storage::config_backups::get_by_id(&conn, &backup_id)?
    };
    let backup_dir = crate::security::local_token::token_dir().join("backups");
    crate::tools::claude_code::restore_config(&backup.backup_path, &backup_dir)?;
    Ok(true)
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
