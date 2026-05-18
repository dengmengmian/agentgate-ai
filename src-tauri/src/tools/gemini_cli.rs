use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::errors::AppError;
use crate::security::local_token;

/// Gemini CLI settings.json path
pub fn settings_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".gemini").join("settings.json")
}

/// Directory where we save the user's original settings.json.
fn saved_dir() -> PathBuf {
    local_token::token_dir().join("gemini_cli_official")
}

fn saved_settings_path() -> PathBuf {
    saved_dir().join("settings.json")
}

fn save_official_settings() -> Result<(), AppError> {
    let src = settings_path();
    if !src.exists() {
        return Ok(());
    }
    let dir = saved_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        AppError::new("GEMINI_SAVE_FAILED", format!("Cannot create dir: {e}"))
    })?;
    fs::copy(&src, saved_settings_path()).map_err(|e| {
        AppError::new("GEMINI_SAVE_FAILED", format!("Cannot save settings.json: {e}"))
    })?;
    Ok(())
}

pub fn has_saved_official() -> bool {
    saved_settings_path().exists()
}

fn restore_official_settings() -> Result<(), AppError> {
    let saved = saved_settings_path();
    if !saved.exists() {
        return Err(AppError::new("GEMINI_NO_SAVED_FILES",
            "No saved official settings found."));
    }
    fs::copy(&saved, settings_path()).map_err(|e| {
        AppError::new("GEMINI_RESTORE_FAILED", format!("Cannot restore settings.json: {e}"))
    })?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct GeminiCliConfigStatus {
    pub config_path: String,
    pub exists: bool,
    pub has_agentgate: bool,
    pub current_model: Option<String>,
    pub has_saved_official: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyConfigResult {
    pub success: bool,
    pub config_path: String,
    pub changed_keys: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToggleResult {
    pub success: bool,
    pub new_provider: String,
    pub config_path: String,
}

pub fn detect() -> GeminiCliConfigStatus {
    let sp = settings_path();
    let sp_str = sp.to_string_lossy().to_string();
    let exists = sp.exists();

    let (has_agentgate, current_model) = if exists {
        let content = fs::read_to_string(&sp).unwrap_or_default();
        let has_ag = content.contains("ag_local_") || content.contains("agentgate");
        let model = serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.get("model")?.get("name")?.as_str().map(String::from));
        (has_ag, model)
    } else {
        (false, None)
    };

    GeminiCliConfigStatus {
        config_path: sp_str,
        exists,
        has_agentgate,
        current_model,
        has_saved_official: has_saved_official(),
    }
}

pub fn apply(host: &str, port: i64, model: &str) -> Result<ApplyConfigResult, AppError> {
    let token = local_token::ensure_token()?;

    let sp = settings_path();
    let sp_str = sp.to_string_lossy().to_string();
    let mut warnings = Vec::new();

    // Ensure parent dir
    if let Some(parent) = sp.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::new("GEMINI_CONFIG_WRITE_FAILED", format!("Cannot create directory: {e}"))
        })?;
    }

    // Save official settings for toggle (skip if already agentgate)
    let status = detect();
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
        AppError::new("GEMINI_CONFIG_PARSE_ERROR", format!("Cannot parse settings.json: {e}"))
    })?;

    // Set model
    if doc.get("model").is_none() {
        doc["model"] = serde_json::json!({});
    }
    doc["model"]["name"] = serde_json::json!(model);

    // Set env vars for API routing
    if doc.get("env").is_none() {
        doc["env"] = serde_json::json!({});
    }
    let env = doc["env"].as_object_mut().ok_or_else(|| {
        AppError::new("GEMINI_CONFIG_PARSE_ERROR", "env field is not an object")
    })?;

    // Gemini CLI uses GEMINI_API_KEY + GOOGLE_GEMINI_BASE_URL
    env.insert("GEMINI_API_KEY".to_string(), serde_json::json!(token));
    env.insert("GOOGLE_GEMINI_BASE_URL".to_string(), serde_json::json!(format!("http://{host}:{port}")));

    // Write atomically
    let new_content = serde_json::to_string_pretty(&doc).map_err(|e| {
        AppError::new("GEMINI_CONFIG_WRITE_FAILED", format!("Cannot serialize: {e}"))
    })?;

    let tmp = sp.with_extension("json.tmp");
    fs::write(&tmp, format!("{new_content}\n")).map_err(|e| {
        AppError::new("GEMINI_CONFIG_WRITE_FAILED", format!("Failed to write temp: {e}"))
    })?;
    fs::rename(&tmp, &sp).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::new("GEMINI_CONFIG_WRITE_FAILED", format!("Failed to replace: {e}"))
    })?;

    if has_saved_official() {
        warnings.push("Original settings saved. Use toggle to switch back.".to_string());
    }

    Ok(ApplyConfigResult {
        success: true,
        config_path: sp_str,
        changed_keys: vec!["model.name".to_string(), "env.GEMINI_API_KEY".to_string(), "env.GOOGLE_GEMINI_BASE_URL".to_string()],
        warnings,
    })
}

pub fn toggle(host: &str, port: i64, model: &str) -> Result<ToggleResult, AppError> {
    let status = detect();

    if status.has_agentgate {
        restore_official_settings()?;
        Ok(ToggleResult {
            success: true,
            new_provider: "official".to_string(),
            config_path: settings_path().to_string_lossy().to_string(),
        })
    } else {
        apply(host, port, model)?;
        Ok(ToggleResult {
            success: true,
            new_provider: "agentgate".to_string(),
            config_path: settings_path().to_string_lossy().to_string(),
        })
    }
}

pub fn open_config() -> Result<(), AppError> {
    let sp = settings_path();
    if !sp.exists() {
        return Err(AppError::new("GEMINI_CONFIG_NOT_FOUND", "Gemini CLI settings.json does not exist"));
    }
    open::that(&sp).map_err(|e| {
        AppError::new("GEMINI_CONFIG_OPEN_FAILED", format!("Failed to open: {e}"))
    })
}

pub fn generate_snippet(host: &str, port: i64, model: &str) -> String {
    let masked = match local_token::read_token() {
        Ok(t) => local_token::mask_token(&t),
        Err(_) => "ag_local_<not_generated>".to_string(),
    };
    format!(
        r#"export GEMINI_API_KEY="{masked}"
export GOOGLE_GEMINI_BASE_URL="http://{host}:{port}"
# Model: {model}"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{FS_LOCK, setup_temp_home, cleanup};

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
    fn test_apply_creates_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let result = apply("127.0.0.1", 9090, "gemini-2.5-flash").unwrap();
        assert!(result.success);
        assert!(settings_path().exists());
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("ag_local_"));
        assert!(content.contains("GOOGLE_GEMINI_BASE_URL"));
        assert!(content.contains("gemini-2.5-flash"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_preserves_existing_fields() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(settings_path().parent().unwrap()).unwrap();
        std::fs::write(settings_path(), r#"{"general":{"vimMode":true}}"#).unwrap();
        let result = apply("127.0.0.1", 9090, "gemini-2.5-flash").unwrap();
        assert!(result.success);
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("vimMode"));
        assert!(content.contains("ag_local_"));
        cleanup(&temp);
    }

    #[test]
    fn test_toggle_saves_and_restores() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(settings_path().parent().unwrap()).unwrap();
        std::fs::write(settings_path(), r#"{"env":{"GEMINI_API_KEY":"real-key"}}"#).unwrap();
        // Toggle to agentgate
        let result = toggle("127.0.0.1", 9090, "gemini-2.5-flash").unwrap();
        assert_eq!(result.new_provider, "agentgate");
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("ag_local_"));
        // Toggle back to official
        let result = toggle("127.0.0.1", 9090, "gemini-2.5-flash").unwrap();
        assert_eq!(result.new_provider, "official");
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(content.contains("real-key"));
        cleanup(&temp);
    }
}
