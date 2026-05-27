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
///
/// auth.json is only copied when it currently holds the user's real
/// credentials — i.e. the `OPENAI_API_KEY` doesn't start with `ag_local_`.
/// This protects an existing healthy backup from being overwritten by a
/// previously-polluted live auth.json (older AgentGate builds had stripped
/// the OAuth tokens to just `{"OPENAI_API_KEY":"ag_local_..."}`).
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
    if auth.exists() && !auth_is_polluted(&auth) {
        fs::copy(&auth, saved_auth_path()).map_err(|e| {
            AppError::new("CODEX_SAVE_FAILED", format!("Cannot save auth.json: {e}"))
        })?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let saved = saved_auth_path();
        if saved.exists() {
            let _ = fs::set_permissions(&saved, fs::Permissions::from_mode(0o600));
        }
    }
    Ok(())
}

fn auth_is_polluted(path: &PathBuf) -> bool {
    let Ok(content) = fs::read_to_string(path) else { return false; };
    let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content) else {
        return false;
    };
    let current_key = map.get("OPENAI_API_KEY").and_then(|v| v.as_str()).unwrap_or("");
    current_key.starts_with("ag_local_") && !map.contains_key("tokens")
}

/// Check if saved official files exist.
fn has_saved_official() -> bool {
    saved_auth_path().exists() || saved_config_path().exists()
}

/// Restore both config.toml + auth.json from saved official files.
///
/// Kept for legacy/manual use — the modern `toggle_provider` restore path
/// inlines a config-only copy instead, so auth.json stays untouched. Useful
/// if a user needs a full hard reset to pre-AgentGate state.
#[allow(dead_code)]
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

/// Repair `auth.json` if a previous AgentGate version replaced it with
/// `{"OPENAI_API_KEY": "ag_local_..."}` and stripped the ChatGPT OAuth
/// tokens. The Codex JetBrains / VS Code plugins detect "configured" by
/// reading `auth_mode` + the `tokens` object out of this file — when those
/// are missing the plugin greys out. We restore from `~/.agentgate/codex_official/auth.json`
/// (saved by `save_official_files` the first time we touched the file) and
/// return true so the caller can surface a notice.
///
/// No-op when the current auth.json already has tokens (clean state) or
/// there's no saved backup to restore from.
fn repair_polluted_auth_json() -> bool {
    let auth_path = auth_json_path();
    let Ok(content) = fs::read_to_string(&auth_path) else { return false; };
    let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content) else {
        return false;
    };
    let current_key = map.get("OPENAI_API_KEY").and_then(|v| v.as_str()).unwrap_or("");
    let is_ag_stripped = current_key.starts_with("ag_local_") && !map.contains_key("tokens");
    if !is_ag_stripped {
        return false;
    }
    let saved = saved_auth_path();
    if !saved.exists() {
        return false;
    }
    fs::copy(&saved, &auth_path).is_ok()
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

    // The new "hijack OpenAI provider" config has `model_provider = "OpenAI"`
    // but with the OpenAI provider's `base_url` pointing at localhost (us).
    // Distinguish "AgentGate compat mode" from "real official OpenAI" by:
    //   - presence of `experimental_bearer_token = "ag_local_..."` (our token),
    //   - OR base_url containing `127.0.0.1` / `localhost` on a sane port.
    // The legacy `model_provider = "agentgate"` shape is still recognised so
    // users upgrading from older builds see correct status before re-apply.
    let (has_agentgate, current_provider, current_model) = if exists {
        let content = fs::read_to_string(&path).unwrap_or_default();
        let legacy_marker = content.contains("[model_providers.agentgate]");
        let openai_hijack = content.contains("experimental_bearer_token = \"ag_local_");
        (
            legacy_marker || openai_hijack,
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
            // Polluted = has ag_local_ but ALSO has OAuth tokens (old version mess)
            // OR is the clean ag-only form (legacy stripped state).
            let is_clean_ag = is_ag && !map.contains_key("tokens");
            let polluted = is_ag && !is_clean_ag && !has_saved_official();
            // In the new "hijack OpenAI" world, auth.json should look like the
            // standard ChatGPT-logged-in state — has tokens, OPENAI_API_KEY
            // is null (or absent). `has_agentgate_auth` was the legacy
            // "auth.json stripped to our token" signal; it stays true only
            // for that old shape so the UI can flag it for repair.
            (is_clean_ag, polluted)
        } else {
            (false, false)
        }
    } else {
        (false, false)
    };

    // "AgentGate active" now means: our hijack snippet has been written
    // (we can tell from the ag_local_ bearer token). The model_provider
    // string itself reads "OpenAI" so we can't distinguish from real OpenAI
    // by that field alone — the bearer token presence is the true marker.
    let content_for_active = if exists { fs::read_to_string(&path).unwrap_or_default() } else { String::new() };
    let is_agentgate_active = content_for_active.contains("experimental_bearer_token = \"ag_local_")
        || current_provider.as_deref() == Some("agentgate"); // legacy

    CodexConfigStatus {
        config_path: path_str, auth_json_path: auth_str,
        exists, auth_json_exists, has_agentgate, has_agentgate_auth,
        current_provider, current_model,
        auth_mode: "key_swap".to_string(), token_path: tp,
        is_agentgate_active, openai_key_polluted,
        has_saved_official: has_saved_official(),
    }
}

pub fn generate_snippet(host: &str, port: i64, bearer_token: &str) -> String {
    // The "hijack OpenAI provider + requires_openai_auth" pattern:
    //
    //   model_provider = "OpenAI"   ← official provider name, NOT a custom one
    //
    //   [model_providers.OpenAI]
    //   base_url = "http://localhost:9090/v1"  ← override points to us
    //   requires_openai_auth = true             ← keep ChatGPT auth state live
    //
    // This is the magic: Codex.app's IDE entries (Mobile, plugins, quota,
    // bundled extensions) gate themselves on `model_provider == openai/
    // chatgpt`. By naming the provider `OpenAI` we satisfy that check, and
    // by overriding its `base_url` the actual traffic still flows through
    // AgentGate. `requires_openai_auth = true` tells Codex "treat this
    // provider as one that still needs ChatGPT login state to be valid",
    // so the IDE keeps the official auth-aware UI alive.
    //
    // `experimental_bearer_token` carries the local AgentGate access token
    // — Codex sends `Authorization: Bearer ag_local_…` to us. auth.json is
    // never touched: ChatGPT OAuth (`tokens.access_token`, `auth_mode:
    // chatgpt`) stays intact and continues to drive the IDE login state.
    //
    // `model` is intentionally NOT set at the top level. The IDE / CLI
    // picker chooses the model display name; AgentGate routes by whatever
    // name comes in via its own per-model capability matrix.
    //
    // Credit: the `requires_openai_auth` discovery comes from a CSDN post
    // by "硅基新手村" (alex_yangchuansheng) — this is the only known way
    // to route Codex through a custom base_url while preserving the
    // official ChatGPT IDE features.
    format!(
        r#"model_provider = "OpenAI"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "http://{host}:{port}/v1"
wire_api = "responses"
experimental_bearer_token = "{bearer_token}"
requires_openai_auth = true"#,
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
    let mut changed_keys = vec!["config.toml".to_string()];

    // Ensure parent dir
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Cannot create directory: {e}"))
        })?;
    }

    // Save original config.toml + auth.json the FIRST time we touch them, so
    // a future "switch back to official" can restore the user's pre-AgentGate
    // setup. Skipped when we're already pointing at AgentGate (the saved copy
    // would otherwise overwrite the user's true original with our config).
    let status_before = detect();
    if !status_before.is_agentgate_active {
        save_official_files()?;
    }

    // === Repair: if a previous AgentGate version stripped auth.json down to
    // just our local token, restore the ChatGPT OAuth tokens from the saved
    // backup. The Codex IDE plugin reads auth.json to detect "configured" —
    // wiping the OAuth fields was what greyed it out. New apply() never
    // writes auth.json so once repaired it stays good.
    if repair_polluted_auth_json() {
        warnings.push(
            "已修复被旧版 AgentGate 清空的 auth.json，Codex IDE 插件应该恢复可用。".to_string(),
        );
        changed_keys.push("auth.json (restored)".to_string());
    }

    // === Write config.toml with experimental_bearer_token. auth.json is
    // intentionally left alone — the bearer travels via config.toml's
    // model_providers.agentgate block instead, matching cc-switch's
    // approach. ===
    let new_content = generate_snippet(host, port, &token);
    let tmp_path = path.with_extension("toml.tmp");
    fs::write(&tmp_path, &new_content).map_err(|e| {
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to write temp file: {e}"))
    })?;
    fs::rename(&tmp_path, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        AppError::new("CODEX_CONFIG_WRITE_FAILED", format!("Failed to replace config: {e}"))
    })?;

    if has_saved_official() {
        warnings.push("已备份原始 config.toml，可随时切换回官方配置。".to_string());
    }

    // Heads-up about the hijack-OpenAI design choice. `model_provider =
    // "OpenAI"` + `requires_openai_auth = true` keeps the IDE plugin entries
    // (Browser, Computer-Use, Mobile, quota query) alive because Codex.app
    // sees the official provider name and still demands a valid ChatGPT
    // login. Conversation requests, however, hit AgentGate's localhost
    // endpoint instead of api.openai.com. The user should know that they
    // need to keep `codex login` valid for the IDE bits to keep working.
    warnings.push(
        "已切换到代理模式：对话请求走 AgentGate，但 Codex.app 内嵌插件（Browser / \
         Computer-Use / Mobile / 配额查询）仍走 ChatGPT 官方登录态 —— 都可用。\
         如果之前没登录过 Codex，请先执行 `codex login` 完成 ChatGPT 认证。"
            .to_string(),
    );

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
/// Restores / writes ONLY config.toml — auth.json is intentionally untouched
/// so the Codex IDE plugin (which probes auth.json for `auth_mode` + `tokens`)
/// stays alive throughout the switch.
pub fn toggle_provider(host: &str, port: i64) -> Result<ToggleResult, AppError> {
    let status = detect();

    if status.is_agentgate_active {
        // Switching TO official: restore saved config.toml only.
        // (auth.json was either never touched by new apply, or has already
        // been repaired in a prior apply.)
        let saved_cfg = saved_config_path();
        if !saved_cfg.exists() {
            return Err(AppError::new(
                "CODEX_NO_SAVED_FILES",
                "No saved official config found. Please log in to Codex again with `codex --login`.",
            ));
        }
        fs::copy(&saved_cfg, config_path()).map_err(|e| {
            AppError::new("CODEX_RESTORE_FAILED", format!("Cannot restore config.toml: {e}"))
        })?;
        let new_provider = detect().current_provider.unwrap_or_else(|| "openai".to_string());
        Ok(ToggleResult {
            success: true,
            new_provider,
            config_path: config_path().to_string_lossy().to_string(),
        })
    } else {
        // Switching TO agentgate: same path as `apply()`. Defer to it so the
        // auth-repair safety net runs here too.
        apply(host, port)?;
        Ok(ToggleResult {
            success: true,
            new_provider: "agentgate".to_string(),
            config_path: config_path().to_string_lossy().to_string(),
        })
    }
}

/// Switch Codex back to its pre-AgentGate "native" mode by restoring the
/// saved config.toml. Keeps `auth.json` untouched (new code never modified
/// it; old polluted state was already cleaned up by `apply`). The saved
/// backup itself is kept on disk so the user can toggle back to compat
/// mode at any time without re-running an explicit setup step.
///
/// With the new "hijack OpenAI + requires_openai_auth" config the IDE
/// plugins stay alive in compat mode too, so this is now mostly a "stop
/// routing through AgentGate" switch — useful when the user wants Codex
/// CLI to talk to api.openai.com directly again.
pub fn disable() -> Result<ApplyConfigResult, AppError> {
    let path = config_path();
    let auth_path = auth_json_path();
    let path_str = path.to_string_lossy().to_string();
    let auth_str = auth_path.to_string_lossy().to_string();
    let tp = local_token::token_path().to_string_lossy().to_string();
    let mut warnings = Vec::new();
    let mut changed_keys = Vec::new();

    let saved_cfg = saved_config_path();
    if !saved_cfg.exists() {
        return Err(AppError::new(
            "CODEX_NO_SAVED_FILES",
            "未找到 AgentGate 备份的官方 config.toml — 请先在 Codex 上至少跑过一次官方登录。",
        ));
    }

    fs::copy(&saved_cfg, &path).map_err(|e| {
        AppError::new("CODEX_RESTORE_FAILED", format!("Cannot restore config.toml: {e}"))
    })?;
    changed_keys.push("config.toml (restored)".to_string());

    // Defense-in-depth: even though new `apply` no longer touches auth.json,
    // restore the OAuth backup if the live file got mangled by an older build
    // and a healthy backup is available.
    if repair_polluted_auth_json() {
        warnings.push(
            "auth.json 被旧版 AgentGate 清空过的部分已恢复，Codex IDE 插件应可用。"
                .to_string(),
        );
        changed_keys.push("auth.json (restored)".to_string());
    }

    warnings.push(
        "已切回原生模式：Codex 直连 ChatGPT 官方，对话请求不再经过 AgentGate。\
         如需重新使用第三方模型路由，点击「应用配置」即可切回代理模式。"
            .to_string(),
    );

    Ok(ApplyConfigResult {
        success: true,
        config_path: path_str,
        auth_json_path: auth_str,
        backup_path: None,
        auth_backup_path: None,
        token_path: tp,
        changed_keys,
        warnings,
    })
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
        let snippet = generate_snippet("127.0.0.1", 9090, "ag_local_abc123");

        // The whole reason for this format: hijack the OpenAI provider so
        // the IDE plugins (which gate on `model_provider == openai/chatgpt`)
        // stay alive.
        assert!(
            snippet.contains("model_provider = \"OpenAI\""),
            "must use the official OpenAI provider name (not a custom one)"
        );
        assert!(snippet.contains("[model_providers.OpenAI]"));
        assert!(snippet.contains("name = \"OpenAI\""));
        assert!(snippet.contains("base_url = \"http://127.0.0.1:9090/v1\""));
        assert!(snippet.contains("wire_api = \"responses\""));
        assert!(
            snippet.contains("requires_openai_auth = true"),
            "requires_openai_auth = true is what keeps the IDE plugin alive — \
             the whole point of this config shape"
        );
        assert!(
            snippet.contains("experimental_bearer_token = \"ag_local_abc123\""),
            "bearer travels via config so auth.json can stay untouched"
        );

        // Regression guards: don't reintroduce known-broken shapes.
        assert!(
            !snippet.contains("model_provider = \"agentgate\""),
            "custom model_provider (legacy) triggered IDE plugin grey-out"
        );
        assert!(
            !snippet.contains("[model_providers.agentgate]"),
            "legacy block name must not be reintroduced"
        );
        assert!(
            !snippet.contains("gpt-5.5"),
            "synthetic gpt-5.5 model name triggered IDE picker rejection"
        );
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
        // No pre-existing auth.json + nothing to repair → auth.json should
        // NOT be created by apply. Bearer travels in config.toml.
        assert!(!auth_json_path().exists(), "apply must not create auth.json");
        let cfg = std::fs::read_to_string(config_path()).unwrap();
        assert!(cfg.contains("model_provider = \"OpenAI\""), "new hijack-OpenAI format");
        assert!(cfg.contains("requires_openai_auth = true"));
        assert!(cfg.contains("experimental_bearer_token = \"ag_local_"));
        cleanup(&temp);
    }

    #[test]
    fn test_apply_preserves_chatgpt_oauth_tokens_in_auth_json() {
        // The whole reason for this refactor: applying AgentGate must NOT
        // strip the OAuth tokens from auth.json, because the Codex IDE
        // plugin reads them to detect "configured". Failing this test means
        // the IDE plugin will grey out for users running AgentGate.
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(config_path(), "model = \"gpt-4\"\nmodel_provider = \"openai\"\n").unwrap();
        let original_auth = r#"{"OPENAI_API_KEY":"sk-real","auth_mode":"chatgpt","tokens":{"access_token":"jwt"}}"#;
        std::fs::write(auth_json_path(), original_auth).unwrap();
        let result = apply("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("chatgpt"), "auth_mode must survive apply");
        assert!(auth.contains("access_token"), "tokens must survive apply");
        assert!(auth.contains("sk-real"), "original OPENAI_API_KEY must survive apply");
        assert!(has_saved_official(), "config.toml backup still saved for restore");
        cleanup(&temp);
    }

    #[test]
    fn test_apply_repairs_old_polluted_auth_json() {
        // Migration path: a previous AgentGate version replaced auth.json
        // with just `{"OPENAI_API_KEY":"ag_local_..."}`. On the next apply
        // we should restore the original from the saved backup.
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(saved_dir()).unwrap();
        std::fs::create_dir_all(auth_json_path().parent().unwrap()).unwrap();
        let original_auth = r#"{"OPENAI_API_KEY":"sk-real","auth_mode":"chatgpt","tokens":{"access_token":"jwt"}}"#;
        // Saved backup from the time AgentGate first ran.
        std::fs::write(saved_auth_path(), original_auth).unwrap();
        // Currently-polluted live auth.json (no tokens, no auth_mode).
        std::fs::write(auth_json_path(), r#"{"OPENAI_API_KEY":"ag_local_xyz"}"#).unwrap();

        let result = apply("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("access_token"), "repair must restore tokens");
        assert!(auth.contains("chatgpt"), "repair must restore auth_mode");
        cleanup(&temp);
    }

    #[test]
    fn test_toggle_provider_restores_config_only() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(config_path(), "model = \"gpt-4\"\nmodel_provider = \"openai\"\n").unwrap();
        let original_auth = r#"{"OPENAI_API_KEY":"sk-real","auth_mode":"chatgpt","tokens":{"access_token":"jwt"}}"#;
        std::fs::write(auth_json_path(), original_auth).unwrap();
        apply("127.0.0.1", 9090).unwrap();
        assert!(detect().is_agentgate_active);
        // Toggle back to official
        let result = toggle_provider("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("chatgpt"), "auth.json still untouched on toggle");
        assert!(auth.contains("access_token"));
        let cfg = std::fs::read_to_string(config_path()).unwrap();
        assert!(cfg.contains("model_provider = \"openai\""));
        // Toggle back to agentgate
        let result = toggle_provider("127.0.0.1", 9090).unwrap();
        assert!(result.success);
        assert_eq!(result.new_provider, "agentgate");
        // auth.json is STILL the original OAuth — never touched by toggle.
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("access_token"), "auth.json survives round-trip");
        assert!(auth.contains("chatgpt"));
        // config.toml has agentgate's bearer token, not auth.json.
        let cfg = std::fs::read_to_string(config_path()).unwrap();
        assert!(cfg.contains("experimental_bearer_token = \"ag_local_"));
        cleanup(&temp);
    }

    #[test]
    fn test_disable_restores_official_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        // Start with an official-style config (what a fresh Codex login produces).
        let original_cfg = "model_provider = \"openai\"\n[plugins.\"browser@openai-bundled\"]\nenabled = true\n";
        std::fs::write(config_path(), original_cfg).unwrap();
        let oauth_auth = r#"{"OPENAI_API_KEY":null,"auth_mode":"chatgpt","tokens":{"access_token":"jwt"}}"#;
        std::fs::write(auth_json_path(), oauth_auth).unwrap();

        // Apply AgentGate config (compat mode).
        apply("127.0.0.1", 9090).unwrap();
        assert!(detect().is_agentgate_active);

        // Disable: should restore the official block.
        let result = disable().unwrap();
        assert!(result.success);
        let cfg = std::fs::read_to_string(config_path()).unwrap();
        assert!(cfg.contains("model_provider = \"openai\""), "official model_provider restored");
        assert!(cfg.contains("browser@openai-bundled"), "official plugin block restored");
        assert!(!cfg.contains("agentgate"), "agentgate block should be gone");

        // auth.json must still be the OAuth one — disable never touches it
        // unless the polluted-repair branch fires (which doesn't apply here).
        let auth = std::fs::read_to_string(auth_json_path()).unwrap();
        assert!(auth.contains("access_token"));
        cleanup(&temp);
    }

    #[test]
    fn test_disable_errors_without_saved_backup() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        // No saved backup, no prior apply — disable should refuse rather
        // than blank out the user's config.
        let err = disable().unwrap_err();
        assert_eq!(err.code, "CODEX_NO_SAVED_FILES");
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
    fn test_detect_with_new_hijack_config() {
        // The new hijack-OpenAI format: model_provider = "OpenAI" but the
        // OpenAI block points at localhost with our ag_local_ bearer token.
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        let snippet = generate_snippet("127.0.0.1", 9090, "ag_local_xyz");
        std::fs::write(config_path(), snippet).unwrap();
        let status = detect();
        assert!(status.exists);
        assert!(status.has_agentgate, "ag_local_ bearer token marks our hijack snippet");
        assert!(status.is_agentgate_active);
        assert_eq!(status.current_provider, Some("OpenAI".to_string()));
        cleanup(&temp);
    }

    #[test]
    fn test_detect_with_legacy_agentgate_config() {
        // Backward compat: old AgentGate builds wrote `model_provider = "agentgate"`.
        // detect() should still recognise those rows so the upgrade flow can
        // surface "needs re-apply" guidance.
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(
            config_path(),
            "model_provider = \"agentgate\"\nmodel = \"gpt-5\"\n[model_providers.agentgate]\nbase_url = \"http://x\"\n",
        )
        .unwrap();
        let status = detect();
        assert!(status.exists);
        assert!(status.has_agentgate);
        assert_eq!(status.current_provider, Some("agentgate".to_string()));
        assert!(status.is_agentgate_active);
        cleanup(&temp);
    }

    #[test]
    fn test_detect_with_real_openai_config_is_not_agentgate() {
        // A user with a genuine [model_providers.OpenAI] block pointing at
        // api.openai.com (no ag_local_ bearer) must NOT be flagged as
        // AgentGate-active.
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(
            config_path(),
            "model_provider = \"OpenAI\"\n[model_providers.OpenAI]\nbase_url = \"https://api.openai.com/v1\"\n",
        )
        .unwrap();
        let status = detect();
        assert!(status.exists);
        assert!(!status.has_agentgate);
        assert!(!status.is_agentgate_active);
        cleanup(&temp);
    }
}
