use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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
        AppError::new(
            crate::errors::codes::CLAUDE_SAVE_FAILED,
            format!("Cannot create dir: {e}"),
        )
    })?;
    fs::copy(&src, saved_settings_path()).map_err(|e| {
        AppError::new(
            crate::errors::codes::CLAUDE_SAVE_FAILED,
            format!("Cannot save settings.json: {e}"),
        )
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
        return Err(AppError::new(
            crate::errors::codes::CLAUDE_NO_SAVED_FILES,
            "No saved official settings found.",
        ));
    }
    fs::copy(&saved, settings_path()).map_err(|e| {
        AppError::new(
            crate::errors::codes::CLAUDE_RESTORE_FAILED,
            format!("Cannot restore settings.json: {e}"),
        )
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

#[derive(Debug, Clone, Serialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ProfileDetection {
    pub path: String,
    pub exists: bool,
    pub has_anthropic_vars: bool,
    pub var_count: usize,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[specta(rename = "ClaudeCodeApplyConfigResult")]
pub struct ApplyConfigResult {
    pub success: bool,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub changed_keys: Vec<String>,
    pub warnings: Vec<String>,
}

// Claude Code settings.json path
// ===== CC 实时状态提醒(信箱文件方式,接收端见 app::cc_notify)=====

/// CC 状态信箱文件:~/.claude/agentgate-cc-notify.json(与 settings.json 同目录)。
pub fn cc_notify_file() -> PathBuf {
    settings_path().with_file_name("agentgate-cc-notify.json")
}

/// 信箱临时文件:hook 先写它再原子 mv 成正式文件,避免接收端读到半截。
pub fn cc_notify_tmp_file() -> PathBuf {
    settings_path().with_file_name(".agentgate-cc-notify.tmp")
}

/// 把 CC 状态 hook 合并/移除到 settings 文档。多个状态事件写同一信箱,
/// 后端按 hook_event_name 区分 working/waiting/done。完全不碰 env。
fn apply_cc_hook_in_doc(settings: &mut serde_json::Value, enabled: bool) {
    let target = cc_notify_file();
    let tmp = cc_notify_tmp_file();
    let command = format!(
        "cat > {} && mv {} {}",
        tmp.display(),
        tmp.display(),
        target.display()
    );
    let marker = "agentgate-cc-notify";
    let with_matcher = ["Notification", "PreToolUse"];
    let no_matcher = ["UserPromptSubmit", "Stop"];

    fn has_marker(entry: &serde_json::Value, marker: &str) -> bool {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|inner| {
                inner.iter().any(|c: &serde_json::Value| {
                    c.get("command")
                        .and_then(|cmd| cmd.as_str())
                        .map(|x| x.contains(marker))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }

    if enabled {
        let hooks = match settings
            .as_object_mut()
            .map(|o| o.entry("hooks").or_insert_with(|| serde_json::json!({})))
            .and_then(|h| h.as_object_mut())
        {
            Some(h) => h,
            None => return,
        };
        for ev in with_matcher.iter().chain(no_matcher.iter()) {
            let m = with_matcher.contains(ev);
            let arr = match hooks
                .entry(*ev)
                .or_insert_with(|| serde_json::json!([]))
                .as_array_mut()
            {
                Some(a) => a,
                None => continue,
            };
            if arr.iter().any(|e| has_marker(e, marker)) {
                continue;
            }
            let entry = if m {
                serde_json::json!({ "matcher": "", "hooks": [{ "type": "command", "command": command.clone() }] })
            } else {
                serde_json::json!({ "hooks": [{ "type": "command", "command": command.clone() }] })
            };
            arr.push(entry);
        }
    } else if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for ev in with_matcher.iter().chain(no_matcher.iter()) {
            if let Some(arr) = hooks.get_mut(*ev).and_then(|n| n.as_array_mut()) {
                arr.retain(|e| !has_marker(e, marker));
            }
        }
    }
}

/// 开/关 CC 状态提醒:把 hook 写入/移除 settings.json(完全不碰 env)。
pub fn set_cc_hook(enabled: bool) -> Result<(), String> {
    let path = settings_path();
    let mut settings: serde_json::Value = if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| format!("settings.json 解析失败: {e}"))?
    } else {
        serde_json::json!({})
    };
    apply_cc_hook_in_doc(&mut settings, enabled);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let serialized = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, serialized).map_err(|e| e.to_string())?;
    Ok(())
}

/// 菜单勾选态:settings.json 里任一 CC 事件存在我们的 hook 即为开。
pub fn cc_hook_enabled() -> bool {
    let content = match std::fs::read_to_string(settings_path()) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let v: serde_json::Value = match serde_json::from_str(&content) {
        Ok(x) => x,
        Err(_) => return false,
    };
    let marker = "agentgate-cc-notify";
    let events = ["Notification", "PreToolUse", "UserPromptSubmit", "Stop"];
    let hooks = match v.get("hooks") {
        Some(h) => h,
        None => return false,
    };
    events.iter().any(|ev| {
        hooks
            .get(*ev)
            .and_then(|n| n.as_array())
            .map(|arr| {
                arr.iter().any(|e: &serde_json::Value| {
                    e.get("hooks")
                        .and_then(|h| h.as_array())
                        .map(|inner| {
                            inner.iter().any(|c: &serde_json::Value| {
                                c.get("command")
                                    .and_then(|cmd| cmd.as_str())
                                    .map(|x| x.contains(marker))
                                    .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    })
}

pub fn settings_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".claude").join("settings.json")
}

pub fn snapshot_paths() -> Vec<(&'static str, PathBuf)> {
    vec![("settings.json", settings_path())]
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
            path: path.to_string_lossy().to_string(),
            exists,
            has_anthropic_vars: has_vars,
            var_count,
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
        recommendations.push(
            "No credentials found. Apply AgentGate config to set up Claude Code.".to_string(),
        );
    }
    if has_api_key && has_auth_token {
        recommendations.push(
            "Remove one of ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN to avoid conflicts."
                .to_string(),
        );
    }

    ClaudeCodeEnvStatus {
        settings_path: sp_str,
        settings_exists,
        current_env,
        detected_profiles,
        conflicts,
        active_base_url,
        active_model,
        has_api_key,
        has_auth_token,
        has_agentgate,
        auth_mode: "inline_token".to_string(),
        recommendations,
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
            AppError::new(
                crate::errors::codes::CLAUDE_CONFIG_WRITE_FAILED,
                format!("Cannot create directory: {e}"),
            )
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
        AppError::new(
            crate::errors::codes::CLAUDE_CONFIG_PARSE_ERROR,
            format!("Cannot parse settings.json: {e}"),
        )
    })?;

    if doc.get("env").is_none() {
        doc["env"] = serde_json::json!({});
    }

    let env = doc["env"].as_object_mut().ok_or_else(|| {
        AppError::new(
            crate::errors::codes::CLAUDE_CONFIG_PARSE_ERROR,
            "env field is not an object",
        )
    })?;

    env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        serde_json::json!(format!("http://{host}:{port}")),
    );
    env.insert("ANTHROPIC_API_KEY".to_string(), serde_json::json!(token));
    env.insert("ANTHROPIC_MODEL".to_string(), serde_json::json!(model));
    env.insert(
        "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
        serde_json::json!(model),
    );
    env.insert(
        "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
        serde_json::json!(model),
    );
    env.insert(
        "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
        serde_json::json!(model),
    );

    if env.remove("ANTHROPIC_AUTH_TOKEN").is_some() {
        warnings.push(
            "Removed ANTHROPIC_AUTH_TOKEN to avoid conflict with ANTHROPIC_API_KEY".to_string(),
        );
    }

    let new_content = serde_json::to_string_pretty(&doc).map_err(|e| {
        AppError::new(
            crate::errors::codes::CLAUDE_CONFIG_WRITE_FAILED,
            format!("Cannot serialize: {e}"),
        )
    })?;

    let tmp_path = sp.with_extension("json.tmp");
    fs::write(&tmp_path, &new_content).map_err(|e| {
        AppError::new(
            crate::errors::codes::CLAUDE_CONFIG_WRITE_FAILED,
            format!("Failed to write temp: {e}"),
        )
    })?;
    fs::rename(&tmp_path, &sp).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        AppError::new(
            crate::errors::codes::CLAUDE_CONFIG_WRITE_FAILED,
            format!("Failed to replace: {e}"),
        )
    })?;
    crate::tools::config_verify::verify_written(&sp, new_content.as_bytes())
        .map_err(|e| AppError::new(crate::errors::codes::CLAUDE_CONFIG_WRITE_FAILED, e))?;

    if has_saved_official() {
        warnings.push("Original settings saved. Use toggle to switch back.".to_string());
    }

    let changed_keys = vec![
        "ANTHROPIC_BASE_URL".to_string(),
        "ANTHROPIC_API_KEY".to_string(),
        "ANTHROPIC_MODEL".to_string(),
    ];

    Ok(ApplyConfigResult {
        success: true,
        config_path: sp_str,
        backup_path: None,
        changed_keys,
        warnings,
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

#[derive(Debug, Clone, Serialize, specta::Type)]
#[specta(rename = "ClaudeCodeToggleResult")]
pub struct ToggleResult {
    pub success: bool,
    pub new_provider: String,
    pub config_path: String,
}

pub fn open_config() -> Result<(), AppError> {
    let sp = settings_path();
    if !sp.exists() {
        return Err(AppError::new(
            crate::errors::codes::CLAUDE_CONFIG_NOT_FOUND,
            "Claude Code settings.json does not exist",
        ));
    }
    open::that(&sp).map_err(|e| {
        AppError::new(
            crate::errors::codes::CLAUDE_CONFIG_OPEN_FAILED,
            format!("Failed to open: {e}"),
        )
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
    use crate::test_utils::{cleanup, setup_temp_home, FS_LOCK};

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
        std::fs::write(
            settings_path(),
            r#"{"env":{"ANTHROPIC_API_KEY":"sk-real"}}"#,
        )
        .unwrap();
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
        std::fs::write(
            settings_path(),
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"secret"}}"#,
        )
        .unwrap();
        let result = apply_config("127.0.0.1", 9090, "model").unwrap();
        assert!(result.success);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("ANTHROPIC_AUTH_TOKEN")));
        let content = std::fs::read_to_string(settings_path()).unwrap();
        assert!(!content.contains("ANTHROPIC_AUTH_TOKEN"));
        cleanup(&temp);
    }
}
