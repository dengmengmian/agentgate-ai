use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::route_profile::*;

pub fn list_all(conn: &Connection) -> Result<Vec<RouteProfileView>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rp.id, rp.name, rp.client_type, rp.input_protocol, rp.mode,
                rp.active_provider_id, rp.enabled, rp.is_default, rp.created_at, rp.updated_at,
                p.name as provider_name,
                (SELECT COUNT(*) FROM route_profile_providers WHERE route_profile_id = rp.id) as cnt
         FROM route_profiles rp
         LEFT JOIN providers p ON p.id = rp.active_provider_id
         ORDER BY rp.is_default DESC, rp.created_at ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(RouteProfileView {
            id: row.get(0)?,
            name: row.get(1)?,
            client_type: row.get(2)?,
            input_protocol: row.get(3)?,
            mode: row.get(4)?,
            active_provider_id: row.get(5)?,
            enabled: row.get(6)?,
            is_default: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            active_provider_name: row.get(10)?,
            providers_count: row.get(11)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

pub fn get_by_id(conn: &Connection, id: &str) -> Result<RouteProfile, AppError> {
    conn.query_row(
        "SELECT id, name, client_type, input_protocol, mode, active_provider_id, enabled, is_default, created_at, updated_at
         FROM route_profiles WHERE id = ?1",
        [id],
        |row| Ok(RouteProfile {
            id: row.get(0)?, name: row.get(1)?, client_type: row.get(2)?,
            input_protocol: row.get(3)?, mode: row.get(4)?, active_provider_id: row.get(5)?,
            enabled: row.get(6)?, is_default: row.get(7)?, created_at: row.get(8)?, updated_at: row.get(9)?,
        }),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::new("ROUTE_PROFILE_NOT_FOUND", format!("Route profile '{id}' not found")),
        other => AppError::database(other),
    })
}

pub fn get_default_for_protocol(conn: &Connection, input_protocol: &str) -> Result<Option<RouteProfile>, AppError> {
    let result = conn.query_row(
        "SELECT id, name, client_type, input_protocol, mode, active_provider_id, enabled, is_default, created_at, updated_at
         FROM route_profiles WHERE is_default = 1 AND input_protocol = ?1 AND enabled = 1 LIMIT 1",
        [input_protocol],
        |row| Ok(RouteProfile {
            id: row.get(0)?, name: row.get(1)?, client_type: row.get(2)?,
            input_protocol: row.get(3)?, mode: row.get(4)?, active_provider_id: row.get(5)?,
            enabled: row.get(6)?, is_default: row.get(7)?, created_at: row.get(8)?, updated_at: row.get(9)?,
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
         VALUES (?1, ?2, ?3, ?4, ?5, 1, 0, ?6, ?6)",
        params![&id, &input.name, &input.client_type, &input.input_protocol, &mode, &now],
    )?;
    get_by_id(conn, &id)
}

pub fn update(conn: &Connection, id: &str, input: UpdateRouteProfileInput) -> Result<RouteProfile, AppError> {
    let existing = get_by_id(conn, id)?;
    let now = chrono::Utc::now().to_rfc3339();
    let name = input.name.unwrap_or(existing.name);
    let mode = input.mode.unwrap_or(existing.mode);
    let enabled = input.enabled.unwrap_or(existing.enabled);

    conn.execute(
        "UPDATE route_profiles SET name=?1, mode=?2, enabled=?3, updated_at=?4 WHERE id=?5",
        params![&name, &mode, enabled, &now, id],
    )?;
    get_by_id(conn, id)
}

pub fn delete(conn: &Connection, id: &str) -> Result<bool, AppError> {
    let profile = get_by_id(conn, id)?;
    if profile.is_default {
        return Err(AppError::new("ROUTE_PROFILE_DELETE_DEFAULT_FORBIDDEN", "Cannot delete the default route profile"));
    }
    conn.execute("DELETE FROM route_profile_providers WHERE route_profile_id = ?1", [id])?;
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

pub fn set_active_provider(conn: &Connection, profile_id: &str, provider_id: &str) -> Result<RouteProfile, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE route_profiles SET active_provider_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![provider_id, &now, profile_id],
    )?;

    // Sync providers.is_active + gateway_settings
    conn.execute("UPDATE providers SET is_active = 0, updated_at = ?1 WHERE is_active = 1", [&now])?;
    conn.execute("UPDATE providers SET is_active = 1, updated_at = ?1 WHERE id = ?2", params![&now, provider_id])?;
    conn.execute("UPDATE gateway_settings SET active_provider_id = ?1, updated_at = ?2 WHERE id = 1", params![provider_id, &now])?;

    get_by_id(conn, profile_id)
}

// ── Route Profile Providers ──

pub fn list_providers(conn: &Connection, profile_id: &str) -> Result<Vec<RouteProfileProviderView>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rpp.id, rpp.provider_id, p.name, p.provider_type, rpp.priority, rpp.enabled,
                rpp.model_override, rpp.max_retries, rpp.cooldown_seconds,
                rpp.failover_on_status_codes, rpp.failover_on_error_keywords,
                COALESCE(prs.available, 1), prs.cooldown_until, COALESCE(prs.consecutive_failures, 0)
         FROM route_profile_providers rpp
         JOIN providers p ON p.id = rpp.provider_id
         LEFT JOIN provider_runtime_status prs ON prs.provider_id = rpp.provider_id
         WHERE rpp.route_profile_id = ?1
         ORDER BY rpp.priority ASC",
    )?;

    let rows = stmt.query_map([profile_id], |row| {
        Ok(RouteProfileProviderView {
            id: row.get(0)?, provider_id: row.get(1)?, provider_name: row.get(2)?,
            provider_type: row.get(3)?, priority: row.get(4)?, enabled: row.get(5)?,
            model_override: row.get(6)?, max_retries: row.get(7)?, cooldown_seconds: row.get(8)?,
            failover_on_status_codes: row.get(9)?, failover_on_error_keywords: row.get(10)?,
            runtime_available: row.get(11)?, cooldown_until: row.get(12)?, consecutive_failures: row.get(13)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

pub fn add_provider(conn: &Connection, profile_id: &str, provider_id: &str, input: AddProviderToRouteInput) -> Result<(), AppError> {
    // Check for duplicates
    let dup: i64 = conn.query_row(
        "SELECT COUNT(*) FROM route_profile_providers WHERE route_profile_id=?1 AND provider_id=?2",
        params![profile_id, provider_id], |row| row.get(0),
    )?;
    if dup > 0 {
        return Err(AppError::new("ROUTE_PROVIDER_DUPLICATED", "Provider is already in this route profile"));
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
    let default_kw = serde_json::json!(["quota", "insufficient balance", "rate limit", "too many requests", "timeout"]).to_string();

    conn.execute(
        "INSERT INTO route_profile_providers (id, route_profile_id, provider_id, priority, enabled, model_override, max_retries, cooldown_seconds, failover_on_status_codes, failover_on_error_keywords, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
        params![
            uuid::Uuid::new_v4().to_string(), profile_id, provider_id, priority,
            input.model_override, input.max_retries.unwrap_or(0), input.cooldown_seconds.unwrap_or(600),
            input.failover_on_status_codes.as_deref().unwrap_or(&default_codes),
            input.failover_on_error_keywords.as_deref().unwrap_or(&default_kw),
            &now,
        ],
    )?;

    // Ensure runtime status
    conn.execute(
        "INSERT OR IGNORE INTO provider_runtime_status (provider_id, available, consecutive_failures, quota_exhausted, updated_at) VALUES (?1, 1, 0, 0, ?2)",
        params![provider_id, &now],
    )?;

    Ok(())
}

pub fn remove_provider(conn: &Connection, profile_id: &str, provider_id: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM route_profile_providers WHERE route_profile_id=?1 AND provider_id=?2",
        params![profile_id, provider_id],
    )?;
    Ok(())
}

pub fn reorder_providers(conn: &Connection, profile_id: &str, provider_ids: &[String]) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    for (i, pid) in provider_ids.iter().enumerate() {
        conn.execute(
            "UPDATE route_profile_providers SET priority=?1, updated_at=?2 WHERE route_profile_id=?3 AND provider_id=?4",
            params![(i + 1) as i64, &now, profile_id, pid],
        )?;
    }
    Ok(())
}

pub fn update_route_provider(conn: &Connection, profile_id: &str, provider_id: &str, input: UpdateRouteProviderInput) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    // Build dynamic update
    let mut sets = vec!["updated_at = ?1".to_string()];
    let mut idx = 2;

    if input.model_override.is_some() { sets.push(format!("model_override = ?{idx}")); idx += 1; }
    if input.max_retries.is_some() { sets.push(format!("max_retries = ?{idx}")); idx += 1; }
    if input.cooldown_seconds.is_some() { sets.push(format!("cooldown_seconds = ?{idx}")); idx += 1; }
    if input.enabled.is_some() { sets.push(format!("enabled = ?{idx}")); idx += 1; }
    if input.failover_on_status_codes.is_some() { sets.push(format!("failover_on_status_codes = ?{idx}")); idx += 1; }
    if input.failover_on_error_keywords.is_some() { sets.push(format!("failover_on_error_keywords = ?{idx}")); idx += 1; }

    let sql = format!(
        "UPDATE route_profile_providers SET {} WHERE route_profile_id = ?{} AND provider_id = ?{}",
        sets.join(", "), idx, idx + 1
    );

    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    param_values.push(Box::new(now));
    if let Some(v) = input.model_override { param_values.push(Box::new(v)); }
    if let Some(v) = input.max_retries { param_values.push(Box::new(v)); }
    if let Some(v) = input.cooldown_seconds { param_values.push(Box::new(v)); }
    if let Some(v) = input.enabled { param_values.push(Box::new(v)); }
    if let Some(v) = input.failover_on_status_codes { param_values.push(Box::new(v)); }
    if let Some(v) = input.failover_on_error_keywords { param_values.push(Box::new(v)); }
    param_values.push(Box::new(profile_id.to_string()));
    param_values.push(Box::new(provider_id.to_string()));

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
    conn.execute(&sql, params_ref.as_slice())?;
    Ok(())
}
