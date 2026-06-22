use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::errors::AppError;
use crate::security::local_token;

const AGENTGATE_MODEL: &str = "openai/agentgate";

pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home)
        .join(".config")
        .join("opencode")
        .join("opencode.json")
}

pub fn snapshot_paths() -> Vec<(&'static str, PathBuf)> {
    vec![("opencode.json", config_path())]
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct OpenCodeConfigStatus {
    pub config_path: String,
    pub exists: bool,
    pub has_agentgate: bool,
    pub current_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[specta(rename = "OpenCodeApplyConfigResult")]
pub struct ApplyConfigResult {
    pub success: bool,
    pub config_path: String,
    pub changed_keys: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn detect() -> OpenCodeConfigStatus {
    let path = config_path();
    let path_str = path.to_string_lossy().to_string();
    let exists = path.exists();

    let (has_agentgate, current_model) = if exists {
        let content = fs::read_to_string(&path).unwrap_or_default();
        let has_ag = content.contains("ag_local_") || content.contains("agentgate");
        let model = serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|v| v.get("model")?.as_str().map(String::from));
        (has_ag, model)
    } else {
        (false, None)
    };

    OpenCodeConfigStatus {
        config_path: path_str,
        exists,
        has_agentgate,
        current_model,
    }
}

pub fn generate_snippet(host: &str, port: i64) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "model": AGENTGATE_MODEL,
        "provider": {
            "openai": {
                "options": {
                    "apiKey": format!("<ag_local_token>"),
                    "baseURL": format!("http://{host}:{port}/v1")
                }
            }
        }
    }))
    .unwrap_or_default()
}

pub fn apply(host: &str, port: i64) -> Result<ApplyConfigResult, AppError> {
    let token = local_token::ensure_token()?;

    let path = config_path();
    let path_str = path.to_string_lossy().to_string();
    let warnings = Vec::new();

    // Ensure parent dir
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::new(
                crate::errors::codes::OPENCODE_CONFIG_WRITE_FAILED,
                format!("Cannot create directory: {e}"),
            )
        })?;
    }

    // Read existing config or start fresh
    let existing = if path.exists() {
        fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string())
    } else {
        "{}".to_string()
    };

    let mut doc: serde_json::Value = serde_json::from_str(&existing).map_err(|e| {
        AppError::new(
            crate::errors::codes::OPENCODE_CONFIG_PARSE_ERROR,
            format!("Cannot parse opencode.json: {e}"),
        )
    })?;

    // Set model
    doc["model"] = serde_json::json!(AGENTGATE_MODEL);

    // Set provider
    doc["provider"] = serde_json::json!({
        "openai": {
            "options": {
                "apiKey": token,
                "baseURL": format!("http://{host}:{port}/v1")
            }
        }
    });

    // Write
    let new_content = serde_json::to_string_pretty(&doc).map_err(|e| {
        AppError::new(
            crate::errors::codes::OPENCODE_CONFIG_WRITE_FAILED,
            format!("Cannot serialize: {e}"),
        )
    })?;

    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, format!("{new_content}\n")).map_err(|e| {
        AppError::new(
            crate::errors::codes::OPENCODE_CONFIG_WRITE_FAILED,
            format!("Failed to write: {e}"),
        )
    })?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::new(
            crate::errors::codes::OPENCODE_CONFIG_WRITE_FAILED,
            format!("Failed to replace: {e}"),
        )
    })?;
    crate::tools::config_verify::verify_written(&path, format!("{new_content}\n").as_bytes())
        .map_err(|e| AppError::new(crate::errors::codes::OPENCODE_CONFIG_WRITE_FAILED, e))?;

    let changed_keys = vec!["model".to_string(), "provider.openai".to_string()];

    Ok(ApplyConfigResult {
        success: true,
        config_path: path_str,
        changed_keys,
        warnings,
    })
}

pub fn open_config() -> Result<(), AppError> {
    let path = config_path();
    if !path.exists() {
        return Err(AppError::new(
            crate::errors::codes::OPENCODE_CONFIG_NOT_FOUND,
            "OpenCode config file does not exist",
        ));
    }
    open::that(&path).map_err(|e| {
        AppError::new(
            crate::errors::codes::OPENCODE_CONFIG_OPEN_FAILED,
            format!("Failed to open: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{cleanup, setup_temp_home, FS_LOCK};

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
        let result = apply("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        assert!(config_path().exists());
        let content = std::fs::read_to_string(config_path()).unwrap();
        assert!(content.contains("ag_local_"));
        assert!(content.contains("127.0.0.1:9090"));
        assert!(content.contains("openai/agentgate"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_preserves_existing_fields() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(
            config_path(),
            r#"{"autoupdate":true,"tools":{"bash":true}}"#,
        )
        .unwrap();
        let result = apply("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        let content = std::fs::read_to_string(config_path()).unwrap();
        assert!(content.contains("autoupdate"));
        assert!(content.contains("ag_local_"));
        cleanup(&temp);
    }
}
