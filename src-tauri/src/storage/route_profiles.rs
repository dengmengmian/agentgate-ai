use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::route_profile::*;

pub fn list_all(conn: &Connection) -> Result<Vec<RouteProfileView>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rp.id, rp.name, rp.input_protocol, rp.mode,
                rp.active_provider_id, rp.enabled, rp.is_default, rp.created_at, rp.updated_at,
                p.name as provider_name,
                (SELECT COUNT(*) FROM route_profile_providers WHERE route_profile_id = rp.id) as cnt,
                rp.selection_strategy
         FROM route_profiles rp
         LEFT JOIN providers p ON p.id = rp.active_provider_id
         ORDER BY rp.is_default DESC, rp.created_at ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(RouteProfileView {
            id: row.get(0)?,
            name: row.get(1)?,
            input_protocol: row.get(2)?,
            mode: row.get(3)?,
            active_provider_id: row.get(4)?,
            enabled: row.get(5)?,
            is_default: row.get(6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
            active_provider_name: row.get(9)?,
            providers_count: row.get(10)?,
            selection_strategy: row.get(11)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<RouteProfile, AppError> {
    conn.query_row(
        "SELECT id, name, input_protocol, mode, active_provider_id, enabled, is_default, created_at, updated_at, selection_strategy
         FROM route_profiles WHERE id = ?1",
        [id],
        |row| Ok(RouteProfile {
            id: row.get(0)?, name: row.get(1)?,
            input_protocol: row.get(2)?, mode: row.get(3)?, active_provider_id: row.get(4)?,
            enabled: row.get(5)?, is_default: row.get(6)?, created_at: row.get(7)?, updated_at: row.get(8)?,
            selection_strategy: row.get(9)?,
        }),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::new(crate::errors::codes::ROUTE_PROFILE_NOT_FOUND, format!("Route profile '{id}' not found")),
        other => AppError::database(other),
    })
}

pub fn get_default_for_protocol(
    conn: &Connection,
    input_protocol: &str,
) -> Result<Option<RouteProfile>, AppError> {
    let result = conn.query_row(
        "SELECT id, name, input_protocol, mode, active_provider_id, enabled, is_default, created_at, updated_at, selection_strategy
         FROM route_profiles WHERE is_default = 1 AND input_protocol = ?1 AND enabled = 1 LIMIT 1",
        [input_protocol],
        |row| Ok(RouteProfile {
            id: row.get(0)?, name: row.get(1)?,
            input_protocol: row.get(2)?, mode: row.get(3)?, active_provider_id: row.get(4)?,
            enabled: row.get(5)?, is_default: row.get(6)?, created_at: row.get(7)?, updated_at: row.get(8)?,
            selection_strategy: row.get(9)?,
        }),
    );
    match result {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(AppError::database(e)),
    }
}

pub fn create(conn: &Connection, input: CreateRouteProfileInput) -> Result<RouteProfile, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let mode = input.mode.unwrap_or_else(|| "manual".to_string());

    conn.execute(
        "INSERT INTO route_profiles (id, name, client_type, input_protocol, mode, enabled, is_default, created_at, updated_at)
         VALUES (?1, ?2, '', ?3, ?4, 1, 0, ?5, ?5)",
        params![&id, &input.name, &input.input_protocol, &mode, &now],
    )?;
    get_by_id(conn, &id)
}

pub fn update(
    conn: &Connection,
    id: &str,
    input: UpdateRouteProfileInput,
) -> Result<RouteProfile, AppError> {
    let existing = get_by_id(conn, id)?;
    let now = chrono::Utc::now().to_rfc3339();
    let name = input.name.unwrap_or(existing.name);
    let mode = input.mode.unwrap_or(existing.mode);
    let selection_strategy = input
        .selection_strategy
        .unwrap_or(existing.selection_strategy);
    let enabled = input.enabled.unwrap_or(existing.enabled);

    conn.execute(
        "UPDATE route_profiles SET name=?1, mode=?2, selection_strategy=?3, enabled=?4, updated_at=?5 WHERE id=?6",
        params![&name, &mode, &selection_strategy, enabled, &now, id],
    )?;
    get_by_id(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool, AppError> {
    let profile = get_by_id(conn, id)?;
    if profile.is_default {
        return Err(AppError::new(
            crate::errors::codes::ROUTE_PROFILE_DELETE_DEFAULT_FORBIDDEN,
            "Cannot delete the default route profile",
        ));
    }
    conn.execute(
        "DELETE FROM route_profile_providers WHERE route_profile_id = ?1",
        [id],
    )?;
    conn.execute("DELETE FROM route_profiles WHERE id = ?1", [id])?;
    Ok(true)
}

pub fn set_default(conn: &Connection, id: &str) -> Result<RouteProfile, AppError> {
    let profile = get_by_id(conn, id)?;
    let now = chrono::Utc::now().to_rfc3339();
    // Clear default for same input_protocol
    conn.execute(
        "UPDATE route_profiles SET is_default = 0, updated_at = ?1 WHERE input_protocol = ?2",
        params![&now, &profile.input_protocol],
    )?;
    conn.execute(
        "UPDATE route_profiles SET is_default = 1, updated_at = ?1 WHERE id = ?2",
        params![&now, id],
    )?;
    get_by_id(conn, id)
}

pub fn set_active_provider(
    conn: &Connection,
    profile_id: &str,
    provider_id: &str,
) -> Result<RouteProfile, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE route_profiles SET active_provider_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![provider_id, &now, profile_id],
    )?;

    // Sync providers.is_active + gateway_settings
    conn.execute(
        "UPDATE providers SET is_active = 0, updated_at = ?1 WHERE is_active = 1",
        [&now],
    )?;
    conn.execute(
        "UPDATE providers SET is_active = 1, updated_at = ?1 WHERE id = ?2",
        params![&now, provider_id],
    )?;
    conn.execute(
        "UPDATE gateway_settings SET active_provider_id = ?1, updated_at = ?2 WHERE id = 1",
        params![provider_id, &now],
    )?;

    get_by_id(conn, profile_id)
}

// ── Route Profile Providers ──

pub fn list_providers(
    conn: &Connection,
    profile_id: &str,
) -> Result<Vec<RouteProfileProviderView>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rpp.id, rpp.provider_id, p.name, p.provider_type, p.protocol,
                CASE WHEN p.anthropic_base_url IS NOT NULL AND p.anthropic_base_url != '' THEN 1 ELSE 0 END,
                p.supports_vision, p.model_capabilities,
                rpp.priority, rpp.enabled,
                rpp.model_override, rpp.cooldown_seconds,
                rpp.failover_on_status_codes, rpp.failover_on_error_keywords, rpp.routing_conditions,
                COALESCE(prs.available, 1), prs.cooldown_until, COALESCE(prs.consecutive_failures, 0)
         FROM route_profile_providers rpp
         JOIN providers p ON p.id = rpp.provider_id
         LEFT JOIN provider_runtime_status prs ON prs.provider_id = rpp.provider_id
         WHERE rpp.route_profile_id = ?1
         ORDER BY rpp.priority ASC",
    )?;

    let rows = stmt.query_map([profile_id], |row| {
        Ok(RouteProfileProviderView {
            id: row.get(0)?,
            provider_id: row.get(1)?,
            provider_name: row.get(2)?,
            provider_type: row.get(3)?,
            provider_protocol: row.get(4)?,
            has_anthropic_url: row.get(5)?,
            supports_vision: row.get(6)?,
            model_capabilities: row.get(7)?,
            priority: row.get(8)?,
            enabled: row.get(9)?,
            model_override: row.get(10)?,
            cooldown_seconds: row.get(11)?,
            failover_on_status_codes: row.get(12)?,
            failover_on_error_keywords: row.get(13)?,
            routing_conditions: row.get(14)?,
            runtime_available: row.get(15)?,
            cooldown_until: row.get(16)?,
            consecutive_failures: row.get(17)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

pub fn add_provider(
    conn: &Connection,
    profile_id: &str,
    provider_id: &str,
    input: AddProviderToRouteInput,
) -> Result<(), AppError> {
    let dup: i64 = conn.query_row(
        "SELECT COUNT(*) FROM route_profile_providers WHERE route_profile_id=?1 AND provider_id=?2",
        params![profile_id, provider_id],
        |row| row.get(0),
    )?;
    if dup > 0 {
        return Err(AppError::new(
            crate::errors::codes::ROUTE_PROVIDER_DUPLICATED,
            "Provider is already in this route profile",
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let priority = input.priority.unwrap_or_else(|| {
        let max: i64 = conn.query_row(
            "SELECT COALESCE(MAX(priority), 0) FROM route_profile_providers WHERE route_profile_id=?1",
            [profile_id], |row| row.get(0),
        ).unwrap_or(0);
        max + 1
    });

    let default_codes = serde_json::json!([402, 429, 500, 502, 503, 504]).to_string();
    let default_kw = serde_json::json!([
        "quota",
        "insufficient balance",
        "rate limit",
        "too many requests",
        "timeout"
    ])
    .to_string();

    conn.execute(
        "INSERT INTO route_profile_providers (id, route_profile_id, provider_id, priority, enabled, model_override, cooldown_seconds, failover_on_status_codes, failover_on_error_keywords, routing_conditions, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            uuid::Uuid::new_v4().to_string(), profile_id, provider_id, priority,
            input.model_override, input.cooldown_seconds.unwrap_or(600),
            input.failover_on_status_codes.as_deref().unwrap_or(&default_codes),
            input.failover_on_error_keywords.as_deref().unwrap_or(&default_kw),
            input.routing_conditions,
            &now,
        ],
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO provider_runtime_status (provider_id, available, consecutive_failures, quota_exhausted, updated_at) VALUES (?1, 1, 0, 0, ?2)",
        params![provider_id, &now],
    )?;

    Ok(())
}

pub fn remove_provider(
    conn: &Connection,
    profile_id: &str,
    provider_id: &str,
) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM route_profile_providers WHERE route_profile_id=?1 AND provider_id=?2",
        params![profile_id, provider_id],
    )?;
    Ok(())
}

pub fn reorder_providers(
    conn: &Connection,
    profile_id: &str,
    provider_ids: &[String],
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    for (i, pid) in provider_ids.iter().enumerate() {
        conn.execute(
            "UPDATE route_profile_providers SET priority=?1, updated_at=?2 WHERE route_profile_id=?3 AND provider_id=?4",
            params![(i + 1) as i64, &now, profile_id, pid],
        )?;
    }
    Ok(())
}

pub fn update_provider_conditions(
    conn: &Connection,
    profile_id: &str,
    provider_id: &str,
    routing_conditions: Option<&str>,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE route_profile_providers SET routing_conditions=?1, updated_at=?2 WHERE route_profile_id=?3 AND provider_id=?4",
        params![routing_conditions, &now, profile_id, provider_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::CreateProviderInput;
    use crate::models::route_profile::{
        AddProviderToRouteInput, CreateRouteProfileInput, UpdateRouteProfileInput,
    };
    use crate::storage::providers;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&conn).unwrap();
        conn
    }

    fn create_test_provider(conn: &Connection) -> String {
        let p = providers::create(
            conn,
            CreateProviderInput {
                name: "TestProvider".to_string(),
                provider_type: "openai".to_string(),
                base_url: "https://api.openai.com".to_string(),
                api_key: Some("sk-test".to_string()),
                default_model: "gpt-4".to_string(),
                reasoning_model: None,
                supported_models: None,
                model_mapping: None,
                extra_headers: None,
                anthropic_base_url: None,
                responses_base_url: None,
                protocol: r#"["openai_chat_completions"]"#.to_string(),
                timeout_seconds: Some(120),
                auto_cache_control: None,
                model_capabilities: None,
                provider_quirks: None,
                body_filter_enabled: None,
                thinking_rectifier_enabled: None,
                error_mapper_enabled: None,
                model_degradation_chain: None,
                enabled: Some(true),
            },
        )
        .unwrap();
        p.id
    }

    #[test]
    fn test_create_and_get_route_profile() {
        let conn = setup_db();
        let profile = create(
            &conn,
            CreateRouteProfileInput {
                name: "TestProfile".to_string(),
                input_protocol: "openai_responses".to_string(),
                mode: Some("manual".to_string()),
            },
        )
        .unwrap();
        assert_eq!(profile.name, "TestProfile");
        assert_eq!(profile.input_protocol, "openai_responses");
        assert_eq!(profile.mode, "manual");

        let fetched = get_by_id(&conn, &profile.id).unwrap();
        assert_eq!(fetched.id, profile.id);
    }

    #[test]
    fn test_update_route_profile() {
        let conn = setup_db();
        let profile = create(
            &conn,
            CreateRouteProfileInput {
                name: "Original".to_string(),
                input_protocol: "openai_responses".to_string(),
                mode: None,
            },
        )
        .unwrap();
        let updated = update(
            &conn,
            &profile.id,
            UpdateRouteProfileInput {
                name: Some("Updated".to_string()),
                mode: None,
                selection_strategy: None,
                enabled: None,
            },
        )
        .unwrap();
        assert_eq!(updated.name, "Updated");
    }

    #[test]
    fn test_set_default_route_profile() {
        let conn = setup_db();
        let p1 = create(
            &conn,
            CreateRouteProfileInput {
                name: "P1".to_string(),
                input_protocol: "openai_responses".to_string(),
                mode: None,
            },
        )
        .unwrap();
        let default = set_default(&conn, &p1.id).unwrap();
        assert!(default.is_default);
    }

    #[test]
    fn test_delete_route_profile_prevents_default() {
        let conn = setup_db();
        let profile = create(
            &conn,
            CreateRouteProfileInput {
                name: "ToDelete".to_string(),
                input_protocol: "openai_responses".to_string(),
                mode: None,
            },
        )
        .unwrap();
        let _ = set_default(&conn, &profile.id);
        let err = delete(&conn, &profile.id).unwrap_err();
        assert_eq!(err.code, "ROUTE_PROFILE_DELETE_DEFAULT_FORBIDDEN");
    }

    #[test]
    fn test_add_and_remove_provider_from_route() {
        let conn = setup_db();
        let provider_id = create_test_provider(&conn);
        let profile = create(
            &conn,
            CreateRouteProfileInput {
                name: "WithProvider".to_string(),
                input_protocol: "openai_responses".to_string(),
                mode: None,
            },
        )
        .unwrap();

        add_provider(
            &conn,
            &profile.id,
            &provider_id,
            AddProviderToRouteInput {
                priority: Some(1),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
        )
        .unwrap();

        let providers = list_providers(&conn, &profile.id).unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_id, provider_id);

        remove_provider(&conn, &profile.id, &provider_id).unwrap();
        let providers_after = list_providers(&conn, &profile.id).unwrap();
        assert!(providers_after.is_empty());
    }

    #[test]
    fn test_add_provider_duplicate_fails() {
        let conn = setup_db();
        let provider_id = create_test_provider(&conn);
        let profile = create(
            &conn,
            CreateRouteProfileInput {
                name: "DupTest".to_string(),
                input_protocol: "openai_responses".to_string(),
                mode: None,
            },
        )
        .unwrap();

        add_provider(
            &conn,
            &profile.id,
            &provider_id,
            AddProviderToRouteInput {
                priority: Some(1),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
        )
        .unwrap();

        let err = add_provider(
            &conn,
            &profile.id,
            &provider_id,
            AddProviderToRouteInput {
                priority: Some(2),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "ROUTE_PROVIDER_DUPLICATED");
    }

    #[test]
    fn test_reorder_providers() {
        let conn = setup_db();
        let pid1 = create_test_provider(&conn);
        let pid2 = create_test_provider(&conn);
        let profile = create(
            &conn,
            CreateRouteProfileInput {
                name: "Reorder".to_string(),
                input_protocol: "openai_responses".to_string(),
                mode: None,
            },
        )
        .unwrap();

        add_provider(
            &conn,
            &profile.id,
            &pid1,
            AddProviderToRouteInput {
                priority: Some(1),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
        )
        .unwrap();
        add_provider(
            &conn,
            &profile.id,
            &pid2,
            AddProviderToRouteInput {
                priority: Some(2),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
        )
        .unwrap();

        reorder_providers(&conn, &profile.id, &[pid2.clone(), pid1.clone()]).unwrap();
        let providers = list_providers(&conn, &profile.id).unwrap();
        assert_eq!(providers[0].provider_id, pid2);
        assert_eq!(providers[1].provider_id, pid1);
    }

    #[test]
    fn test_update_provider_conditions() {
        let conn = setup_db();
        let provider_id = create_test_provider(&conn);
        let profile = create(
            &conn,
            CreateRouteProfileInput {
                name: "Conditions".to_string(),
                input_protocol: "openai_responses".to_string(),
                mode: None,
            },
        )
        .unwrap();

        add_provider(
            &conn,
            &profile.id,
            &provider_id,
            AddProviderToRouteInput {
                priority: Some(1),
                model_override: None,
                cooldown_seconds: None,
                failover_on_status_codes: None,
                failover_on_error_keywords: None,
                routing_conditions: None,
            },
        )
        .unwrap();

        let cond = r#"{"has_images":true}"#;
        update_provider_conditions(&conn, &profile.id, &provider_id, Some(cond)).unwrap();
        let providers = list_providers(&conn, &profile.id).unwrap();
        assert_eq!(providers[0].routing_conditions, Some(cond.to_string()));
    }

    #[test]
    fn test_get_default_for_protocol() {
        let conn = setup_db();
        // Migrations seed default profiles; we should find one for openai_responses
        let default = get_default_for_protocol(&conn, "openai_responses").unwrap();
        assert!(default.is_some());
    }
}
