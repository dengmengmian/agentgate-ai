use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::errors::AppError;
use crate::security::local_token;

/// Directory where we save the user's original settings.json.
fn saved_dir() -> PathBuf {
    local_token::token_dir().join("claude_code_official")
}

fn saved_settings_path() -> PathBuf {
    saved_dir().join("settings.json")
}

/// Save original settings.json for restoring on toggle.
fn save_official_settings() -> Result<(), AppError> {
    let src = settings_path();
    if !src.exists() {
        return Ok(());
    }
    let dir = saved_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        AppError::new("CLAUDE_SAVE_FAILED", format!("Cannot create dir: {e}"))
    })?;
    fs::copy(&src, saved_settings_path()).map_err(|e| {
        AppError::new("CLAUDE_SAVE_FAILED", format!("Cannot save settings.json: {e}"))
    })?;
    Ok(())
}

/// Check if saved official settings exist.
pub fn has_saved_official() -> bool {
    saved_settings_path().exists()
}

/// Restore the saved original settings.json.
fn restore_official_settings() -> Result<(), AppError> {
    let saved = saved_settings_path();
    if !saved.exists() {
        return Err(AppError::new("CLAUDE_NO_SAVED_FILES",
            "No saved official settings found."));
    }
    fs::copy(&saved, settings_path()).map_err(|e| {
        AppError::new("CLAUDE_RESTORE_FAILED", format!("Cannot restore settings.json: {e}"))
    })?;
    Ok(())
}

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
    pub has_saved_official: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileDetection {
    pub path: String,
    pub exists: bool,
    pub has_anthropic_vars: bool,
    pub var_count: usize,
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
        has_saved_official: has_saved_official(),
    }
}

pub fn apply_config(host: &str, port: i64, model: &str) -> Result<ApplyConfigResult, AppError> {
    let token = local_token::ensure_token()?;

    let sp = settings_path();
    let sp_str = sp.to_string_lossy().to_string();
    let mut warnings = Vec::new();

    // Ensure parent dir
    if let Some(parent) = sp.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::new("CLAUDE_CONFIG_WRITE_FAILED", format!("Cannot create directory: {e}"))
        })?;
    }

    // Save official settings for toggle restore (skip if already agentgate)
    let status = detect_env();
    if !status.has_agentgate && sp.exists() {
        save_official_settings()?;
    }

    // Read existing or start fresh
    let existing = if sp.exists() {
        fs::read_to_string(&sp).unwrap_or_else(|_| "{}".to_string())
    } else {
        "{}".to_string()
    };

    let mut doc: serde_json::Value = serde_json::from_str(&existing).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_PARSE_ERROR", format!("Cannot parse settings.json: {e}"))
    })?;

    if doc.get("env").is_none() {
        doc["env"] = serde_json::json!({});
    }

    let env = doc["env"].as_object_mut().ok_or_else(|| {
        AppError::new("CLAUDE_CONFIG_PARSE_ERROR", "env field is not an object")
    })?;

    env.insert("ANTHROPIC_BASE_URL".to_string(), serde_json::json!(format!("http://{host}:{port}")));
    env.insert("ANTHROPIC_API_KEY".to_string(), serde_json::json!(token));
    env.insert("ANTHROPIC_MODEL".to_string(), serde_json::json!(model));
    env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(), serde_json::json!(model));
    env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), serde_json::json!(model));
    env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(), serde_json::json!(model));

    if env.remove("ANTHROPIC_AUTH_TOKEN").is_some() {
        warnings.push("Removed ANTHROPIC_AUTH_TOKEN to avoid conflict with ANTHROPIC_API_KEY".to_string());
    }

    let new_content = serde_json::to_string_pretty(&doc).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_WRITE_FAILED", format!("Cannot serialize: {e}"))
    })?;

    let tmp_path = sp.with_extension("json.tmp");
    fs::write(&tmp_path, &new_content).map_err(|e| {
        AppError::new("CLAUDE_CONFIG_WRITE_FAILED", format!("Failed to write temp: {e}"))
    })?;
    fs::rename(&tmp_path, &sp).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        AppError::new("CLAUDE_CONFIG_WRITE_FAILED", format!("Failed to replace: {e}"))
    })?;

    if has_saved_official() {
        warnings.push("Original settings saved. Use toggle to switch back.".to_string());
    }

    let changed_keys = vec![
        "ANTHROPIC_BASE_URL".to_string(), "ANTHROPIC_API_KEY".to_string(),
        "ANTHROPIC_MODEL".to_string(),
    ];

    Ok(ApplyConfigResult {
        success: true, config_path: sp_str,
        backup_path: None, changed_keys, warnings,
    })
}

/// Toggle between AgentGate and official config.
pub fn toggle_provider(host: &str, port: i64, model: &str) -> Result<ToggleResult, AppError> {
    let status = detect_env();

    if status.has_agentgate {
        // Switching TO official: restore saved settings.json
        restore_official_settings()?;
        Ok(ToggleResult {
            success: true,
            new_provider: "official".to_string(),
            config_path: settings_path().to_string_lossy().to_string(),
        })
    } else {
        // Switching TO agentgate: save current, apply agentgate
        apply_config(host, port, model)?;
        Ok(ToggleResult {
            success: true,
            new_provider: "agentgate".to_string(),
            config_path: settings_path().to_string_lossy().to_string(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToggleResult {
    pub success: bool,
    pub new_provider: String,
    pub config_path: String,
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
        let result = apply_config("127.0.0.1", 9090, "claude-model").unwrap();
        assert!(result.success);
        assert!(settings_path().exists());
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("ANTHROPIC_BASE_URL"));
        assert!(content.contains("ANTHROPIC_API_KEY"));
        assert!(content.contains("claude-model"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_saves_and_toggle_restores() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        // Pre-create settings with official config
        std::fs::create_dir_all(settings_path().parent().unwrap()).unwrap();
        std::fs::write(settings_path(), r#"{"env":{"ANTHROPIC_API_KEY":"sk-real"}}"#).unwrap();
        // Apply agentgate
        apply_config("127.0.0.1", 9090, "model").unwrap();
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("ag_local_"));
        assert!(has_saved_official());
        // Toggle back to official
        let result = toggle_provider("127.0.0.1", 9090, "model").unwrap();
        assert_eq!(result.new_provider, "official");
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("sk-real"));
        // Toggle to agentgate again
        let result = toggle_provider("127.0.0.1", 9090, "model").unwrap();
        assert_eq!(result.new_provider, "agentgate");
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("ag_local_"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_config_removes_auth_token() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(settings_path().parent().unwrap()).unwrap();
        std::fs::write(settings_path(), r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"secret"}}"#).unwrap();
        let result = apply_config("127.0.0.1", 9090, "model").unwrap();
        assert!(result.success);
        assert!(result.warnings.iter().any(|w| w.contains("ANTHROPIC_AUTH_TOKEN")));
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(!content.contains("ANTHROPIC_AUTH_TOKEN"));
        cleanup(&temp);
    }
}
