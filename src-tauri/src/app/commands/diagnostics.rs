use tauri::State;

use crate::app::state::AppState;
use crate::errors::AppError;

// ── Diagnostics Commands ───────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn run_health_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::health_check(&state.db))
}

#[tauri::command]
#[specta::specta]
pub fn run_database_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::database_check(&state.db))
}

#[tauri::command]
#[specta::specta]
pub fn run_gateway_auth_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::gateway_auth_check(&state.db))
}

#[tauri::command]
#[specta::specta]
pub fn run_provider_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::provider_check(&state.db))
}

#[tauri::command]
#[specta::specta]
pub fn run_codex_config_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::codex_config_check(&state.db))
}

#[tauri::command]
#[specta::specta]
pub fn run_claude_code_config_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::claude_code_config_check(
        &state.db,
    ))
}

#[tauri::command]
#[specta::specta]
pub fn run_route_profile_check(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::CheckReport, AppError> {
    Ok(crate::diagnostics::checks::route_profile_check(&state.db))
}

#[tauri::command]
#[specta::specta]
pub fn run_full_self_test(
    state: State<'_, AppState>,
) -> Result<crate::diagnostics::report::FullSelfTestReport, AppError> {
    Ok(crate::diagnostics::checks::full_self_test(&state.db))
}

#[tauri::command]
#[specta::specta]
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
#[specta::specta]
pub fn open_app_data_dir() -> Result<bool, AppError> {
    let dir = crate::security::local_token::token_dir();
    open::that(&dir).map_err(|e| AppError::internal(format!("Cannot open: {e}")))?;
    Ok(true)
}

