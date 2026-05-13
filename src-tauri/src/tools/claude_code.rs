use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::errors::AppError;
use crate::security::local_token;

const ENV_VARS: &[&str] = &[
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
];

const SHELL_PROFILES: &[&str] = &[".zshrc", ".bashrc", ".bash_profile", ".profile"];

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeCodeEnvStatus {
    pub settings_path: String,
    pub settings_exists: bool,
    pub current_env: HashMap<String, String>,
    pub detected_profiles: Vec<ProfileDetection>,
    pub conflicts: Vec<String>,
    pub active_base_url: Option<String>,
    pub active_model: Option<String>,
    pub has_api_key: bool,
    pub has_auth_token: bool,
    pub has_agentgate: bool,
    pub auth_mode: String,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileDetection {
    pub path: String,
    pub exists: bool,
    pub has_anthropic_vars: bool,
    pub var_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaudeCodeConfigPreview {
    pub config_path: String,
    pub exists: bool,
    pub current_summary: Option<String>,
    pub proposed_env: HashMap<String, String>,
    pub warnings: Vec<String>,
    pub conflicts: Vec<String>,
    pub auth_mode: String,
    pub masked_local_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyConfigResult {
    pub success: bool,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub changed_keys: Vec<String>,
    pub warnings: Vec<String>,
}

/// Claude Code settings.json path
pub fn settings_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".claude").join("settings.json")
}

pub fn detect_env() -> ClaudeCodeEnvStatus {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    let sp = settings_path();
    let sp_str = sp.to_string_lossy().to_string();
    let settings_exists = sp.exists();

    // Check if settings.json has agentgate token
    let has_agentgate = if settings_exists {
        let content = fs::read_to_string(&sp).unwrap_or_default();
        content.contains("ag_local_")
    } else {
        false
    };

    // Current process env (masked)
    let mut current_env = HashMap::new();
    for var in ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            let masked = if var.contains("KEY") || var.contains("TOKEN") {
                mask_value(&val)
            } else {
                val
            };
            current_env.insert(var.to_string(), masked);
        }
    }

    // Shell profiles
    let mut detected_profiles = Vec::new();
    for profile in SHELL_PROFILES {
        let path = PathBuf::from(&home).join(profile);
        let exists = path.exists();
        let (has_vars, var_count) = if exists {
            let content = fs::read_to_string(&path).unwrap_or_default();
            let count = ENV_VARS.iter().filter(|v| content.contains(*v)).count();
            (count > 0, count)
        } else {
            (false, 0)
        };
        detected_profiles.push(ProfileDetection {
            path: path.to_string_lossy().to_string(), exists, has_anthropic_vars: has_vars, var_count,
        });
    }

    // Conflicts
    let mut conflicts = Vec::new();
    let has_api_key = std::env::var("ANTHROPIC_API_KEY").is_ok();
    let has_auth_token = std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok();
    if has_api_key && has_auth_token {
        conflicts.push("BOTH_API_KEY_AND_AUTH_TOKEN: Both are set. Claude Code may use AUTH_TOKEN preferentially.".to_string());
    }

    let active_base_url = std::env::var("ANTHROPIC_BASE_URL").ok();
    let active_model = std::env::var("ANTHROPIC_MODEL").ok();

    let mut recommendations = Vec::new();
    if !has_api_key && !has_auth_token && !has_agentgate {
        recommendations.push("No credentials found. Apply AgentGate config to set up Claude Code.".to_string());
    }
    if has_api_key && has_auth_token {
        recommendations.push("Remove one of ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN to avoid conflicts.".to_string());
    }

    ClaudeCodeEnvStatus {
        settings_path: sp_str, settings_exists, current_env, detected_profiles, conflicts,
        active_base_url, active_model, has_api_key, has_auth_token, has_agentgate,
        auth_mode: "inline_token".to_string(), recommendations,
    }
}

pub fn preview_config(host: &str, port: i64, model: &str) -> Result<ClaudeCodeConfigPreview, AppError> {
    let token = local_token::ensure_token()?;
    let masked = local_token::mask_token(&token);

    let sp = settings_path();
    let sp_str = sp.to_string_lossy().to_string();
    let exists = sp.exists();

    let mut warnings = Vec::new();
    let mut conflicts = Vec::new();

    let current_summary = if exists {
        let content = fs::read_to_string(&sp).unwrap_or_default();
        if content.contains("ANTHROPIC_AUTH_TOKEN") {
            conflicts.push("Existing config has ANTHROPIC_AUTH_TOKEN which may conflict".to_string());
        }
        Some(format!("File exists ({} bytes)", content.len()))
    } else {
        warnings.push("Settings file does not exist, will be created".to_string());
        None
    };

    let mut proposed_env = HashMap::new();
    proposed_env.insert("ANTHROPIC_BASE_URL".to_string(), format!("http://{host}:{port}"));
    proposed_env.insert("ANTHROPIC_API_KEY".to_string(), masked.clone());
    proposed_env.insert("ANTHROPIC_MODEL".to_string(), model.to_string());
    proposed_env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), model.to_string());
    proposed_env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), model.to_string());
    proposed_env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), model.to_string());

    Ok(ClaudeCodeConfigPreview {
        config_path: sp_str, exists, current_summary, proposed_env,
        warnings, conflicts, auth_mode: "inline_token".to_string(), masked_local_token: masked,
    })
}

pub fn apply_config(host: &str, port: i64, model: &str, backup_dir: &Path) -> Result<ApplyConfigResult, AppError> {
    let token = local_token::ensure_token()?;

    let sp = settings_path();
    let sp_str = sp.to_string_lossy().to_string();
    let mut backup_path_str = None;
    let mut warnings = Vec::new();

    // Ensure parent dir
    if let Some(parent) = sp.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::new("CLAUDE_CONFIG_WRITE_FAILED", format!("Cannot create directory: {e}"))
        })?;
    }

    // Backup if exists
    if sp.exists() {
        let bp = create_backup(&sp, backup_dir, "claude_code_settings")?;
        backup_path_str = Some(bp);
    }

    // Read existing or start fresh
    let existing = if sp.exists() {
        fs::read_to_string(&sp).unwrap_or_else(|_| "{}".to_string())
    } else {
        "{}".to_string()
    };

    // Parse JSON
    let mut doc: serde_json::Value = serde_json::from_str(&existing).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_PARSE_ERROR", format!("Cannot parse settings.json: {e}"))
    })?;

    // Ensure env object exists
    if doc.get("env").is_none() {
        doc["env"] = serde_json::json!({});
    }

    let env = doc["env"].as_object_mut().ok_or_else(|| {
        AppError::new("CLAUDE_CONFIG_PARSE_ERROR", "env field is not an object")
    })?;

    // Set AgentGate fields
    env.insert("ANTHROPIC_BASE_URL".to_string(), serde_json::json!(format!("http://{host}:{port}")));
    env.insert("ANTHROPIC_API_KEY".to_string(), serde_json::json!(token));
    env.insert("ANTHROPIC_MODEL".to_string(), serde_json::json!(model));
    env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), serde_json::json!(model));
    env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), serde_json::json!(model));
    env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), serde_json::json!(model));

    // Remove ANTHROPIC_AUTH_TOKEN to avoid conflict
    if env.remove("ANTHROPIC_AUTH_TOKEN").is_some() {
        warnings.push("Removed ANTHROPIC_AUTH_TOKEN to avoid conflict with ANTHROPIC_API_KEY".to_string());
    }

    // Write
    let new_content = serde_json::to_string_pretty(&doc).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_WRITE_FAILED", format!("Cannot serialize: {e}"))
    })?;

    let tmp_path = sp.with_extension("json.tmp");
    fs::write(&tmp_path, &new_content).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_WRITE_FAILED", format!("Failed to write temp: {e}"))
    })?;

    fs::rename(&tmp_path, &sp).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        if let Some(ref bp) = backup_path_str {
            let _ = fs::copy(bp, &sp);
        }
        AppError::new("CLAUDE_CONFIG_WRITE_FAILED", format!("Failed to replace: {e}"))
    })?;

    let changed_keys = vec![
        "ANTHROPIC_BASE_URL".to_string(), "ANTHROPIC_API_KEY".to_string(),
        "ANTHROPIC_MODEL".to_string(), "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
        "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
    ];

    Ok(ApplyConfigResult {
        success: true, config_path: sp_str,
        backup_path: backup_path_str, changed_keys, warnings,
    })
}

pub fn backup_config(backup_dir: &Path) -> Result<crate::tools::codex::BackupResult, AppError> {
    let sp = settings_path();
    if !sp.exists() {
        return Err(AppError::new("CLAUDE_CONFIG_NOT_FOUND", "Claude Code settings.json does not exist"));
    }
    let bp = create_backup(&sp, backup_dir, "claude_code_settings")?;
    Ok(crate::tools::codex::BackupResult {
        backup_id: String::new(),
        backup_path: bp,
        source_path: sp.to_string_lossy().to_string(),
    })
}

pub fn restore_config(backup_path: &str, backup_dir: &Path) -> Result<(), AppError> {
    let sp = settings_path();
    let bp = Path::new(backup_path);
    if !bp.exists() {
        return Err(AppError::new("CLAUDE_CONFIG_RESTORE_FAILED", "Backup file does not exist"));
    }
    if sp.exists() {
        let _ = create_backup(&sp, backup_dir, "claude_code_pre_restore");
    }
    fs::copy(bp, &sp).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_RESTORE_FAILED", format!("Failed to restore: {e}"))
    })?;
    Ok(())
}

pub fn open_config() -> Result<(), AppError> {
    let sp = settings_path();
    if !sp.exists() {
        return Err(AppError::new("CLAUDE_CONFIG_NOT_FOUND", "Claude Code settings.json does not exist"));
    }
    open::that(&sp).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_OPEN_FAILED", format!("Failed to open: {e}"))
    })
}

pub fn generate_env_snippet(host: &str, port: i64, model: &str) -> String {
    let masked = match local_token::read_token() {
        Ok(t) => local_token::mask_token(&t),
        Err(_) => "ag_local_<not_generated>".to_string(),
    };
    format!(
        r#"export ANTHROPIC_BASE_URL="http://{host}:{port}"
export ANTHROPIC_API_KEY="{masked}"
export ANTHROPIC_MODEL="{model}"
export ANTHROPIC_DEFAULT_SONNET_MODEL="{model}"
export ANTHROPIC_DEFAULT_OPUS_MODEL="{model}"
export ANTHROPIC_DEFAULT_HAIKU_MODEL="{model}"
unset ANTHROPIC_AUTH_TOKEN"#
    )
}

pub(crate) fn mask_value(val: &str) -> String {
    if val.len() <= 8 {
        "*".repeat(val.len())
    } else {
        let prefix = &val[..4];
        let suffix = &val[val.len() - 4..];
        format!("{prefix}****{suffix}")
    }
}

fn create_backup(source: &Path, backup_dir: &Path, prefix: &str) -> Result<String, AppError> {
    fs::create_dir_all(backup_dir).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_BACKUP_FAILED", format!("Cannot create backup dir: {e}"))
    })?;
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bak");
    let backup_name = format!("{prefix}_{timestamp}.{ext}");
    let backup_path = backup_dir.join(&backup_name);
    fs::copy(source, &backup_path).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_BACKUP_FAILED", format!("Failed to copy: {e}"))
    })?;
    Ok(backup_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{FS_LOCK, setup_temp_home, cleanup};

    #[test]
    fn test_generate_env_snippet_format() {
        let snippet = generate_env_snippet("127.0.0.1", 9090, "claude-sonnet");
        assert!(snippet.contains("export ANTHROPIC_BASE_URL=\"http://127.0.0.1:9090\""));
        assert!(snippet.contains("export ANTHROPIC_MODEL=\"claude-sonnet\""));
        assert!(snippet.contains("unset ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn test_mask_value_short() {
        assert_eq!(mask_value("abc"), "***");
        assert_eq!(mask_value("abcdefgh"), "********");
    }

    #[test]
    fn test_mask_value_long() {
        let val = "sk-abcdefghijklmnopqrstuvwxyz";
        let masked = mask_value(val);
        assert!(masked.starts_with("sk-a"));
        assert!(masked.ends_with("wxyz"));
        assert!(masked.contains("****"));
    }

    #[test]
    fn test_apply_config_creates_new_file() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let backup_dir = temp.join("backups");
        let result = apply_config("127.0.0.1", 9090, "claude-model", &backup_dir).unwrap();
        assert!(result.success);
        assert!(settings_path().exists());
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("ANTHROPIC_BASE_URL"));
        assert!(content.contains("ANTHROPIC_API_KEY"));
        assert!(content.contains("claude-model"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_config_updates_existing() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let backup_dir = temp.join("backups");
        std::fs::create_dir_all(settings_path().parent().unwrap()).unwrap();
        std::fs::write(settings_path(), r#"{"env":{"ANTHROPIC_MODEL":"old-model"}}"#).unwrap();
        let result = apply_config("127.0.0.1", 9090, "new-model", &backup_dir).unwrap();
        assert!(result.success);
        assert!(result.backup_path.is_some());
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("new-model"));
        assert!(!content.contains("ANTHROPIC_AUTH_TOKEN"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_config_removes_auth_token() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let backup_dir = temp.join("backups");
        std::fs::create_dir_all(settings_path().parent().unwrap()).unwrap();
        std::fs::write(settings_path(), r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"secret"}}"#).unwrap();
        let result = apply_config("127.0.0.1", 9090, "model", &backup_dir).unwrap();
        assert!(result.success);
        assert!(result.warnings.iter().any(|w| w.contains("ANTHROPIC_AUTH_TOKEN")));
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(!content.contains("ANTHROPIC_AUTH_TOKEN"));
        cleanup(&temp);
    }

    #[test]
    fn test_preview_config_new_file() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let preview = preview_config("127.0.0.1", 9090, "model").unwrap();
        assert!(!preview.exists);
        assert!(preview.warnings.iter().any(|w| w.contains("does not exist")));
        assert_eq!(preview.proposed_env.get("ANTHROPIC_BASE_URL"), Some(&"http://127.0.0.1:9090".to_string()));
        cleanup(&temp);
    }

    #[test]
    fn test_preview_config_existing_file() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(settings_path().parent().unwrap()).unwrap();
        std::fs::write(settings_path(), r#"{"env":{}}"#).unwrap();
        let preview = preview_config("127.0.0.1", 9090, "model").unwrap();
        assert!(preview.exists);
        assert_eq!(preview.current_summary, Some("File exists (10 bytes)".to_string()));
        cleanup(&temp);
    }
}
