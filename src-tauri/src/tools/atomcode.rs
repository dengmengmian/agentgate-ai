use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::errors::AppError;
use crate::security::local_token;

const AGENTGATE_MODEL: &str = "agentgate";

/// AtomCode config.toml path
pub fn config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".atomcode").join("config.toml")
}

pub fn snapshot_paths() -> Vec<(&'static str, PathBuf)> {
    vec![("config.toml", config_path())]
}

fn saved_dir() -> PathBuf {
    local_token::token_dir().join("atomcode_official")
}

fn saved_config_path() -> PathBuf {
    saved_dir().join("config.toml")
}

fn save_official_config() -> Result<(), AppError> {
    let src = config_path();
    if !src.exists() {
        return Ok(());
    }
    let dir = saved_dir();
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::new("ATOMCODE_SAVE_FAILED", format!("Cannot create dir: {e}")))?;
    fs::copy(&src, saved_config_path()).map_err(|e| {
        AppError::new(
            "ATOMCODE_SAVE_FAILED",
            format!("Cannot save config.toml: {e}"),
        )
    })?;
    Ok(())
}

pub fn has_saved_official() -> bool {
    saved_config_path().exists()
}

fn restore_official_config() -> Result<(), AppError> {
    let saved = saved_config_path();
    if !saved.exists() {
        return Err(AppError::new(
            "ATOMCODE_NO_SAVED_FILES",
            "No saved official config found.",
        ));
    }
    fs::copy(&saved, config_path()).map_err(|e| {
        AppError::new(
            "ATOMCODE_RESTORE_FAILED",
            format!("Cannot restore config.toml: {e}"),
        )
    })?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AtomCodeConfigStatus {
    pub config_path: String,
    pub exists: bool,
    pub has_agentgate: bool,
    pub current_model: Option<String>,
    pub has_saved_official: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[specta(rename = "AtomCodeApplyConfigResult")]
pub struct ApplyConfigResult {
    pub success: bool,
    pub config_path: String,
    pub changed_keys: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[specta(rename = "AtomCodeToggleResult")]
pub struct ToggleResult {
    pub success: bool,
    pub new_provider: String,
    pub config_path: String,
}

pub fn detect() -> AtomCodeConfigStatus {
    let path = config_path();
    let path_str = path.to_string_lossy().to_string();
    let exists = path.exists();

    let (has_agentgate, current_model) = if exists {
        let content = fs::read_to_string(&path).unwrap_or_default();
        let has_ag = content.contains("ag_local_") || content.contains("agentgate");
        // Try to extract model from TOML
        let model = content
            .lines()
            .find(|l| l.trim().starts_with("model") && l.contains('='))
            .and_then(|l| l.split('=').nth(1))
            .map(|v| v.trim().trim_matches('"').to_string());
        (has_ag, model)
    } else {
        (false, None)
    };

    AtomCodeConfigStatus {
        config_path: path_str,
        exists,
        has_agentgate,
        current_model,
        has_saved_official: has_saved_official(),
    }
}

pub fn apply(host: &str, port: i64, _model: &str) -> Result<ApplyConfigResult, AppError> {
    let token = local_token::ensure_token()?;

    let path = config_path();
    let path_str = path.to_string_lossy().to_string();
    let warnings = Vec::new();

    // Ensure parent dir
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::new(
                "ATOMCODE_CONFIG_WRITE_FAILED",
                format!("Cannot create directory: {e}"),
            )
        })?;
    }

    // Save official config for toggle (skip if already agentgate)
    let status = detect();
    if !status.has_agentgate && path.exists() {
        save_official_config()?;
    }

    // Surgical merge: AtomCode users routinely keep their own
    // [providers.deepseek] / [providers.kimi] / [providers.openai] alongside us.
    // Old code overwrote the whole file and erased them — now we only touch
    // top-level `default_provider` and `[providers.agentgate]`.
    let existing = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let mut merged = existing;
    merged = crate::tools::toml_merge::upsert_top_level_key(
        &merged,
        "default_provider",
        "\"agentgate\"",
    );
    let section_body = format!(
        "type = \"openai\"\napi_key = \"{token}\"\nmodel = \"{AGENTGATE_MODEL}\"\nbase_url = \"http://{host}:{port}/v1\"\ncontext_window = 1000000\n"
    );
    merged =
        crate::tools::toml_merge::upsert_section(&merged, "providers.agentgate", &section_body);

    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, &merged).map_err(|e| {
        AppError::new(
            "ATOMCODE_CONFIG_WRITE_FAILED",
            format!("Failed to write: {e}"),
        )
    })?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::new(
            "ATOMCODE_CONFIG_WRITE_FAILED",
            format!("Failed to replace: {e}"),
        )
    })?;

    Ok(ApplyConfigResult {
        success: true,
        config_path: path_str,
        changed_keys: vec!["providers.agentgate".to_string()],
        warnings,
    })
}

pub fn toggle(host: &str, port: i64, model: &str) -> Result<ToggleResult, AppError> {
    let status = detect();

    if status.has_agentgate {
        restore_official_config()?;
        Ok(ToggleResult {
            success: true,
            new_provider: "official".to_string(),
            config_path: config_path().to_string_lossy().to_string(),
        })
    } else {
        apply(host, port, model)?;
        Ok(ToggleResult {
            success: true,
            new_provider: "agentgate".to_string(),
            config_path: config_path().to_string_lossy().to_string(),
        })
    }
}

pub fn open_config() -> Result<(), AppError> {
    let path = config_path();
    if !path.exists() {
        return Err(AppError::new(
            "ATOMCODE_CONFIG_NOT_FOUND",
            "AtomCode config.toml does not exist",
        ));
    }
    open::that(&path).map_err(|e| {
        AppError::new(
            "ATOMCODE_CONFIG_OPEN_FAILED",
            format!("Failed to open: {e}"),
        )
    })
}

pub fn generate_snippet(host: &str, port: i64, _model: &str) -> String {
    let masked = match local_token::read_token() {
        Ok(t) => local_token::mask_token(&t),
        Err(_) => "ag_local_<not_generated>".to_string(),
    };
    format!(
        r#"default_provider = "agentgate"

[providers.agentgate]
type           = "openai"
api_key        = "{masked}"
model          = "{AGENTGATE_MODEL}"
base_url       = "http://{host}:{port}/v1"
context_window = 1000000"#
    )
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
        let result = apply("127.0.0.1", 9090, "gpt-5.5").unwrap();
        assert!(result.success);
        assert!(config_path().exists());
        let content = std::fs::read_to_string(config_path()).unwrap();
        assert!(content.contains("ag_local_"));
        assert!(content.contains("127.0.0.1:9090"));
        // surgical merge 后 key = value 不再带 padding 对齐空格
        assert!(content.contains(r#"model = "agentgate""#));
        assert!(content.contains("[providers.agentgate]"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_preserves_user_other_providers() {
        // 回归测试：surgical merge 不应该擦掉用户已有的 [providers.*] 段
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        let user_config = r#"# my notes
default_provider = "deepseek"

[providers.deepseek]
type = "openai"
api_key = "sk-user-key"
model = "deepseek-chat"
base_url = "https://api.deepseek.com/v1"

[providers.kimi]
type = "openai"
api_key = "sk-kimi-key"
"#;
        std::fs::write(config_path(), user_config).unwrap();

        apply("127.0.0.1", 9090, "gpt-5.5").unwrap();
        let content = std::fs::read_to_string(config_path()).unwrap();

        // 我们的改动落地了
        assert!(content.contains("default_provider = \"agentgate\""));
        assert!(content.contains("[providers.agentgate]"));
        assert!(content.contains("ag_local_"));
        // 用户的其他 provider 完整保留
        assert!(content.contains("[providers.deepseek]"));
        assert!(content.contains("api_key = \"sk-user-key\""));
        assert!(content.contains("[providers.kimi]"));
        assert!(content.contains("api_key = \"sk-kimi-key\""));
        // 注释保留
        assert!(content.contains("# my notes"));
        cleanup(&temp);
    }

    #[test]
    fn test_toggle_saves_and_restores() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(
            config_path(),
            r#"[providers.deepseek]
type = "openai"
api_key = "sk-real"
model = "deepseek-v4-flash"
"#,
        )
        .unwrap();
        // Toggle to agentgate
        let result = toggle("127.0.0.1", 9090, "gpt-5.5").unwrap();
        assert_eq!(result.new_provider, "agentgate");
        let content = std::fs::read_to_string(config_path()).unwrap();
        assert!(content.contains("ag_local_"));
        // Toggle back to official
        let result = toggle("127.0.0.1", 9090, "gpt-5.5").unwrap();
        assert_eq!(result.new_provider, "official");
        let content = std::fs::read_to_string(config_path()).unwrap();
        assert!(content.contains("sk-real"));
        cleanup(&temp);
    }
}
