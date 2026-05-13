use std::fs;
use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigPreview {
    pub config_path: String,
    pub auth_json_path: String,
    pub exists: bool,
    pub current_summary: Option<String>,
    pub proposed_snippet: String,
    pub proposed_auth_json: String,
    pub warnings: Vec<String>,
    pub auth_mode: String,
    pub token_path: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct BackupResult {
    pub backup_id: String,
    pub backup_path: String,
    pub source_path: String,
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

    let has_agentgate_auth = if auth_json_exists {
        let content = fs::read_to_string(&auth_path).unwrap_or_default();
        content.contains("ag_local_")
    } else {
        false
    };

    CodexConfigStatus {
        config_path: path_str, auth_json_path: auth_str,
        exists, auth_json_exists, has_agentgate, has_agentgate_auth,
        current_provider, current_model,
        auth_mode: "auth_json".to_string(), token_path: tp,
    }
}

pub fn generate_snippet(host: &str, port: i64) -> String {
    format!(
        r#"model = "gpt-5"
model_provider = "agentgate"

[model_providers.agentgate]
name = "AgentGate"
base_url = "http://{host}:{port}/v1"
wire_api = "responses""#,
    )
}

fn generate_auth_json_masked() -> String {
    let masked = match local_token::read_token() {
        Ok(t) => local_token::mask_token(&t),
        Err(_) => "ag_local_<not_generated>".to_string(),
    };
    format!("{{\n  \"OPENAI_API_KEY\": \"{masked}\"\n}}")
}

pub fn preview(host: &str, port: i64) -> ConfigPreview {
    let path = config_path();
    let path_str = path.to_string_lossy().to_string();
    let auth_str = auth_json_path().to_string_lossy().to_string();
    let exists = path.exists();
    let tp = local_token::token_path().to_string_lossy().to_string();
    let mut warnings = Vec::new();

    // Build preview that reflects what apply would actually produce
    let (current_summary, snippet) = if exists {
        let content = fs::read_to_string(&path).unwrap_or_default();
        if content.contains("agentgate") {
            warnings.push("Config already contains AgentGate settings, will be updated".to_string());
        }
        let summary = Some(format!("File exists ({} bytes)", content.len()));
        // Simulate what update_toml_content would produce
        let preview_content = match update_toml_content(&content, host, port) {
            Ok(c) => c,
            Err(_) => generate_snippet(host, port),
        };
        (summary, preview_content)
    } else {
        warnings.push("Config file does not exist, will be created".to_string());
        (None, generate_snippet(host, port))
    };

    ConfigPreview {
        config_path: path_str, auth_json_path: auth_str,
        exists, current_summary,
        proposed_snippet: snippet,
        proposed_auth_json: generate_auth_json_masked(),
        warnings,
        auth_mode: "auth_json".to_string(), token_path: tp,
    }
}

pub fn apply(host: &str, port: i64, backup_dir: &Path) -> Result<ApplyConfigResult, AppError> {
    let token = local_token::ensure_token()?;

    let path = config_path();
    let auth_path = auth_json_path();
    let path_str = path.to_string_lossy().to_string();
    let auth_str = auth_path.to_string_lossy().to_string();
    let tp = local_token::token_path().to_string_lossy().to_string();
    let mut config_backup = None;
    let mut auth_backup = None;
    let mut warnings = Vec::new();

    // Ensure parent dir
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Cannot create directory: {e}"))
        })?;
    }

    // Backup config.toml if exists
    if path.exists() {
        config_backup = Some(create_backup(&path, backup_dir, "codex_config")?);
    }

    // Backup auth.json if exists
    if auth_path.exists() {
        auth_backup = Some(create_backup(&auth_path, backup_dir, "codex_auth")?);
    }

    // === Write config.toml ===
    let existing_content = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };

    let new_content = if existing_content.is_empty() {
        generate_snippet(host, port)
    } else {
        match update_toml_content(&existing_content, host, port) {
            Ok(c) => c,
            Err(e) => {
                warnings.push(format!("TOML parse warning: {e}, writing full config"));
                generate_snippet(host, port)
            }
        }
    };

    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, &new_content).map_err(|e| {
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to write temp file: {e}"))
    })?;
    fs::rename(&tmp_path, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        if let Some(ref bp) = config_backup { let _ = fs::copy(bp, &path); }
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to replace config: {e}"))
    })?;

    // === Write auth.json ===
    let auth_content = format!("{{\n  \"OPENAI_API_KEY\": \"{token}\"\n}}\n");
    let auth_tmp = auth_path.with_extension("json.tmp");
    fs::write(&auth_tmp, &auth_content).map_err(|e| {
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to write auth.json: {e}"))
    })?;
    fs::rename(&auth_tmp, &auth_path).map_err(|e| {
        let _ = fs::remove_file(&auth_tmp);
        if let Some(ref bp) = auth_backup { let _ = fs::copy(bp, &auth_path); }
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to replace auth.json: {e}"))
    })?;

    let changed_keys = vec![
        "model_provider".to_string(),
        "model_providers.agentgate".to_string(),
        "auth.json OPENAI_API_KEY".to_string(),
    ];

    Ok(ApplyConfigResult {
        success: true, config_path: path_str, auth_json_path: auth_str,
        backup_path: config_backup, auth_backup_path: auth_backup,
        token_path: tp, changed_keys, warnings,
    })
}

pub fn backup(backup_dir: &Path) -> Result<BackupResult, AppError> {
    let path = config_path();
    if !path.exists() {
        return Err(AppError::new("CODEX_CONFIG_NOT_FOUND", "Codex config file does not exist"));
    }
    let backup_path = create_backup(&path, backup_dir, "codex_config")?;
    Ok(BackupResult {
        backup_id: String::new(),
        backup_path,
        source_path: path.to_string_lossy().to_string(),
    })
}

pub fn restore(backup_path: &str, backup_dir: &Path) -> Result<(), AppError> {
    let path = config_path();
    let bp = Path::new(backup_path);
    if !bp.exists() {
        return Err(AppError::new("CODEX_CONFIG_RESTORE_FAILED", "Backup file does not exist"));
    }
    if path.exists() {
        let _ = create_backup(&path, backup_dir, "codex_pre_restore");
    }
    fs::copy(bp, &path).map_err(|e| {
        AppError::new("CODEX_CONFIG_RESTORE_FAILED", format!("Failed to restore: {e}"))
    })?;
    Ok(())
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

fn create_backup(source: &Path, backup_dir: &Path, prefix: &str) -> Result<String, AppError> {
    fs::create_dir_all(backup_dir).map_err(|e| {
        AppError::new("CODEX_CONFIG_BACKUP_FAILED", format!("Cannot create backup dir: {e}"))
    })?;
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bak");
    let backup_name = format!("{prefix}_{timestamp}.{ext}");
    let backup_path = backup_dir.join(&backup_name);
    fs::copy(source, &backup_path).map_err(|e| {
        AppError::new("CODEX_CONFIG_BACKUP_FAILED", format!("Failed to copy: {e}"))
    })?;
    Ok(backup_path.to_string_lossy().to_string())
}

pub(crate) fn update_toml_content(existing: &str, host: &str, port: i64) -> Result<String, String> {
    use toml_edit::DocumentMut;

    let mut doc: DocumentMut = existing.parse().map_err(|e| format!("{e}"))?;

    // Only set model if not already present
    if doc.get("model").is_none() {
        doc["model"] = toml_edit::value("gpt-5");
    }
    doc["model_provider"] = toml_edit::value("agentgate");

    if doc.get("model_providers").is_none() {
        doc["model_providers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }

    let providers = doc["model_providers"].as_table_mut().ok_or("model_providers is not a table")?;

    let mut agentgate = toml_edit::Table::new();
    agentgate["name"] = toml_edit::value("AgentGate");
    agentgate["base_url"] = toml_edit::value(format!("http://{host}:{port}/v1"));
    agentgate["wire_api"] = toml_edit::value("responses");

    // No auth section — auth goes in auth.json
    // Remove old auth fields if present
    agentgate.remove("auth");
    agentgate.remove("env_key");
    agentgate.remove("experimental_bearer_token");
    agentgate.remove("requires_openai_auth");

    providers["agentgate"] = toml_edit::Item::Table(agentgate);

    Ok(doc.to_string())
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
        assert!(snippet.contains("model = \"gpt-5\""));
        assert!(snippet.contains("model_provider = \"agentgate\""));
        assert!(snippet.contains("base_url = \"http://127.0.0.1:9090/v1\""));
        assert!(snippet.contains("wire_api = \"responses\""));
    }

    #[test]
    fn test_update_toml_content_empty() {
        let result = update_toml_content("", "localhost", 8080).unwrap();
        assert!(result.contains("model_provider = \"agentgate\""));
        assert!(result.contains("base_url = \"http://localhost:8080/v1\""));
    }

    #[test]
    fn test_update_toml_content_preserves_existing_model() {
        let existing = r#"model = "custom-model"
other = "value"
"#;
        let result = update_toml_content(existing, "host", 1234).unwrap();
        assert!(result.contains("model = \"custom-model\""));
        assert!(result.contains("model_provider = \"agentgate\""));
        assert!(result.contains("other = \"value\""));
    }

    #[test]
    fn test_update_toml_content_sets_model_if_missing() {
        let existing = r#"other = "value"
"#;
        let result = update_toml_content(existing, "host", 1234).unwrap();
        assert!(result.contains("model = \"gpt-5\""));
        assert!(result.contains("model_provider = \"agentgate\""));
    }

    #[test]
    fn test_update_toml_content_removes_old_auth_fields() {
        let existing = r#"[model_providers.agentgate]
auth = "old"
env_key = "OLD_KEY"
"#;
        let result = update_toml_content(existing, "host", 1234).unwrap();
        assert!(!result.contains("auth = \"old\""));
        assert!(!result.contains("env_key = \"OLD_KEY\""));
        assert!(result.contains("wire_api = \"responses\""));
    }

    #[test]
    fn test_extract_toml_value_found() {
        let content = r#"model = "gpt-4"
model_provider = "agentgate"
"#;
        assert_eq!(extract_toml_value(content, "model"), Some("gpt-4".to_string()));
        assert_eq!(extract_toml_value(content, "model_provider"), Some("agentgate".to_string()));
    }

    #[test]
    fn test_extract_toml_value_not_found() {
        let content = r#"model = "gpt-4"
"#;
        assert_eq!(extract_toml_value(content, "missing"), None);
    }

    #[test]
    fn test_extract_toml_value_single_quotes() {
        let content = "model = 'gpt-4'\n";
        assert_eq!(extract_toml_value(content, "model"), Some("gpt-4".to_string()));
    }

    #[test]
    fn test_apply_creates_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let backup_dir = temp.join("backups");
        let result = apply("127.0.0.1", 9090, &backup_dir).unwrap();
        assert!(result.success);
        assert!(config_path().exists());
        assert!(auth_json_path().exists());
        let config_content = std::fs::read_to_string(config_path()).unwrap();
        assert!(config_content.contains("agentgate"));
        let auth_content = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth_content.contains("OPENAI_API_KEY"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_updates_existing_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let backup_dir = temp.join("backups");
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(config_path(), "model = \"old-model\"\n").unwrap();
        let result = apply("127.0.0.1", 9090, &backup_dir).unwrap();
        assert!(result.success);
        assert!(result.backup_path.is_some());
        let config_content = std::fs::read_to_string(config_path()).unwrap();
        assert!(config_content.contains("model_provider = \"agentgate\""));
        assert!(config_content.contains("old-model")); // original model preserved
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
        assert_eq!(status.current_model, Some("gpt-5".to_string()));
        cleanup(&temp);
    }
}
