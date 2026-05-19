use rusqlite::Connection;

use crate::errors::AppError;

pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS providers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider_type TEXT NOT NULL,
            base_url TEXT NOT NULL,
            api_key TEXT,
            default_model TEXT NOT NULL,
            reasoning_model TEXT,
            protocol TEXT NOT NULL,
            timeout_seconds INTEGER NOT NULL DEFAULT 120,
            status TEXT NOT NULL DEFAULT 'not_tested',
            enabled INTEGER NOT NULL DEFAULT 1,
            is_active INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS gateway_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            host TEXT NOT NULL DEFAULT '127.0.0.1',
            port INTEGER NOT NULL DEFAULT 9090,
            active_provider_id TEXT,
            input_protocol TEXT NOT NULL DEFAULT 'openai_responses',
            output_protocol TEXT NOT NULL DEFAULT 'openai_chat_completions',
            auto_start INTEGER NOT NULL DEFAULT 0,
            log_retention_days INTEGER NOT NULL DEFAULT 14,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS request_logs (
            id TEXT PRIMARY KEY,
            request_id TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            client TEXT,
            provider TEXT,
            model TEXT,
            route TEXT,
            status_code INTEGER,
            latency_ms INTEGER,
            input_tokens INTEGER,
            output_tokens INTEGER,
            raw_request TEXT,
            converted_request TEXT,
            raw_response TEXT,
            converted_response TEXT,
            sse_events TEXT,
            tool_calls TEXT,
            error_message TEXT
        );

        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        ",
    )?;

    // Phase 6: route profiles tables
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS route_profiles (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            client_type TEXT NOT NULL,
            input_protocol TEXT NOT NULL,
            mode TEXT NOT NULL DEFAULT 'manual',
            active_provider_id TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            is_default INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS route_profile_providers (
            id TEXT PRIMARY KEY,
            route_profile_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            priority INTEGER NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            model_override TEXT,
            max_retries INTEGER NOT NULL DEFAULT 0,
            cooldown_seconds INTEGER NOT NULL DEFAULT 600,
            failover_on_status_codes TEXT,
            failover_on_error_keywords TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS provider_runtime_status (
            provider_id TEXT PRIMARY KEY,
            available INTEGER NOT NULL DEFAULT 1,
            consecutive_failures INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            last_error_code TEXT,
            last_error_at TEXT,
            cooldown_until TEXT,
            quota_exhausted INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );
        ",
    )?;

    // Migration: add supported_models column to providers
    let has_sm: bool = conn.prepare("SELECT supported_models FROM providers LIMIT 0").is_ok();
    if !has_sm {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN supported_models TEXT;")?;
    }

    // Migration: add model_mapping column to providers
    let has_mm: bool = conn.prepare("SELECT model_mapping FROM providers LIMIT 0").is_ok();
    if !has_mm {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN model_mapping TEXT;")?;
    }

    // Migration: add extra_headers column to providers
    let has_eh: bool = conn.prepare("SELECT extra_headers FROM providers LIMIT 0").is_ok();
    if !has_eh {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN extra_headers TEXT;")?;
    }

    // Migration: add anthropic_base_url column to providers
    let has_abu: bool = conn.prepare("SELECT anthropic_base_url FROM providers LIMIT 0").is_ok();
    if !has_abu {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN anthropic_base_url TEXT;")?;
    }

    // Migration: add supports_vision column to providers
    let has_sv: bool = conn.prepare("SELECT supports_vision FROM providers LIMIT 0").is_ok();
    if !has_sv {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN supports_vision INTEGER;")?;
    }

    // Migration: add responses_base_url column to providers
    let has_rbu: bool = conn.prepare("SELECT responses_base_url FROM providers LIMIT 0").is_ok();
    if !has_rbu {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN responses_base_url TEXT;")?;
    }

    // Phase 7b: model_pricing table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS model_pricing (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            model_pattern TEXT NOT NULL,
            input_price REAL NOT NULL,
            output_price REAL NOT NULL,
            is_custom INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        )"
    )?;
    // Populate defaults
    crate::storage::pricing::ensure_defaults(conn)?;

    // Migration: add cost column to request_logs
    let has_cost: bool = conn.prepare("SELECT cost FROM request_logs LIMIT 0").is_ok();
    if !has_cost {
        conn.execute_batch("ALTER TABLE request_logs ADD COLUMN cost REAL;")?;
    }
    // Migration: add auto_cache_control and supports_cache columns
    let has_acc: bool = conn.prepare("SELECT auto_cache_control FROM providers LIMIT 0").is_ok();
    if !has_acc {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN auto_cache_control INTEGER;")?;
    }
    let has_sc: bool = conn.prepare("SELECT supports_cache FROM providers LIMIT 0").is_ok();
    if !has_sc {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN supports_cache INTEGER;")?;
    }

    // Migration: add routing_conditions to route_profile_providers
    let has_rc: bool = conn.prepare("SELECT routing_conditions FROM route_profile_providers LIMIT 0").is_ok();
    if !has_rc {
        conn.execute_batch("ALTER TABLE route_profile_providers ADD COLUMN routing_conditions TEXT;")?;
    }

    // Backfill cost for logs that have tokens but no cost (runs on every startup,
    // catches newly added pricing defaults and previously unmatched models)
    let _ = crate::storage::pricing::backfill_costs(conn);

    // Phase 7: config_backups table
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS config_backups (
            id TEXT PRIMARY KEY,
            tool_type TEXT NOT NULL,
            source_path TEXT NOT NULL,
            backup_path TEXT NOT NULL,
            backup_kind TEXT NOT NULL,
            created_at TEXT NOT NULL,
            metadata_json TEXT
        );
        ",
    )?;

    // Index on request_logs.timestamp for stats query performance
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_request_logs_timestamp ON request_logs(timestamp);",
    )?;

    // Clean up orphan route_profile_providers referencing deleted providers
    conn.execute_batch(
        "DELETE FROM route_profile_providers WHERE provider_id NOT IN (SELECT id FROM providers);",
    )?;

    // Migration: add trace_json column if not present
    let has_trace: bool = conn
        .prepare("SELECT trace_json FROM request_logs LIMIT 0")
        .is_ok();
    if !has_trace {
        conn.execute_batch("ALTER TABLE request_logs ADD COLUMN trace_json TEXT;")?;
    }

    // Ensure gateway_settings has exactly one row
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM gateway_settings", [], |row| row.get(0))?;
    if count == 0 {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO gateway_settings (id, host, port, input_protocol, output_protocol, auto_start, log_retention_days, updated_at)
             VALUES (1, '127.0.0.1', 9090, 'openai_responses', 'openai_chat_completions', 0, 14, ?1)",
            [&now],
        )?;
    }

    // Migration: convert protocol from single string to JSON array
    // e.g. "openai_chat_completions" → '["openai_chat_completions"]'
    conn.execute_batch(
        "UPDATE providers SET protocol = '[\"' || protocol || '\"]' WHERE protocol NOT LIKE '[%';",
    )?;

    // Migration: rename "OpenCode Default" route profile to "Chat Completions Default"
    conn.execute_batch(
        "UPDATE route_profiles SET name = 'Chat Completions Default' WHERE name = 'OpenCode Default' AND input_protocol = 'openai_chat_completions';",
    )?;

    // Seed default providers on first run
    seed_default_providers(conn)?;

    // Remove demo request logs seeded by older builds. Real gateway request IDs
    // are UUIDs, while these sample rows used a stable req-seed-* prefix.
    conn.execute("DELETE FROM request_logs WHERE request_id LIKE 'req-seed-%'", [])?;

    // Seed default route profile on first run
    seed_default_route_profile(conn)?;

    // Pet settings table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pet_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            pet_type TEXT NOT NULL DEFAULT 'robot',
            visible INTEGER NOT NULL DEFAULT 1,
            pos_x REAL NOT NULL DEFAULT 100.0,
            pos_y REAL NOT NULL DEFAULT 100.0
        );"
    )?;
    // Ensure pet_settings has exactly one row
    let pet_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM pet_settings", [], |row| row.get(0))?;
    if pet_count == 0 {
        conn.execute(
            "INSERT INTO pet_settings (id, pet_type, visible, pos_x, pos_y) VALUES (1, 'robot', 1, 100.0, 100.0)",
            [],
        )?;
    }

    Ok(())
}

fn seed_default_providers(conn: &Connection) -> Result<(), AppError> {
    let count: i64 =
        conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }

    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO providers (id, name, provider_type, base_url, default_model, reasoning_model, protocol, timeout_seconds, status, enabled, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, 1, ?10, ?10)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            "DeepSeek",
            "deepseek",
            "https://api.deepseek.com",
            "deepseek-v4-flash",
            "deepseek-v4-pro",
            r#"["openai_chat_completions"]"#,
            120,
            "not_tested",
            &now,
        ],
    )?;

    conn.execute(
        "INSERT INTO providers (id, name, provider_type, base_url, default_model, protocol, timeout_seconds, status, enabled, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 0, ?9, ?9)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            "Custom OpenAI Compatible",
            "custom_openai_compatible",
            "http://localhost:8000",
            "custom-model",
            r#"["openai_chat_completions"]"#,
            120,
            "not_tested",
            &now,
        ],
    )?;

    // Set active_provider_id in gateway_settings
    let active_id: Option<String> = conn
        .query_row(
            "SELECT id FROM providers WHERE is_active = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = active_id {
        conn.execute(
            "UPDATE gateway_settings SET active_provider_id = ?1, updated_at = ?2 WHERE id = 1",
            rusqlite::params![&id, &now],
        )?;
    }

    Ok(())
}

fn seed_default_route_profile(conn: &Connection) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();

    let active_provider_id: Option<String> = conn
        .query_row("SELECT id FROM providers WHERE is_active = 1 LIMIT 1", [], |row| row.get(0))
        .ok();

    let mut stmt = conn.prepare("SELECT id FROM providers WHERE enabled = 1 ORDER BY is_active DESC, created_at ASC")?;
    let provider_ids: Vec<String> = stmt.query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let default_codes = serde_json::json!([402, 429, 500, 502, 503, 504]).to_string();
    let default_kw = serde_json::json!(["quota", "insufficient balance", "rate limit", "too many requests", "timeout"]).to_string();

    // Seed one default profile per protocol (skip if already exists for that protocol)
    let profiles = [
        ("Codex Default", "openai_responses"),
        ("Claude Code Default", "anthropic_messages"),
        ("Chat Completions Default", "openai_chat_completions"),
    ];

    for (name, protocol) in profiles {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM route_profiles WHERE input_protocol = ?1",
            [protocol], |row| row.get(0),
        )?;
        if exists > 0 {
            continue;
        }

        let profile_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO route_profiles (id, name, client_type, input_protocol, mode, active_provider_id, enabled, is_default, created_at, updated_at)
             VALUES (?1, ?2, '', ?3, 'manual', ?4, 1, 1, ?5, ?5)",
            rusqlite::params![&profile_id, name, protocol, &active_provider_id, &now],
        )?;

        for (i, pid) in provider_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO route_profile_providers (id, route_profile_id, provider_id, priority, enabled, max_retries, cooldown_seconds, failover_on_status_codes, failover_on_error_keywords, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, 0, 600, ?5, ?6, ?7, ?7)",
                rusqlite::params![uuid::Uuid::new_v4().to_string(), &profile_id, pid, (i + 1) as i64, &default_codes, &default_kw, &now],
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO provider_runtime_status (provider_id, available, consecutive_failures, quota_exhausted, updated_at) VALUES (?1, 1, 0, 0, ?2)",
                rusqlite::params![pid, &now],
            )?;
        }
    }

    Ok(())
}
