use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::provider::{CreateProviderInput, Provider, UpdateProviderInput};

pub fn list_all(conn: &Connection) -> Result<Vec<Provider>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, provider_type, base_url, api_key, default_model, reasoning_model, supported_models, model_mapping, extra_headers, anthropic_base_url, responses_base_url,
                protocol, timeout_seconds, status, supports_vision, auto_cache_control, supports_cache, model_capabilities, enabled, is_active, created_at, updated_at
         FROM providers ORDER BY is_active DESC, created_at ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(Provider {
            id: row.get(0)?,
            name: row.get(1)?,
            provider_type: row.get(2)?,
            base_url: row.get(3)?,
            api_key: row.get(4)?,
            default_model: row.get(5)?,
            reasoning_model: row.get(6)?,
            supported_models: row.get(7)?,
            model_mapping: row.get(8)?,
            extra_headers: row.get(9)?,
            anthropic_base_url: row.get(10)?,
            responses_base_url: row.get(11)?,
            protocol: row.get(12)?,
            timeout_seconds: row.get(13)?,
            status: row.get(14)?,
            supports_vision: row.get(15)?,
            auto_cache_control: row.get(16)?,
            supports_cache: row.get(17)?,
            model_capabilities: row.get(18)?,
            enabled: row.get(19)?,
            is_active: row.get(20)?,
            created_at: row.get(21)?,
            updated_at: row.get(22)?,
        })
    })?;

    let mut providers = Vec::new();
    for row in rows {
        providers.push(row?);
    }
    Ok(providers)
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<Provider, AppError> {
    conn.query_row(
        "SELECT id, name, provider_type, base_url, api_key, default_model, reasoning_model, supported_models, model_mapping, extra_headers, anthropic_base_url, responses_base_url,
                protocol, timeout_seconds, status, supports_vision, auto_cache_control, supports_cache, model_capabilities, enabled, is_active, created_at, updated_at
         FROM providers WHERE id = ?1",
        [id],
        |row| {
            Ok(Provider {
                id: row.get(0)?,
                name: row.get(1)?,
                provider_type: row.get(2)?,
                base_url: row.get(3)?,
                api_key: row.get(4)?,
                default_model: row.get(5)?,
                reasoning_model: row.get(6)?,
                supported_models: row.get(7)?,
                model_mapping: row.get(8)?,
                extra_headers: row.get(9)?,
                anthropic_base_url: row.get(10)?,
                responses_base_url: row.get(11)?,
                protocol: row.get(12)?,
                timeout_seconds: row.get(13)?,
                status: row.get(14)?,
                supports_vision: row.get(15)?,
                auto_cache_control: row.get(16)?,
                supports_cache: row.get(17)?,
                model_capabilities: row.get(18)?,
                enabled: row.get(19)?,
                is_active: row.get(20)?,
                created_at: row.get(21)?,
                updated_at: row.get(22)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found("Provider", id),
        other => AppError::database(other),
    })
}

pub fn create(conn: &Connection, input: CreateProviderInput) -> Result<Provider, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let timeout = input.timeout_seconds.unwrap_or(120);
    let enabled = input.enabled.unwrap_or(true);

    conn.execute(
        "INSERT INTO providers (id, name, provider_type, base_url, api_key, default_model, reasoning_model,
                                supported_models, model_mapping, extra_headers, anthropic_base_url, responses_base_url, protocol, timeout_seconds, status, auto_cache_control, model_capabilities, enabled, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'not_tested', ?15, ?16, ?17, 0, ?18, ?18)",
        params![
            &id,
            &input.name,
            &input.provider_type,
            &input.base_url,
            &input.api_key,
            &input.default_model,
            &input.reasoning_model,
            &input.supported_models,
            &input.model_mapping,
            &input.extra_headers,
            &input.anthropic_base_url,
            &input.responses_base_url,
            &input.protocol,
            timeout,
            &input.auto_cache_control,
            &input.model_capabilities,
            enabled,
            &now,
        ],
    )?;

    get_by_id(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: UpdateProviderInput) -> Result<Provider, AppError> {
    let existing = get_by_id(conn, id)?;
    let now = chrono::Utc::now().to_rfc3339();

    let name = input.name.unwrap_or(existing.name);
    let provider_type = input.provider_type.unwrap_or(existing.provider_type);
    let base_url = input.base_url.unwrap_or(existing.base_url);
    let api_key = match input.api_key {
        Some(k) => Some(k),
        None => existing.api_key,
    };
    let default_model = input.default_model.unwrap_or(existing.default_model);
    let reasoning_model = input.reasoning_model.or(existing.reasoning_model);
    let supported_models = input.supported_models.or(existing.supported_models);
    let model_mapping = input.model_mapping.or(existing.model_mapping);
    let extra_headers = input.extra_headers.or(existing.extra_headers);
    let anthropic_base_url = input.anthropic_base_url.or(existing.anthropic_base_url);
    let responses_base_url = input.responses_base_url.or(existing.responses_base_url);
    let protocol = input.protocol.unwrap_or(existing.protocol);
    let timeout_seconds = input.timeout_seconds.unwrap_or(existing.timeout_seconds);
    let auto_cache_control = input.auto_cache_control.or(existing.auto_cache_control);
    let model_capabilities = input.model_capabilities.or(existing.model_capabilities);
    let enabled = input.enabled.unwrap_or(existing.enabled);

    conn.execute(
        "UPDATE providers SET name=?1, provider_type=?2, base_url=?3, api_key=?4, default_model=?5,
                reasoning_model=?6, supported_models=?7, model_mapping=?8, extra_headers=?9, anthropic_base_url=?10, responses_base_url=?11, protocol=?12, timeout_seconds=?13, auto_cache_control=?14, model_capabilities=?15, enabled=?16, updated_at=?17
         WHERE id=?18",
        params![
            &name,
            &provider_type,
            &base_url,
            &api_key,
            &default_model,
            &reasoning_model,
            &supported_models,
            &model_mapping,
            &extra_headers,
            &anthropic_base_url,
            &responses_base_url,
            &protocol,
            timeout_seconds,
            auto_cache_control,
            &model_capabilities,
            enabled,
            &now,
            id,
        ],
    )?;

    get_by_id(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool, AppError> {
    let provider = get_by_id(conn, id)?;
    let was_active = provider.is_active;

    conn.execute("DELETE FROM providers WHERE id = ?1", [id])?;

    // Clean up route_profile_providers references to this provider
    conn.execute("DELETE FROM route_profile_providers WHERE provider_id = ?1", [id])?;

    if was_active {
        // Set next enabled provider as active
        let next_id: Option<String> = conn
            .query_row(
                "SELECT id FROM providers WHERE enabled = 1 ORDER BY created_at ASC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        if let Some(next) = &next_id {
            conn.execute(
                "UPDATE providers SET is_active = 1, updated_at = ?1 WHERE id = ?2",
                params![chrono::Utc::now().to_rfc3339(), next],
            )?;
        }

        // Update gateway_settings
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE gateway_settings SET active_provider_id = ?1, updated_at = ?2 WHERE id = 1",
            params![next_id, &now],
        )?;
    }

    Ok(true)
}

pub fn set_active(conn: &Connection, id: &str) -> Result<Provider, AppError> {
    let _provider = get_by_id(conn, id)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Clear all active
    conn.execute(
        "UPDATE providers SET is_active = 0, updated_at = ?1 WHERE is_active = 1",
        [&now],
    )?;

    // Set new active
    conn.execute(
        "UPDATE providers SET is_active = 1, updated_at = ?1 WHERE id = ?2",
        params![&now, id],
    )?;

    // Sync gateway_settings
    conn.execute(
        "UPDATE gateway_settings SET active_provider_id = ?1, updated_at = ?2 WHERE id = 1",
        params![id, &now],
    )?;

    // Sync all default route profiles
    conn.execute(
        "UPDATE route_profiles SET active_provider_id = ?1, updated_at = ?2 WHERE is_default = 1",
        params![id, &now],
    )?;

    get_by_id(conn, id)
}

pub fn update_status(conn: &Connection, id: &str, status: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE providers SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, &now, id],
    )?;
    Ok(())
}

pub fn update_supports_vision(conn: &Connection, id: &str, supports_vision: bool) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE providers SET supports_vision = ?1, updated_at = ?2 WHERE id = ?3",
        params![supports_vision, &now, id],
    )?;
    Ok(())
}

pub fn update_supports_cache(conn: &Connection, id: &str, supports_cache: bool) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE providers SET supports_cache = ?1, updated_at = ?2 WHERE id = ?3",
        params![supports_cache, &now, id],
    )?;
    Ok(())
}

pub fn update_model_capabilities(conn: &Connection, id: &str, matrix_json: &str) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE providers SET model_capabilities = ?1, updated_at = ?2 WHERE id = ?3",
        params![matrix_json, &now, id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::provider::{CreateProviderInput, UpdateProviderInput};

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&conn).unwrap();
        conn
    }

    fn create_test_provider(conn: &Connection, name: &str) -> Provider {
        create(conn, CreateProviderInput {
            name: name.to_string(),
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
            enabled: Some(true),
        }).unwrap()
    }

    #[test]
    fn test_list_all_empty() {
        let conn = setup_db();
        let providers = list_all(&conn).unwrap();
        // After migrations seed_default_providers inserts 2 defaults
        assert!(providers.len() >= 2);
    }

    #[test]
    fn test_create_and_get_provider() {
        let conn = setup_db();
        let p = create_test_provider(&conn, "TestProvider");
        assert_eq!(p.name, "TestProvider");
        assert_eq!(p.provider_type, "openai");

        let fetched = get_by_id(&conn, &p.id).unwrap();
        assert_eq!(fetched.id, p.id);
        assert_eq!(fetched.name, "TestProvider");
    }

    #[test]
    fn test_get_by_id_not_found() {
        let conn = setup_db();
        let err = get_by_id(&conn, "nonexistent").unwrap_err();
        assert_eq!(err.code, "NOT_FOUND");
    }

    #[test]
    fn test_update_provider() {
        let conn = setup_db();
        let p = create_test_provider(&conn, "Original");
        let updated = update(&conn, &p.id, UpdateProviderInput {
            name: Some("Updated".to_string()),
            provider_type: None,
            base_url: None,
            api_key: None,
            default_model: None,
            reasoning_model: None,
            supported_models: None,
            model_mapping: None,
            extra_headers: None,
            anthropic_base_url: None,
            responses_base_url: None,
            auto_cache_control: None,
            model_capabilities: None,
            protocol: None,
            timeout_seconds: None,
            enabled: None,
        }).unwrap();
        assert_eq!(updated.name, "Updated");
    }

    #[test]
    fn test_delete_provider() {
        let conn = setup_db();
        let p = create_test_provider(&conn, "ToDelete");
        let result = delete(&conn, &p.id).unwrap();
        assert!(result);
        assert!(get_by_id(&conn, &p.id).is_err());
    }

    #[test]
    fn test_set_active_provider() {
        let conn = setup_db();
        let p1 = create_test_provider(&conn, "P1");
        let p2 = create_test_provider(&conn, "P2");
        set_active(&conn, &p1.id).unwrap();
        let active1 = get_by_id(&conn, &p1.id).unwrap();
        assert!(active1.is_active);

        set_active(&conn, &p2.id).unwrap();
        let active2 = get_by_id(&conn, &p2.id).unwrap();
        assert!(active2.is_active);
        let inactive1 = get_by_id(&conn, &p1.id).unwrap();
        assert!(!inactive1.is_active);
    }

    #[test]
    fn test_update_status() {
        let conn = setup_db();
        let p = create_test_provider(&conn, "StatusTest");
        update_status(&conn, &p.id, "ok").unwrap();
        let updated = get_by_id(&conn, &p.id).unwrap();
        assert_eq!(updated.status, "ok");
    }

    #[test]
    fn test_update_supports_vision() {
        let conn = setup_db();
        let p = create_test_provider(&conn, "VisionTest");
        update_supports_vision(&conn, &p.id, true).unwrap();
        let updated = get_by_id(&conn, &p.id).unwrap();
        assert_eq!(updated.supports_vision, Some(true));
    }

    #[test]
    fn test_update_supports_cache() {
        let conn = setup_db();
        let p = create_test_provider(&conn, "CacheTest");
        update_supports_cache(&conn, &p.id, true).unwrap();
        let updated = get_by_id(&conn, &p.id).unwrap();
        assert_eq!(updated.supports_cache, Some(true));
    }

    #[test]
    fn test_update_model_capabilities() {
        let conn = setup_db();
        let p = create_test_provider(&conn, "CapTest");
        let matrix = r#"{"gpt-4":["text","vision"]}"#;
        update_model_capabilities(&conn, &p.id, matrix).unwrap();
        let updated = get_by_id(&conn, &p.id).unwrap();
        assert_eq!(updated.model_capabilities, Some(matrix.to_string()));
    }
}
