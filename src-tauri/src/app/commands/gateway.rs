use tauri::{Emitter, State};

use crate::app::state::AppState;
use crate::errors::AppError;
use crate::gateway;
use crate::models::gateway::{GatewaySettings, GatewayStatus, UpdateGatewaySettingsInput};
use crate::storage;

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
