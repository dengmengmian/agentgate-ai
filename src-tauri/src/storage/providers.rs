use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::provider::{CreateProviderInput, Provider, UpdateProviderInput};

pub fn list_all(conn: &Connection) -> Result<Vec<Provider>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, name, provider_type, base_url, api_key, default_model, reasoning_model, supported_models, model_mapping, extra_headers, anthropic_base_url, responses_base_url,
                protocol, timeout_seconds, status, supports_vision, auto_cache_control, supports_cache, enabled, is_active, created_at, updated_at
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
            enabled: row.get(18)?,
            is_active: row.get(19)?,
            created_at: row.get(20)?,
            updated_at: row.get(21)?,
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
                protocol, timeout_seconds, status, supports_vision, auto_cache_control, supports_cache, enabled, is_active, created_at, updated_at
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
                enabled: row.get(18)?,
                is_active: row.get(19)?,
                created_at: row.get(20)?,
                updated_at: row.get(21)?,
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
                                supported_models, model_mapping, extra_headers, anthropic_base_url, responses_base_url, protocol, timeout_seconds, status, auto_cache_control, enabled, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'not_tested', ?15, ?16, 0, ?17, ?17)",
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
    let enabled = input.enabled.unwrap_or(existing.enabled);

    conn.execute(
        "UPDATE providers SET name=?1, provider_type=?2, base_url=?3, api_key=?4, default_model=?5,
                reasoning_model=?6, supported_models=?7, model_mapping=?8, extra_headers=?9, anthropic_base_url=?10, responses_base_url=?11, protocol=?12, timeout_seconds=?13, auto_cache_control=?14, enabled=?15, updated_at=?16
         WHERE id=?17",
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
