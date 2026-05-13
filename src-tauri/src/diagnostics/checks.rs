use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::diagnostics::report::*;
use crate::security::local_token;
use crate::storage;

// ── Health Check ──────────────────────────────────────────────

pub fn health_check(db: &Arc<Mutex<Connection>>) -> CheckReport {
    let mut checks = Vec::new();

    // Token file
    let tp = local_token::token_path();
    if tp.exists() {
        checks.push(CheckItem::ok("token_file", "Token file", "Token file exists"));
        match local_token::read_token() {
            Ok(t) if t.starts_with("ag_local_") => {
                checks.push(CheckItem::ok("token_format", "Token format", "Valid ag_local_ format"));
            }
            Ok(_) => {
                checks.push(CheckItem::warning("token_format", "Token format", "Token doesn't start with ag_local_"));
            }
            Err(_) => {
                checks.push(CheckItem::failed("token_read", "Token read", "Cannot read token file"));
            }
        }
    } else {
        checks.push(CheckItem::warning("token_file", "Token file", "Token file not found")
            .with_suggestion("Restart AgentGate to auto-generate"));
    }

    // DB accessible
    match db.lock() {
        Ok(conn) => {
            checks.push(CheckItem::ok("db_lock", "Database", "Database accessible"));
            // Tables
            let tables = ["providers", "gateway_settings", "request_logs", "route_profiles", "config_backups"];
            for table in &tables {
                let exists: bool = conn.prepare(&format!("SELECT 1 FROM {table} LIMIT 0")).is_ok();
                if exists {
                    checks.push(CheckItem::ok(&format!("table_{table}"), &format!("Table: {table}"), "Exists"));
                } else {
                    checks.push(CheckItem::failed(&format!("table_{table}"), &format!("Table: {table}"), "Missing"));
                }
            }
        }
        Err(_) => {
            checks.push(CheckItem::failed("db_lock", "Database", "Cannot acquire database lock"));
        }
    }

    CheckReport::new("Health Check", checks)
}

// ── Database Check ────────────────────────────────────────────

pub fn database_check(db: &Arc<Mutex<Connection>>) -> CheckReport {
    let mut checks = Vec::new();

    let Ok(conn) = db.lock() else {
        checks.push(CheckItem::failed("db_lock", "Database lock", "Cannot lock"));
        return CheckReport::new("Database Check", checks);
    };

    // Row counts
    for (table, label) in &[
        ("providers", "Providers"),
        ("route_profiles", "Route profiles"),
        ("request_logs", "Request logs"),
        ("config_backups", "Config backups"),
    ] {
        match conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get::<_, i64>(0)) {
            Ok(n) => checks.push(CheckItem::ok(&format!("count_{table}"), label, &format!("{n} rows"))),
            Err(_) => checks.push(CheckItem::failed(&format!("count_{table}"), label, "Cannot query")),
        }
    }

    // Log size check
    if let Ok(n) = conn.query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get::<_, i64>(0)) {
        if n > 10000 {
            checks.push(CheckItem::warning("logs_size", "Log size", &format!("{n} logs - consider cleanup"))
                .with_suggestion("Reduce log retention in Settings"));
        }
    }

    // Orphan checks
    let orphan_rpp: i64 = conn.query_row(
        "SELECT COUNT(*) FROM route_profile_providers rpp WHERE NOT EXISTS (SELECT 1 FROM providers WHERE id = rpp.provider_id)",
        [], |r| r.get(0),
    ).unwrap_or(0);
    if orphan_rpp > 0 {
        checks.push(CheckItem::warning("orphan_rpp", "Orphan route providers", &format!("{orphan_rpp} route provider entries reference missing providers")));
    } else {
        checks.push(CheckItem::ok("orphan_rpp", "Route provider integrity", "No orphan records"));
    }

    CheckReport::new("Database Check", checks)
}

// ── Gateway Auth Check ────────────────────────────────────────

pub fn gateway_auth_check(db: &Arc<Mutex<Connection>>) -> CheckReport {
    let mut checks = Vec::new();

    // Token exists
    let token = match local_token::read_token() {
        Ok(t) => {
            checks.push(CheckItem::ok("token_exists", "Token exists", "Local token readable"));
            t
        }
        Err(_) => {
            checks.push(CheckItem::failed("token_exists", "Token exists", "Cannot read token")
                .with_suggestion("Restart AgentGate to auto-generate"));
            return CheckReport::new("Gateway Auth Check", checks);
        }
    };

    // Format
    if token.starts_with("ag_local_") && token.len() > 20 {
        checks.push(CheckItem::ok("token_format", "Token format", "Valid format"));
    } else {
        checks.push(CheckItem::warning("token_format", "Token format", "Unexpected format"));
    }

    // Check logs for token leakage
    if let Ok(conn) = db.lock() {
        let leaked: i64 = conn.query_row(
            "SELECT COUNT(*) FROM request_logs WHERE raw_request LIKE ?1 OR converted_request LIKE ?1 OR error_message LIKE ?1",
            [&format!("%{token}%")], |r| r.get(0),
        ).unwrap_or(0);
        if leaked > 0 {
            checks.push(CheckItem::failed("token_leakage", "Token leakage", &format!("Token found in {leaked} log entries!"))
                .with_suggestion("Clear logs and check redaction logic"));
        } else {
            checks.push(CheckItem::ok("token_leakage", "Token leakage", "No token found in logs"));
        }
    }

    CheckReport::new("Gateway Auth Check", checks)
}

// ── Provider Check ────────────────────────────────────────────

pub fn provider_check(db: &Arc<Mutex<Connection>>) -> CheckReport {
    let mut checks = Vec::new();

    let Ok(conn) = db.lock() else {
        checks.push(CheckItem::failed("db", "Database", "Cannot lock"));
        return CheckReport::new("Provider Check", checks);
    };

    let providers = storage::providers::list_all(&conn).unwrap_or_default();
    let enabled = providers.iter().filter(|p| p.enabled).count();
    checks.push(if enabled > 0 {
        CheckItem::ok("enabled_count", "Enabled providers", &format!("{enabled} enabled"))
    } else {
        CheckItem::failed("enabled_count", "Enabled providers", "No enabled providers")
            .with_suggestion("Enable at least one provider in Providers page")
    });

    // Active provider
    let active = providers.iter().find(|p| p.is_active);
    match active {
        Some(p) => {
            checks.push(CheckItem::ok("active", "Active provider", &p.name));
            if p.api_key.as_ref().map_or(true, |k| k.is_empty()) {
                checks.push(CheckItem::failed("active_key", "Active provider API key", "API key not set")
                    .with_suggestion("Set API key in Providers page"));
            } else {
                checks.push(CheckItem::ok("active_key", "Active provider API key", "Set"));
            }
        }
        None => {
            checks.push(CheckItem::warning("active", "Active provider", "No active provider set"));
        }
    }

    // Cooldown
    let cooldown_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM provider_runtime_status WHERE cooldown_until IS NOT NULL AND cooldown_until > ?1",
        [chrono::Utc::now().to_rfc3339()], |r| r.get(0),
    ).unwrap_or(0);
    if cooldown_count > 0 {
        checks.push(CheckItem::warning("cooldown", "Providers in cooldown", &format!("{cooldown_count} in cooldown")));
    } else {
        checks.push(CheckItem::ok("cooldown", "Provider cooldowns", "None in cooldown"));
    }

    CheckReport::new("Provider Check", checks)
}

// ── Codex Config Check ────────────────────────────────────────

pub fn codex_config_check(db: &Arc<Mutex<Connection>>) -> CheckReport {
    let mut checks = Vec::new();

    let status = crate::tools::codex::detect();

    if !status.exists {
        checks.push(CheckItem::warning("config_exists", "Config file", "Not found")
            .with_suggestion("Apply config from Tools page"));
        return CheckReport::new("Codex Config Check", checks);
    }
    checks.push(CheckItem::ok("config_exists", "Config file", "Exists"));

    if status.has_agentgate {
        checks.push(CheckItem::ok("agentgate_provider", "AgentGate provider", "Configured"));
    } else {
        checks.push(CheckItem::warning("agentgate_provider", "AgentGate provider", "Not configured")
            .with_suggestion("Apply AgentGate config from Tools page"));
    }

    if status.current_provider.as_deref() == Some("agentgate") {
        checks.push(CheckItem::ok("model_provider", "model_provider", "Set to agentgate"));
    } else {
        checks.push(CheckItem::warning("model_provider", "model_provider", &format!("Set to {:?}", status.current_provider)));
    }

    // Check auth.json
    if status.auth_json_exists {
        checks.push(CheckItem::ok("auth_json", "auth.json", "Exists"));
        if status.has_agentgate_auth {
            checks.push(CheckItem::ok("auth_token", "Auth token", "AgentGate token set"));
        } else {
            checks.push(CheckItem::warning("auth_token", "Auth token", "auth.json doesn't contain AgentGate token")
                .with_suggestion("Re-apply Codex config from Tools page"));
        }
    } else {
        checks.push(CheckItem::warning("auth_json", "auth.json", "Not found")
            .with_suggestion("Apply config from Tools page"));
    }

    // Base URL check
    if let Ok(conn) = db.lock() {
        if let Ok(settings) = storage::gateway_settings::get(&conn) {
            let expected_url = format!("http://{}:{}/v1", settings.host, settings.port);
            let content = std::fs::read_to_string(crate::tools::codex::config_path()).unwrap_or_default();
            if content.contains(&expected_url) {
                checks.push(CheckItem::ok("base_url", "Base URL", &format!("Matches gateway: {expected_url}")));
            } else {
                checks.push(CheckItem::warning("base_url", "Base URL", "May not match current gateway settings")
                    .with_suggestion("Re-apply config if gateway host/port changed"));
            }
        }
    }

    CheckReport::new("Codex Config Check", checks)
}

// ── Claude Code Config Check ──────────────────────────────────

pub fn claude_code_config_check(db: &Arc<Mutex<Connection>>) -> CheckReport {
    let mut checks = Vec::new();

    let status = crate::tools::claude_code::detect_env();

    if !status.settings_exists {
        checks.push(CheckItem::warning("settings_exists", "Settings file", "Not found")
            .with_suggestion("Apply config from Tools page"));
        return CheckReport::new("Claude Code Config Check", checks);
    }
    checks.push(CheckItem::ok("settings_exists", "Settings file", "Exists"));

    if status.has_agentgate {
        checks.push(CheckItem::ok("agentgate_token", "AgentGate token", "Found in settings"));

        // Verify token matches
        if let Ok(current_token) = local_token::read_token() {
            let content = std::fs::read_to_string(crate::tools::claude_code::settings_path()).unwrap_or_default();
            if content.contains(&current_token) {
                checks.push(CheckItem::ok("token_match", "Token match", "Settings token matches current"));
            } else {
                checks.push(CheckItem::failed("token_match", "Token match", "Token in settings doesn't match current token")
                    .with_suggestion("Re-apply Claude Code config from Tools page (token was regenerated)"));
            }
        }
    } else {
        checks.push(CheckItem::warning("agentgate_token", "AgentGate token", "Not found in settings"));
    }

    // Base URL
    if let Some(ref url) = status.active_base_url {
        if url.contains("127.0.0.1") || url.contains("localhost") {
            checks.push(CheckItem::ok("base_url", "Base URL", url));
        } else {
            checks.push(CheckItem::warning("base_url", "Base URL", &format!("Points to {url}, not AgentGate")));
        }
    }

    // Conflicts
    if status.conflicts.is_empty() {
        checks.push(CheckItem::ok("conflicts", "Env conflicts", "No conflicts"));
    } else {
        for c in &status.conflicts {
            checks.push(CheckItem::warning("conflict", "Env conflict", c));
        }
    }

    CheckReport::new("Claude Code Config Check", checks)
}

// ── Route Profile Check ───────────────────────────────────────

pub fn route_profile_check(db: &Arc<Mutex<Connection>>) -> CheckReport {
    let mut checks = Vec::new();

    let Ok(conn) = db.lock() else {
        checks.push(CheckItem::failed("db", "Database", "Cannot lock"));
        return CheckReport::new("Route Profile Check", checks);
    };

    let profiles = storage::route_profiles::list_all(&conn).unwrap_or_default();
    if profiles.is_empty() {
        checks.push(CheckItem::warning("profiles", "Route profiles", "No profiles exist"));
        return CheckReport::new("Route Profile Check", checks);
    }

    checks.push(CheckItem::ok("profiles", "Route profiles", &format!("{} profiles", profiles.len())));

    let default = profiles.iter().find(|p| p.is_default);
    match default {
        Some(d) => {
            checks.push(CheckItem::ok("default", "Default profile", &d.name));
            if d.providers_count == 0 {
                checks.push(CheckItem::failed("default_providers", "Default providers", "No providers in default profile")
                    .with_suggestion("Add providers in Routes page"));
            } else {
                checks.push(CheckItem::ok("default_providers", "Default providers", &format!("{} providers", d.providers_count)));
            }
            if d.mode == "failover" && d.providers_count < 2 {
                checks.push(CheckItem::warning("failover_count", "Failover mode", "Less than 2 providers for failover"));
            }
        }
        None => {
            checks.push(CheckItem::warning("default", "Default profile", "No default profile set"));
        }
    }

    CheckReport::new("Route Profile Check", checks)
}

// ── Full Self Test ────────────────────────────────────────────

pub fn full_self_test(db: &Arc<Mutex<Connection>>) -> FullSelfTestReport {
    let reports = vec![
        health_check(db),
        database_check(db),
        gateway_auth_check(db),
        provider_check(db),
        route_profile_check(db),
        codex_config_check(db),
        claude_code_config_check(db),
    ];

    let has_failed = reports.iter().any(|r| r.status == "failed");
    let has_warning = reports.iter().any(|r| r.status == "warning");
    let overall = if has_failed { "failed" } else if has_warning { "warning" } else { "ok" };

    let ok_count = reports.iter().filter(|r| r.status == "ok").count();
    let summary = format!("{}/{} checks passed", ok_count, reports.len());

    FullSelfTestReport {
        overall_status: overall.to_string(),
        reports,
        summary,
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}

// ── Diagnostic Bundle Export ──────────────────────────────────

pub fn export_bundle(
    db: &Arc<Mutex<Connection>>,
    include_logs: bool,
    max_logs: usize,
) -> Result<ExportResult, crate::errors::AppError> {
    use crate::security::redaction;
    use std::fs;
    use std::io::Write;

    let export_dir = local_token::token_dir().join("diagnostics");
    fs::create_dir_all(&export_dir).map_err(|e| {
        crate::errors::AppError::new("DIAGNOSTIC_EXPORT_FAILED", format!("Cannot create dir: {e}"))
    })?;

    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let bundle_dir = export_dir.join(format!("agentgate-diag-{ts}"));
    fs::create_dir_all(&bundle_dir).map_err(|e| {
        crate::errors::AppError::new("DIAGNOSTIC_EXPORT_FAILED", format!("Cannot create bundle dir: {e}"))
    })?;

    let mut files = Vec::new();
    let mut warnings = Vec::new();

    // 1. Self test report
    let report = full_self_test(db);
    let report_json = serde_json::to_string_pretty(&report).unwrap_or_default();
    fs::write(bundle_dir.join("self_test_report.json"), &report_json).ok();
    files.push("self_test_report.json".to_string());

    // 2. Gateway status
    if let Ok(conn) = db.lock() {
        if let Ok(settings) = storage::gateway_settings::get(&conn) {
            let sj = serde_json::to_string_pretty(&settings).unwrap_or_default();
            fs::write(bundle_dir.join("gateway_settings.json"), &sj).ok();
            files.push("gateway_settings.json".to_string());
        }

        // 3. Providers (redacted)
        let providers = storage::providers::list_all(&conn).unwrap_or_default();
        let redacted: Vec<serde_json::Value> = providers.iter().map(|p| {
            serde_json::json!({
                "id": p.id, "name": p.name, "provider_type": p.provider_type,
                "base_url": p.base_url, "default_model": p.default_model,
                "protocol": p.protocol, "status": p.status,
                "enabled": p.enabled, "is_active": p.is_active,
                "api_key": p.api_key.as_ref().map(|k| redaction::redact_value(k)),
            })
        }).collect();
        fs::write(bundle_dir.join("providers.redacted.json"), serde_json::to_string_pretty(&redacted).unwrap_or_default()).ok();
        files.push("providers.redacted.json".to_string());

        // 4. Route profiles
        let profiles = storage::route_profiles::list_all(&conn).unwrap_or_default();
        fs::write(bundle_dir.join("route_profiles.json"), serde_json::to_string_pretty(&profiles).unwrap_or_default()).ok();
        files.push("route_profiles.json".to_string());

        // 5. Recent logs (redacted)
        if include_logs {
            let filter = crate::models::request_log::RequestLogFilter {
                client: None, provider: None, status: None, keyword: None,
                limit: Some(max_logs as i64), offset: None,
            };
            let logs = storage::request_logs::list(&conn, filter).unwrap_or_default();
            let redacted_logs: Vec<serde_json::Value> = logs.iter().map(|l| {
                serde_json::json!({
                    "request_id": l.request_id, "timestamp": l.timestamp,
                    "client": l.client, "provider": l.provider, "model": l.model,
                    "route": l.route, "status_code": l.status_code, "latency_ms": l.latency_ms,
                    "error_message": l.error_message.as_ref().map(|m| redaction::redact_text(m)),
                })
            }).collect();
            fs::write(bundle_dir.join("recent_logs.redacted.json"), serde_json::to_string_pretty(&redacted_logs).unwrap_or_default()).ok();
            files.push("recent_logs.redacted.json".to_string());
        }
    }

    // 6. Config summaries
    let codex = crate::tools::codex::detect();
    let claude = crate::tools::claude_code::detect_env();
    let config_summary = serde_json::json!({
        "codex": { "exists": codex.exists, "has_agentgate": codex.has_agentgate, "auth_mode": codex.auth_mode },
        "claude_code": { "exists": claude.settings_exists, "has_agentgate": claude.has_agentgate, "conflicts": claude.conflicts.len() },
    });
    fs::write(bundle_dir.join("config_summaries.json"), serde_json::to_string_pretty(&config_summary).unwrap_or_default()).ok();
    files.push("config_summaries.json".to_string());

    // 7. README
    let readme = "AgentGate Diagnostic Bundle\n\
        ==========================\n\n\
        This bundle contains diagnostic information for troubleshooting.\n\
        All API keys and tokens have been redacted.\n\n\
        DO NOT share this bundle publicly if it contains request payloads.\n";
    fs::write(bundle_dir.join("README.txt"), readme).ok();
    files.push("README.txt".to_string());

    // 8. Manifest
    let manifest = serde_json::json!({
        "app_version": "0.1.0",
        "platform": std::env::consts::OS,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "files": files,
        "redaction_enabled": true,
    });
    fs::write(bundle_dir.join("manifest.json"), serde_json::to_string_pretty(&manifest).unwrap_or_default()).ok();

    let bundle_path = bundle_dir.to_string_lossy().to_string();

    Ok(ExportResult {
        success: true,
        path: bundle_path,
        files,
        warnings,
    })
}
