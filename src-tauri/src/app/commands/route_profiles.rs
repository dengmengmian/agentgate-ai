use tauri::State;

use crate::app::state::AppState;
use crate::errors::AppError;
use crate::storage;

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

