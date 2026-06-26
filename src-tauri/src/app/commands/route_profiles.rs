use tauri::State;

use crate::app::state::AppState;
use crate::errors::AppError;
use crate::storage;

// ── Route Profile Commands ─────────────────────────────────────

#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
pub fn delete_route_profile(id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::delete(&conn, &id)
}

#[tauri::command]
#[specta::specta]
pub fn set_default_route_profile(id: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::route_profiles::set_default(&conn, &id)?;
    Ok(true)
}

#[tauri::command]
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
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
#[specta::specta]
pub fn reset_all_provider_runtime_status(state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::provider_runtime_status::reset_all(&conn)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    use super::*;
    use crate::app::state::AppState;
    use crate::models::provider::CreateProviderInput;
    use crate::models::route_profile::{
        AddProviderToRouteInput, CreateRouteProfileInput, UpdateRouteProfileInput,
    };
    use crate::storage;

    fn test_state() -> AppState {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        {
            let conn = pool.get().unwrap();
            crate::storage::migrations::run_migrations(&conn).unwrap();
        }
        AppState {
            db: pool,
            gateway_runtime: Arc::new(Mutex::new(
                crate::models::gateway::GatewayRuntimeState::default(),
            )),
            pet_click_through: Arc::new(Mutex::new(false)),
        }
    }

    unsafe fn as_state<'r>(state: &'r AppState) -> tauri::State<'r, AppState> {
        std::mem::transmute(state)
    }

    fn create_test_provider(state: &AppState) -> String {
        let conn = state.db.get().unwrap();
        let provider = storage::providers::create(
            &conn,
            CreateProviderInput {
                name: "RouteTestProvider".to_string(),
                provider_type: "openai".to_string(),
                base_url: "https://api.openai.com".to_string(),
                api_key: Some("sk-test".to_string()),
                default_model: "gpt-4".to_string(),
                protocol: r#"["openai_chat_completions"]"#.to_string(),
                timeout_seconds: Some(120),
                ..Default::default()
            },
        )
        .unwrap();
        provider.id
    }

    fn create_test_profile(state: &AppState) -> crate::models::route_profile::RouteProfileView {
        create_route_profile(
            CreateRouteProfileInput {
                name: "TestProfile".to_string(),
                input_protocol: "openai_chat_completions".to_string(),
                mode: Some("manual".to_string()),
            },
            unsafe { as_state(state) },
        )
        .unwrap()
    }

    #[test]
    fn list_route_profiles_includes_defaults() {
        let state = test_state();
        let profiles = list_route_profiles(unsafe { as_state(&state) }).unwrap();
        assert!(!profiles.is_empty());
    }

    #[test]
    fn create_route_profile_persists() {
        let state = test_state();
        let profile = create_test_profile(&state);
        assert_eq!(profile.name, "TestProfile");
        assert_eq!(profile.input_protocol, "openai_chat_completions");
        assert_eq!(profile.mode, "manual");
    }

    #[test]
    fn get_route_profile_returns_profile_and_providers() {
        let state = test_state();
        let profile = create_test_profile(&state);
        let detail = get_route_profile(profile.id.clone(), unsafe { as_state(&state) }).unwrap();
        assert_eq!(detail.profile.id, profile.id);
        assert!(detail.providers.is_empty());
    }

    #[test]
    fn update_route_profile_changes_name() {
        let state = test_state();
        let profile = create_test_profile(&state);
        let updated = update_route_profile(
            profile.id.clone(),
            UpdateRouteProfileInput {
                name: Some("Renamed".to_string()),
                mode: None,
                selection_strategy: None,
                enabled: None,
            },
            unsafe { as_state(&state) },
        )
        .unwrap();
        assert_eq!(updated.name, "Renamed");

        let detail = get_route_profile(profile.id, unsafe { as_state(&state) }).unwrap();
        assert_eq!(detail.profile.name, "Renamed");
    }

    #[test]
    fn delete_route_profile_removes_non_default() {
        let state = test_state();
        let profile = create_test_profile(&state);
        let deleted =
            delete_route_profile(profile.id.clone(), unsafe { as_state(&state) }).unwrap();
        assert!(deleted);
        let err = get_route_profile(profile.id, unsafe { as_state(&state) }).unwrap_err();
        assert_eq!(err.code, "ROUTE_PROFILE_NOT_FOUND");
    }

    #[test]
    fn delete_route_profile_rejects_default() {
        let state = test_state();
        let profile = create_test_profile(&state);
        set_default_route_profile(profile.id.clone(), unsafe { as_state(&state) }).unwrap();
        let err = delete_route_profile(profile.id, unsafe { as_state(&state) }).unwrap_err();
        assert_eq!(err.code, "ROUTE_PROFILE_DELETE_DEFAULT_FORBIDDEN");
    }

    #[test]
    fn set_default_route_profile_marks_default() {
        let state = test_state();
        let profile = create_test_profile(&state);
        set_default_route_profile(profile.id.clone(), unsafe { as_state(&state) }).unwrap();
        let detail = get_route_profile(profile.id, unsafe { as_state(&state) }).unwrap();
        assert!(detail.profile.is_default);
    }

    #[test]
    fn set_route_profile_mode_updates_mode() {
        let state = test_state();
        let profile = create_test_profile(&state);
        set_route_profile_mode(profile.id.clone(), "failover".to_string(), unsafe {
            as_state(&state)
        })
        .unwrap();
        let detail = get_route_profile(profile.id, unsafe { as_state(&state) }).unwrap();
        assert_eq!(detail.profile.mode, "failover");
    }

    #[test]
    fn add_and_remove_provider_from_route() {
        let state = test_state();
        let profile = create_test_profile(&state);
        let provider_id = create_test_provider(&state);

        add_provider_to_route(
            profile.id.clone(),
            provider_id.clone(),
            AddProviderToRouteInput {
                priority: Some(1),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
            unsafe { as_state(&state) },
        )
        .unwrap();

        let detail = get_route_profile(profile.id.clone(), unsafe { as_state(&state) }).unwrap();
        assert_eq!(detail.providers.len(), 1);
        assert_eq!(detail.providers[0].provider_id, provider_id);

        remove_provider_from_route(profile.id.clone(), provider_id.clone(), unsafe {
            as_state(&state)
        })
        .unwrap();
        let detail = get_route_profile(profile.id, unsafe { as_state(&state) }).unwrap();
        assert!(detail.providers.is_empty());
    }

    #[test]
    fn set_route_active_provider_syncs_provider() {
        let state = test_state();
        let profile = create_test_profile(&state);
        let provider_id = create_test_provider(&state);

        add_provider_to_route(
            profile.id.clone(),
            provider_id.clone(),
            AddProviderToRouteInput {
                priority: Some(1),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
            unsafe { as_state(&state) },
        )
        .unwrap();

        set_route_active_provider(profile.id.clone(), provider_id.clone(), unsafe {
            as_state(&state)
        })
        .unwrap();

        let detail = get_route_profile(profile.id, unsafe { as_state(&state) }).unwrap();
        assert_eq!(detail.profile.active_provider_id, Some(provider_id));
    }

    #[test]
    fn reorder_route_providers_changes_priority() {
        let state = test_state();
        let profile = create_test_profile(&state);
        let pid1 = create_test_provider(&state);
        let pid2 = create_test_provider(&state);

        add_provider_to_route(
            profile.id.clone(),
            pid1.clone(),
            AddProviderToRouteInput {
                priority: Some(1),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
            unsafe { as_state(&state) },
        )
        .unwrap();
        add_provider_to_route(
            profile.id.clone(),
            pid2.clone(),
            AddProviderToRouteInput {
                priority: Some(2),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
            unsafe { as_state(&state) },
        )
        .unwrap();

        reorder_route_providers(
            profile.id.clone(),
            vec![pid2.clone(), pid1.clone()],
            unsafe { as_state(&state) },
        )
        .unwrap();

        let detail = get_route_profile(profile.id, unsafe { as_state(&state) }).unwrap();
        assert_eq!(detail.providers[0].provider_id, pid2);
        assert_eq!(detail.providers[1].provider_id, pid1);
    }

    #[test]
    fn update_route_provider_conditions_persists() {
        let state = test_state();
        let profile = create_test_profile(&state);
        let provider_id = create_test_provider(&state);

        add_provider_to_route(
            profile.id.clone(),
            provider_id.clone(),
            AddProviderToRouteInput {
                priority: Some(1),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
            unsafe { as_state(&state) },
        )
        .unwrap();

        let cond = r#"{"has_images":true}"#;
        update_route_provider_conditions(
            profile.id.clone(),
            provider_id.clone(),
            Some(cond.to_string()),
            unsafe { as_state(&state) },
        )
        .unwrap();

        let detail = get_route_profile(profile.id, unsafe { as_state(&state) }).unwrap();
        assert_eq!(
            detail.providers[0].routing_conditions,
            Some(cond.to_string())
        );
    }

    #[test]
    fn reset_provider_runtime_status_resets_one() {
        let state = test_state();
        let provider_id = create_test_provider(&state);
        let status =
            reset_provider_runtime_status(provider_id.clone(), unsafe { as_state(&state) })
                .unwrap();
        assert_eq!(status.provider_id, provider_id);
        assert!(status.available);
    }

    #[test]
    fn reset_all_provider_runtime_status_clears_records() {
        let state = test_state();
        let provider_id = create_test_provider(&state);
        let _ = reset_provider_runtime_status(provider_id, unsafe { as_state(&state) }).unwrap();
        let before = list_provider_runtime_status(unsafe { as_state(&state) }).unwrap();
        assert!(!before.is_empty());

        reset_all_provider_runtime_status(unsafe { as_state(&state) }).unwrap();
        let after = list_provider_runtime_status(unsafe { as_state(&state) }).unwrap();
        assert!(after.iter().all(|s| s.consecutive_failures == 0));
    }
}
