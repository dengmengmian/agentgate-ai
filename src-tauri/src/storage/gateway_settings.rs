use rusqlite::{params, Connection};

use crate::errors::AppError;
use crate::models::gateway::{GatewaySettings, UpdateGatewaySettingsInput};

pub fn get(conn: &Connection) -> Result<GatewaySettings, AppError> {
    conn.query_row(
        "SELECT id, host, port, active_provider_id, input_protocol, output_protocol,
                auto_start, log_retention_days, updated_at
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
                updated_at: row.get(8)?,
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
    let log_retention_days = input.log_retention_days.unwrap_or(existing.log_retention_days);

    conn.execute(
        "UPDATE gateway_settings SET host=?1, port=?2, active_provider_id=?3,
                input_protocol=?4, output_protocol=?5, auto_start=?6,
                log_retention_days=?7, updated_at=?8
         WHERE id = 1",
        params![
            &host,
            port,
            &active_provider_id,
            &input_protocol,
            &output_protocol,
            auto_start,
            log_retention_days,
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
        let updated = update(&conn, UpdateGatewaySettingsInput {
            host: Some("0.0.0.0".to_string()),
            port: Some(8080),
            active_provider_id: None,
            input_protocol: Some("openai_chat_completions".to_string()),
            output_protocol: None,
            auto_start: Some(true),
            log_retention_days: Some(7),
        }).unwrap();
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
        let updated = update(&conn, UpdateGatewaySettingsInput {
            host: Some("0.0.0.0".to_string()),
            port: None,
            active_provider_id: None,
            input_protocol: None,
            output_protocol: None,
            auto_start: None,
            log_retention_days: None,
        }).unwrap();
        assert_eq!(updated.host, "0.0.0.0");
        assert_eq!(updated.port, original.port);
        assert_eq!(updated.input_protocol, original.input_protocol);
    }
}
