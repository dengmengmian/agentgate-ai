use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::gateway::{GatewaySettings, UpdateGatewaySettingsInput};

pub fn get(conn: &Connection) -> Result<GatewaySettings, AppError> {
    conn.query_row(
        "SELECT id, host, port, active_provider_id, input_protocol, output_protocol,
                auto_start, log_retention_days, body_filter_global, thinking_rectifier_global,
                error_mapper_global, updated_at, health_probe_enabled,
                codex_compact_enabled, codex_compact_summary_max_tokens,
                cost_alert_enabled, cost_alert_threshold
         FROM gateway_settings WHERE id = 1",
        [],
        |row| {
            Ok(GatewaySettings {
                id: row.get(0)?,
                host: row.get(1)?,
                port: row.get(2)?,
                active_provider_id: row.get(3)?,
                input_protocol: row.get(4)?,
                output_protocol: row.get(5)?,
                auto_start: row.get(6)?,
                log_retention_days: row.get(7)?,
                body_filter_global: row.get(8)?,
                thinking_rectifier_global: row.get(9)?,
                error_mapper_global: row.get(10)?,
                updated_at: row.get(11)?,
                health_probe_enabled: row.get(12)?,
                codex_compact_enabled: row.get(13)?,
                codex_compact_summary_max_tokens: row.get(14)?,
                cost_alert_enabled: row.get(15)?,
                cost_alert_threshold: row.get(16)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::internal("Gateway settings not initialized")
        }
        other => AppError::database(other),
    })
}

pub fn update(
    conn: &Connection,
    input: UpdateGatewaySettingsInput,
) -> Result<GatewaySettings, AppError> {
    let existing = get(conn)?;
    let now = chrono::Utc::now().to_rfc3339();

    let host = input.host.unwrap_or(existing.host);
    let port = input.port.unwrap_or(existing.port);
    let active_provider_id = match input.active_provider_id {
        Some(id) => Some(id),
        None => existing.active_provider_id,
    };
    let input_protocol = input.input_protocol.unwrap_or(existing.input_protocol);
    let output_protocol = input.output_protocol.unwrap_or(existing.output_protocol);
    let auto_start = input.auto_start.unwrap_or(existing.auto_start);
    let log_retention_days = input
        .log_retention_days
        .unwrap_or(existing.log_retention_days);
    let body_filter_global = input
        .body_filter_global
        .unwrap_or(existing.body_filter_global);
    let thinking_rectifier_global = input
        .thinking_rectifier_global
        .unwrap_or(existing.thinking_rectifier_global);
    let error_mapper_global = input
        .error_mapper_global
        .unwrap_or(existing.error_mapper_global);
    let health_probe_enabled = input
        .health_probe_enabled
        .unwrap_or(existing.health_probe_enabled);
    let codex_compact_enabled = input
        .codex_compact_enabled
        .unwrap_or(existing.codex_compact_enabled);
    let codex_compact_summary_max_tokens = input
        .codex_compact_summary_max_tokens
        .unwrap_or(existing.codex_compact_summary_max_tokens);
    let cost_alert_enabled = input
        .cost_alert_enabled
        .unwrap_or(existing.cost_alert_enabled);
    let cost_alert_threshold = match input.cost_alert_threshold {
        Some(v) => Some(v),
        None => existing.cost_alert_threshold,
    };

    conn.execute(
        "UPDATE gateway_settings SET host=?1, port=?2, active_provider_id=?3,
                input_protocol=?4, output_protocol=?5, auto_start=?6,
                log_retention_days=?7, body_filter_global=?8,
                thinking_rectifier_global=?9, error_mapper_global=?10,
                health_probe_enabled=?11, codex_compact_enabled=?12,
                codex_compact_summary_max_tokens=?13,
                cost_alert_enabled=?14, cost_alert_threshold=?15, updated_at=?16
         WHERE id = 1",
        params![
            &host,
            port,
            &active_provider_id,
            &input_protocol,
            &output_protocol,
            auto_start,
            log_retention_days,
            body_filter_global,
            thinking_rectifier_global,
            error_mapper_global,
            health_probe_enabled,
            codex_compact_enabled,
            codex_compact_summary_max_tokens,
            cost_alert_enabled,
            cost_alert_threshold,
            &now,
        ],
    )?;

    get(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gateway::UpdateGatewaySettingsInput;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::migrations::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_get_gateway_settings() {
        let conn = setup_db();
        let settings = get(&conn).unwrap();
        assert_eq!(settings.id, 1);
        assert_eq!(settings.host, "127.0.0.1");
        assert_eq!(settings.port, 9090);
    }

    #[test]
    fn test_update_gateway_settings() {
        let conn = setup_db();
        let updated = update(
            &conn,
            UpdateGatewaySettingsInput {
                host: Some("0.0.0.0".to_string()),
                port: Some(8080),
                input_protocol: Some("openai_chat_completions".to_string()),
                auto_start: Some(true),
                log_retention_days: Some(7),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.host, "0.0.0.0");
        assert_eq!(updated.port, 8080);
        assert_eq!(updated.input_protocol, "openai_chat_completions");
        assert_eq!(updated.auto_start, true);
        assert_eq!(updated.log_retention_days, 7);
    }

    #[test]
    fn test_partial_update_preserves_existing() {
        let conn = setup_db();
        let original = get(&conn).unwrap();
        let updated = update(
            &conn,
            UpdateGatewaySettingsInput {
                host: Some("0.0.0.0".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.host, "0.0.0.0");
        assert_eq!(updated.port, original.port);
        assert_eq!(updated.input_protocol, original.input_protocol);
    }
}
