use std::fs;
use std::path::PathBuf;

use crate::errors::AppError;
use crate::security::local_token;
use serde::Serialize;

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".codex").join("config.toml")
}

pub fn auth_json_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".codex").join("auth.json")
}

/// Directory where we save the user's original config.toml + auth.json.
fn saved_dir() -> PathBuf {
    local_token::token_dir().join("codex_official")
}

fn saved_config_path() -> PathBuf {
    saved_dir().join("config.toml")
}

fn saved_auth_path() -> PathBuf {
    saved_dir().join("auth.json")
}

/// Save original config.toml + auth.json for restoring on toggle.
fn save_official_files() -> Result<(), AppError> {
    let dir = saved_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        AppError::new("CODEX_SAVE_FAILED", format!("Cannot create dir: {e}"))
    })?;

    let cfg = config_path();
    if cfg.exists() {
        fs::copy(&cfg, saved_config_path()).map_err(|e| {
            AppError::new("CODEX_SAVE_FAILED", format!("Cannot save config.toml: {e}"))
        })?;
    }

    let auth = auth_json_path();
    if auth.exists() {
        fs::copy(&auth, saved_auth_path()).map_err(|e| {
            AppError::new("CODEX_SAVE_FAILED", format!("Cannot save auth.json: {e}"))
        })?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(saved_auth_path(), fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Check if saved official files exist.
fn has_saved_official() -> bool {
    saved_auth_path().exists() || saved_config_path().exists()
}

/// Restore both config.toml + auth.json from saved official files.
fn restore_official_files() -> Result<(), AppError> {
    let dir = saved_dir();
    if !dir.exists() {
        return Err(AppError::new("CODEX_NO_SAVED_FILES",
            "No saved official config found. Please log in to Codex again with `codex --login`."));
    }

    let saved_cfg = saved_config_path();
    if saved_cfg.exists() {
        fs::copy(&saved_cfg, config_path()).map_err(|e| {
            AppError::new("CODEX_RESTORE_FAILED", format!("Cannot restore config.toml: {e}"))
        })?;
    }

    let saved_auth = saved_auth_path();
    if saved_auth.exists() {
        fs::copy(&saved_auth, auth_json_path()).map_err(|e| {
            AppError::new("CODEX_RESTORE_FAILED", format!("Cannot restore auth.json: {e}"))
        })?;
    }

    Ok(())
}

/// Write a minimal auth.json with only the AgentGate token as OPENAI_API_KEY.
fn write_agentgate_auth(token: &str) -> Result<(), AppError> {
    let auth_path = auth_json_path();
    if let Some(parent) = auth_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let content = format!("{{\n  \"OPENAI_API_KEY\": \"{token}\"\n}}\n");
    let tmp = auth_path.with_extension("json.tmp");
    fs::write(&tmp, &content).map_err(|e| {
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to write auth.json: {e}"))
    })?;
    fs::rename(&tmp, &auth_path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to replace auth.json: {e}"))
    })?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexConfigStatus {
    pub config_path: String,
    pub auth_json_path: String,
    pub exists: bool,
    pub auth_json_exists: bool,
    pub has_agentgate: bool,
    pub has_agentgate_auth: bool,
    pub current_provider: Option<String>,
    pub current_model: Option<String>,
    pub auth_mode: String,
    pub token_path: String,
    /// Whether the provider is currently set to "agentgate" (active) or something else.
    pub is_agentgate_active: bool,
    /// True if OPENAI_API_KEY in auth.json was overwritten with ag_local_ by old AgentGate.
    pub openai_key_polluted: bool,
    /// True if saved official config exists for toggle restore.
    pub has_saved_official: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyConfigResult {
    pub success: bool,
    pub config_path: String,
    pub auth_json_path: String,
    pub backup_path: Option<String>,
    pub auth_backup_path: Option<String>,
    pub token_path: String,
    pub changed_keys: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn detect() -> CodexConfigStatus {
    let path = config_path();
    let auth_path = auth_json_path();
    let path_str = path.to_string_lossy().to_string();
    let auth_str = auth_path.to_string_lossy().to_string();
    let tp = local_token::token_path().to_string_lossy().to_string();

    let exists = path.exists();
    let auth_json_exists = auth_path.exists();

    let (has_agentgate, current_provider, current_model) = if exists {
        let content = fs::read_to_string(&path).unwrap_or_default();
        (
            content.contains("agentgate"),
            extract_toml_value(&content, "model_provider"),
            extract_toml_value(&content, "model"),
        )
    } else {
        (false, None, None)
    };

    let (has_agentgate_auth, openai_key_polluted) = if auth_json_exists {
        let content = fs::read_to_string(&auth_path).unwrap_or_default();
        if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content) {
            let current_key = map.get("OPENAI_API_KEY").and_then(|v| v.as_str()).unwrap_or("");
            let is_ag = current_key.starts_with("ag_local_");
            // Also check: is it a clean agentgate-only auth (no OAuth tokens)?
            let is_clean_ag = is_ag && !map.contains_key("tokens");
            // Polluted = has ag_local_ but ALSO has OAuth tokens (old version mess)
            let polluted = is_ag && !is_clean_ag && !has_saved_official();
            (is_clean_ag, polluted)
        } else {
            (false, false)
        }
    } else {
        (false, false)
    };

    let is_agentgate_active = current_provider.as_deref() == Some("agentgate");

    CodexConfigStatus {
        config_path: path_str, auth_json_path: auth_str,
        exists, auth_json_exists, has_agentgate, has_agentgate_auth,
        current_provider, current_model,
        auth_mode: "key_swap".to_string(), token_path: tp,
        is_agentgate_active, openai_key_polluted,
        has_saved_official: has_saved_official(),
    }
}

pub fn generate_snippet(host: &str, port: i64) -> String {
    format!(
        r#"model = "gpt-5.5"
model_provider = "agentgate"

[model_providers.agentgate]
name = "AgentGate"
base_url = "http://{host}:{port}/v1"
wire_api = "responses""#,
    )
}

pub fn apply(host: &str, port: i64) -> Result<ApplyConfigResult, AppError> {
    let token = local_token::ensure_token()?;

    let path = config_path();
    let auth_path = auth_json_path();
    let path_str = path.to_string_lossy().to_string();
    let auth_str = auth_path.to_string_lossy().to_string();
    let tp = local_token::token_path().to_string_lossy().to_string();
    let mut warnings = Vec::new();

    // Ensure parent dir
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Cannot create directory: {e}"))
        })?;
    }

    // === Save original config.toml + auth.json for toggle restore ===
    let already_agentgate = detect().is_agentgate_active && detect().has_agentgate_auth;
    if !already_agentgate {
        save_official_files()?;
    }

    // === Write config.toml ===
    let new_content = generate_snippet(host, port);
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, &new_content).map_err(|e| {
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to write temp file: {e}"))
    })?;
    fs::rename(&tmp_path, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to replace config: {e}"))
    })?;

    // === Write clean auth.json with only AgentGate token ===
    write_agentgate_auth(&token)?;

    if has_saved_official() {
        warnings.push("Original config saved. Use toggle to switch back.".to_string());
    }

    let changed_keys = vec![
        "config.toml".to_string(),
        "auth.json".to_string(),
    ];

    Ok(ApplyConfigResult {
        success: true, config_path: path_str, auth_json_path: auth_str,
        backup_path: None, auth_backup_path: None,
        token_path: tp, changed_keys, warnings,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ToggleResult {
    pub success: bool,
    pub new_provider: String,
    pub config_path: String,
}

/// Toggle between AgentGate and the original official config.
/// Swaps both config.toml AND auth.json as a pair.
pub fn toggle_provider(host: &str, port: i64) -> Result<ToggleResult, AppError> {
    let status = detect();

    if status.is_agentgate_active {
        // Switching TO official: restore saved config.toml + auth.json
        restore_official_files()?;
        let new_provider = detect().current_provider.unwrap_or_else(|| "openai".to_string());
        Ok(ToggleResult {
            success: true,
            new_provider,
            config_path: config_path().to_string_lossy().to_string(),
        })
    } else {
        // Switching TO agentgate: save current files, write agentgate config
        let token = local_token::ensure_token()?;
        save_official_files()?;

        // Write agentgate config.toml
        let path = config_path();
        let content = generate_snippet(host, port);
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, &content).map_err(|e| {
            AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to write: {e}"))
        })?;
        fs::rename(&tmp, &path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to replace config: {e}"))
        })?;

        // Write minimal auth.json
        write_agentgate_auth(&token)?;

        Ok(ToggleResult {
            success: true,
            new_provider: "agentgate".to_string(),
            config_path: path.to_string_lossy().to_string(),
        })
    }
}

pub fn open_config() -> Result<(), AppError> {
    let path = config_path();
    if !path.exists() {
        return Err(AppError::new("CODEX_CONFIG_NOT_FOUND", "Codex config file does not exist"));
    }
    open::that(&path).map_err(|e| {
        AppError::new("CODEX_CONFIG_OPEN_FAILED", format!("Failed to open: {e}"))
    })
}

pub(crate) fn extract_toml_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(key) {
            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let val = rest.trim().trim_matches('"').trim_matches('\'');
                return Some(val.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{FS_LOCK, setup_temp_home, cleanup};

    #[test]
    fn test_generate_snippet() {
        let snippet = generate_snippet("127.0.0.1", 9090);
        assert!(snippet.contains("model = \"gpt-5.5\""));
        assert!(snippet.contains("model_provider = \"agentgate\""));
        assert!(snippet.contains("base_url = \"http://127.0.0.1:9090/v1\""));
        assert!(snippet.contains("wire_api = \"responses\""));
    }

    #[test]
    fn test_extract_toml_value() {
        let content = "model = \"gpt-4\"\nmodel_provider = \"agentgate\"\n";
        assert_eq!(extract_toml_value(content, "model"), Some("gpt-4".to_string()));
        assert_eq!(extract_toml_value(content, "model_provider"), Some("agentgate".to_string()));
        assert_eq!(extract_toml_value(content, "missing"), None);
    }

    #[test]
    fn test_apply_creates_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let result = apply("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        assert!(config_path().exists());
        assert!(auth_json_path().exists());
        let cfg = std::fs::read_to_string(config_path()).unwrap();
        assert!(cfg.contains("model_provider = \"agentgate\""));
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("ag_local_"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_saves_official_files() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(config_path(), "model = \"gpt-4\"\nmodel_provider = \"openai\"\n").unwrap();
        let original_auth = r#"{"OPENAI_API_KEY":"sk-real","auth_mode":"chatgpt","tokens":{"access_token":"jwt"}}"#;
        std::fs::write(auth_json_path(), original_auth).unwrap();
        let result = apply("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("ag_local_"));
        assert!(!auth.contains("chatgpt"));
        assert!(has_saved_official());
        let saved = std::fs::read_to_string(saved_auth_path()).unwrap();
        assert!(saved.contains("chatgpt"));
        cleanup(&temp);
    }

    #[test]
    fn test_toggle_provider() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(config_path(), "model = \"gpt-4\"\nmodel_provider = \"openai\"\n").unwrap();
        let original_auth = r#"{"OPENAI_API_KEY":"sk-real","auth_mode":"chatgpt","tokens":{"access_token":"jwt"}}"#;
        std::fs::write(auth_json_path(), original_auth).unwrap();
        apply("127.0.0.1", 9090).unwrap();
        assert!(detect().is_agentgate_active);
        // Toggle to official
        let result = toggle_provider("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("chatgpt"));
        let cfg = std::fs::read_to_string(config_path()).unwrap();
        assert!(cfg.contains("model_provider = \"openai\""));
        // Toggle back to agentgate
        let result = toggle_provider("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        assert_eq!(result.new_provider, "agentgate");
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("ag_local_"));
        cleanup(&temp);
    }

    #[test]
    fn test_detect_no_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let status = detect();
        assert!(!status.exists);
        assert!(!status.has_agentgate);
        cleanup(&temp);
    }

    #[test]
    fn test_detect_with_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(config_path(), "model_provider = \"agentgate\"\nmodel = \"gpt-5\"\n").unwrap();
        let status = detect();
        assert!(status.exists);
        assert!(status.has_agentgate);
        assert_eq!(status.current_provider, Some("agentgate".to_string()));
        assert!(status.is_agentgate_active);
        cleanup(&temp);
    }
}
