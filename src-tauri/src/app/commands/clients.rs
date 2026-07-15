use tauri::State;

use super::record_pre_apply;
use crate::app::state::AppState;
use crate::errors::AppError;
use crate::models::settings::ToolConfigView;
use crate::storage;

fn dirs_next() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE").ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("HOME").ok()
    }
}

// ── Tool Commands ──────────────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn list_tools() -> Result<Vec<ToolConfigView>, AppError> {
    let home = dirs_next().unwrap_or_default();

    let tools = vec![
        ToolConfigView {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            slug: "claude-code".to_string(),
            icon: "terminal".to_string(),
            config_path: format!("{}/.claude/settings.json", home),
            description:
                "Anthropic's CLI for Claude. Agentic coding tool with terminal integration."
                    .to_string(),
            config_exists: std::path::Path::new(&format!("{}/.claude/settings.json", home))
                .exists(),
        },
        ToolConfigView {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            slug: "codex".to_string(),
            icon: "code".to_string(),
            config_path: format!("{}/.codex/config.toml", home),
            description:
                "OpenAI's CLI coding agent. Supports OpenAI Responses API and chat completions."
                    .to_string(),
            config_exists: std::path::Path::new(&format!("{}/.codex/config.toml", home)).exists(),
        },
        ToolConfigView {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            slug: "opencode".to_string(),
            icon: "braces".to_string(),
            config_path: format!("{}/.config/opencode/opencode.json", home),
            description: "Open-source terminal AI coding assistant. Supports multiple providers."
                .to_string(),
            config_exists: std::path::Path::new(&format!(
                "{}/.config/opencode/opencode.json",
                home
            ))
            .exists(),
        },
        ToolConfigView {
            id: "atomcode".to_string(),
            name: "AtomCode".to_string(),
            slug: "atomcode".to_string(),
            icon: "atom".to_string(),
            config_path: format!("{}/.atomcode/config.toml", home),
            description:
                "Open-source AI coding agent in your terminal. Uses OpenAI-compatible API."
                    .to_string(),
            config_exists: std::path::Path::new(&format!("{}/.atomcode/config.toml", home))
                .exists(),
        },
        ToolConfigView {
            id: "gemini_cli".to_string(),
            name: "Gemini CLI".to_string(),
            slug: "gemini-cli".to_string(),
            icon: "sparkles".to_string(),
            config_path: format!("{}/.gemini/settings.json", home),
            description:
                "Google's AI coding CLI. Uses Gemini API with OpenAI-compatible endpoint support."
                    .to_string(),
            config_exists: std::path::Path::new(&format!("{}/.gemini/settings.json", home))
                .exists(),
        },
    ];

    Ok(tools)
}

#[tauri::command]
#[specta::specta]
pub fn generate_codex_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    let token = crate::security::local_token::ensure_token()?;
    Ok(crate::tools::codex::generate_snippet(
        &settings.host,
        settings.port,
        &token,
    ))
}

// ── Codex Config Commands ──────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn detect_codex_config() -> Result<crate::tools::codex::CodexConfigStatus, AppError> {
    Ok(crate::tools::codex::detect())
}

#[tauri::command]
#[specta::specta]
pub fn apply_codex_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::codex::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::Codex,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };

    record_pre_apply(
        &state,
        "codex",
        "apply",
        crate::tools::codex::snapshot_paths(),
        "apply",
    );
    crate::tools::codex::apply(&host, port)
}

#[tauri::command]
#[specta::specta]
pub fn toggle_codex_provider(
    state: State<'_, AppState>,
) -> Result<crate::tools::codex::ToggleResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::Codex,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    record_pre_apply(
        &state,
        "codex",
        "toggle",
        crate::tools::codex::snapshot_paths(),
        "toggle",
    );
    crate::tools::codex::toggle_provider(&host, port)
}

/// Restore Codex to its pre-AgentGate state — the saved config.toml is
/// copied back so the user gets the official `[plugins.*]` / `[mcp_servers.*]`
/// blocks alive again. Used by the UI's "Switch to native mode" button.
#[tauri::command]
#[specta::specta]
pub fn disable_codex_agentgate(
    state: State<'_, AppState>,
) -> Result<crate::tools::codex::ApplyConfigResult, AppError> {
    record_pre_apply(
        &state,
        "codex",
        "disable",
        crate::tools::codex::snapshot_paths(),
        "disable",
    );
    crate::tools::codex::disable()
}

#[tauri::command]
#[specta::specta]
pub fn open_codex_config() -> Result<bool, AppError> {
    crate::tools::codex::open_config()?;
    Ok(true)
}

// ── Claude Desktop Commands（第一阶段：只读 detect + profile 预览，不写盘）──

#[tauri::command]
#[specta::specta]
pub fn detect_claude_desktop() -> crate::tools::claude_desktop::ClaudeDesktopStatus {
    crate::tools::claude_desktop::detect()
}

/// 生成指向 AgentGate 网关的 3p profile JSON（pretty），仅供和用户机器上实际的
/// Claude Desktop 3p 配置对比、确认 schema，不写任何文件。
#[tauri::command]
#[specta::specta]
pub fn preview_claude_desktop_profile(state: State<'_, AppState>) -> Result<String, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let s = storage::gateway_settings::get(&conn)?;
        (s.host, s.port)
    };
    let token = crate::security::local_token::ensure_token()?;
    let profile = crate::tools::claude_desktop::generate_profile(&host, port, &token);
    serde_json::to_string_pretty(&profile)
        .map_err(|e| AppError::internal(format!("serialize profile failed: {e}")))
}

/// 接入 Claude Desktop：写 3p profile + 切 appliedId 到 AgentGate。apply 前先经
/// apply_history 快照 profile/_meta，用户可在客户端历史里一键回滚。
#[tauri::command]
#[specta::specta]
pub fn apply_claude_desktop_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::claude_desktop::ClaudeDesktopApplyResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    let token = crate::security::local_token::ensure_token()?;
    record_pre_apply(
        &state,
        "claude_desktop",
        "apply",
        crate::tools::claude_desktop::snapshot_paths(),
        "apply",
    );
    crate::tools::claude_desktop::apply(&host, port, &token)
}

// ── Claude Code Commands ──────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn detect_claude_code_env() -> Result<crate::tools::claude_code::ClaudeCodeEnvStatus, AppError>
{
    Ok(crate::tools::claude_code::detect_env())
}

#[tauri::command]
#[specta::specta]
pub fn apply_claude_code_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::claude_code::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::ClaudeCode,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    record_pre_apply(
        &state,
        "claude_code",
        "apply",
        crate::tools::claude_code::snapshot_paths(),
        "apply",
    );
    crate::tools::claude_code::apply_config(&host, port, "claude-sonnet-4-7")
}

#[tauri::command]
#[specta::specta]
pub fn toggle_claude_code_provider(
    state: State<'_, AppState>,
) -> Result<crate::tools::claude_code::ToggleResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let _ = storage::recommended_mappings::supplement_active_provider(
            &conn,
            storage::recommended_mappings::MappingProfile::ClaudeCode,
        );
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    record_pre_apply(
        &state,
        "claude_code",
        "toggle",
        crate::tools::claude_code::snapshot_paths(),
        "toggle",
    );
    crate::tools::claude_code::toggle_provider(&host, port, "claude-sonnet-4-7")
}

#[tauri::command]
#[specta::specta]
pub fn open_claude_code_config() -> Result<bool, AppError> {
    crate::tools::claude_code::open_config()?;
    Ok(true)
}

#[tauri::command]
#[specta::specta]
pub fn generate_claude_code_env(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::claude_code::generate_env_snippet(
        &settings.host,
        settings.port,
        "claude-sonnet-4-7",
    ))
}

// ── OpenCode Commands ─────────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn detect_opencode_config() -> Result<crate::tools::opencode::OpenCodeConfigStatus, AppError> {
    Ok(crate::tools::opencode::detect())
}

#[tauri::command]
#[specta::specta]
pub fn apply_opencode_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::opencode::ApplyConfigResult, AppError> {
    let (host, port) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        (settings.host, settings.port)
    };
    record_pre_apply(
        &state,
        "opencode",
        "apply",
        crate::tools::opencode::snapshot_paths(),
        "apply",
    );
    crate::tools::opencode::apply(&host, port)
}

#[tauri::command]
#[specta::specta]
pub fn generate_opencode_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::opencode::generate_snippet(
        &settings.host,
        settings.port,
    ))
}

#[tauri::command]
#[specta::specta]
pub fn open_opencode_config() -> Result<bool, AppError> {
    crate::tools::opencode::open_config()?;
    Ok(true)
}

// ── Gemini CLI Config Commands ─────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn detect_gemini_config() -> Result<crate::tools::gemini_cli::GeminiCliConfigStatus, AppError> {
    Ok(crate::tools::gemini_cli::detect())
}

#[tauri::command]
#[specta::specta]
pub fn apply_gemini_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::gemini_cli::ApplyConfigResult, AppError> {
    let (host, port, model) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gemini-2.5-flash".to_string());
        (settings.host, settings.port, model)
    };
    record_pre_apply(
        &state,
        "gemini",
        "apply",
        crate::tools::gemini_cli::snapshot_paths(),
        "apply",
    );
    crate::tools::gemini_cli::apply(&host, port, &model)
}

#[tauri::command]
#[specta::specta]
pub fn generate_gemini_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    Ok(crate::tools::gemini_cli::generate_snippet(
        &settings.host,
        settings.port,
        "gemini-2.5-flash",
    ))
}

#[tauri::command]
#[specta::specta]
pub fn toggle_gemini_provider(
    state: State<'_, AppState>,
) -> Result<crate::tools::gemini_cli::ToggleResult, AppError> {
    let (host, port, model) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gemini-2.5-flash".to_string());
        (settings.host, settings.port, model)
    };
    record_pre_apply(
        &state,
        "gemini",
        "toggle",
        crate::tools::gemini_cli::snapshot_paths(),
        "toggle",
    );
    crate::tools::gemini_cli::toggle(&host, port, &model)
}

#[tauri::command]
#[specta::specta]
pub fn open_gemini_config() -> Result<bool, AppError> {
    crate::tools::gemini_cli::open_config()?;
    Ok(true)
}

// ── AtomCode Config Commands ──────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn detect_atomcode_config() -> Result<crate::tools::atomcode::AtomCodeConfigStatus, AppError> {
    Ok(crate::tools::atomcode::detect())
}

#[tauri::command]
#[specta::specta]
pub fn apply_atomcode_config(
    state: State<'_, AppState>,
) -> Result<crate::tools::atomcode::ApplyConfigResult, AppError> {
    let (host, port, model) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gpt-5.5".to_string());
        (settings.host, settings.port, model)
    };
    record_pre_apply(
        &state,
        "atomcode",
        "apply",
        crate::tools::atomcode::snapshot_paths(),
        "apply",
    );
    crate::tools::atomcode::apply(&host, port, &model)
}

#[tauri::command]
#[specta::specta]
pub fn generate_atomcode_config(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    let provider_id = settings.active_provider_id.clone().unwrap_or_default();
    let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
    let model = provider
        .map(|p| p.default_model)
        .unwrap_or_else(|| "gpt-5.5".to_string());
    Ok(crate::tools::atomcode::generate_snippet(
        &settings.host,
        settings.port,
        &model,
    ))
}

/// After a client's config is rewritten, look up matching live processes
/// so the UI can warn the user that the existing session needs to be
/// restarted to pick up the new config. Each `client_id` maps to one or
/// more process basenames (e.g. `codex` matches both the CLI and the
/// macOS desktop app). Returns an empty list on Windows (pgrep-only
/// detection); the caller treats empty as "couldn't detect", not "OK".
#[tauri::command]
#[specta::specta]
pub fn detect_client_running(
    client_id: String,
) -> Result<Vec<crate::tools::process_detect::RunningProcess>, AppError> {
    let needles: &[&str] = match client_id.as_str() {
        "codex" => &["codex"],
        "claude_code" => &["claude"],
        "opencode" => &["opencode"],
        "gemini" => &["gemini"],
        "atomcode" => &["atomcode"],
        _ => return Err(AppError::validation("unknown client_id")),
    };
    Ok(crate::tools::process_detect::find_running(needles))
}

/// Restart Codex Desktop so freshly-written config.toml / auth.json take
/// effect. macOS only at the moment — `restart_codex_desktop` returns
/// `supported: false` on other platforms and the UI hides the button.
/// Never called automatically; only fires when the user clicks the button in
/// PostApplyDialog.
#[tauri::command]
#[specta::specta]
pub fn restart_codex_desktop() -> Result<crate::tools::codex_restart::CodexRestartResult, AppError>
{
    crate::tools::codex_restart::restart()
}

/// 读取各客户端(Codex / Claude Code)现有的 MCP server 配置，汇总展示。
/// 以客户端文件为真相源，只读不写；env 只返回 key 不返回 value。
#[tauri::command]
#[specta::specta]
pub fn list_mcp_servers() -> Result<Vec<crate::tools::mcp::McpServer>, AppError> {
    Ok(crate::tools::mcp::list_all())
}

/// 添加或更新指定客户端的 MCP server。只写入一个客户端配置文件，不做跨客户端同步。
#[tauri::command]
#[specta::specta]
pub fn upsert_mcp_server(
    input: crate::tools::mcp::UpsertMcpServerInput,
) -> Result<crate::tools::mcp::McpServer, AppError> {
    crate::tools::mcp::upsert(input)
}

/// 删除指定客户端的 MCP server。文件或 server 不存在时返回 false。
#[tauri::command]
#[specta::specta]
pub fn delete_mcp_server(client: String, name: String) -> Result<bool, AppError> {
    crate::tools::mcp::delete(&client, &name)
}

/// 将一个客户端里的 MCP server 显式同步到一个或多个目标客户端。
#[tauri::command]
#[specta::specta]
pub fn sync_mcp_server(
    input: crate::tools::mcp::SyncMcpServerInput,
) -> Result<Vec<crate::tools::mcp::McpServer>, AppError> {
    crate::tools::mcp::sync(input)
}

/// 导出 MCP server 配置。默认由前端传 include_secrets=false，不导出 env value。
#[tauri::command]
#[specta::specta]
pub fn export_mcp_servers(include_secrets: bool) -> Result<String, AppError> {
    crate::tools::mcp::export_config(include_secrets)
}

/// 从 JSON 文本导入 MCP server 配置到指定客户端。
#[tauri::command]
#[specta::specta]
pub fn import_mcp_servers(
    payload: String,
    target_clients: Vec<String>,
) -> Result<Vec<crate::tools::mcp::McpServer>, AppError> {
    crate::tools::mcp::import_config(&payload, target_clients)
}

/// 曾经 apply 过配置的客户端 id 列表。前端用来判断「配置漂移」：客户端 detected
/// 但 id 在这个列表里，说明接入过又被改回去了，提示重新应用。
#[tauri::command]
#[specta::specta]
pub fn clients_with_apply_history(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::apply_history::distinct_clients(&conn)
}

/// 列出某客户端的 apply/disable/toggle 历史（按时间倒序）。前端用来
/// 渲染历史抽屉。
#[tauri::command]
#[specta::specta]
pub fn list_client_apply_history(
    state: State<'_, AppState>,
    client_id: String,
) -> Result<Vec<storage::apply_history::HistoryEntry>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::apply_history::list(&conn, &client_id)
}

/// 回滚到某条历史记录所代表的盘上状态。snapshot 反序列化后按 file 写回原
/// absolute_path（不存在的文件被删除）。回滚本身**不**记录新历史，避免反复
/// 回滚把保留窗撑满。
#[tauri::command]
#[specta::specta]
pub fn rollback_client_apply(
    state: State<'_, AppState>,
    history_id: String,
) -> Result<storage::apply_history::HistoryEntry, AppError> {
    let entry = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        storage::apply_history::get(&conn, &history_id)?
    };
    let snapshot: storage::apply_history::ClientSnapshot =
        serde_json::from_str(&entry.snapshot_json)
            .map_err(|e| AppError::internal(format!("snapshot deserialise failed: {e}")))?;
    storage::apply_history::restore_files(&snapshot)?;
    Ok(entry)
}

/// 删除一条配置历史记录(初始快照受保护,不可删)。客户端配置历史和全局指令
/// 历史共用 `client_apply_history` 表,故两处删除都走这里。
#[tauri::command]
#[specta::specta]
pub fn delete_client_apply_history(
    state: State<'_, AppState>,
    history_id: String,
) -> Result<(), AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::apply_history::delete(&conn, &history_id)
}

#[tauri::command]
#[specta::specta]
pub fn toggle_atomcode_provider(
    state: State<'_, AppState>,
) -> Result<crate::tools::atomcode::ToggleResult, AppError> {
    let (host, port, model) = {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        let settings = storage::gateway_settings::get(&conn)?;
        let provider_id = settings.active_provider_id.clone().unwrap_or_default();
        let provider = storage::providers::get_by_id(&conn, &provider_id).ok();
        let model = provider
            .map(|p| p.default_model)
            .unwrap_or_else(|| "gpt-5.5".to_string());
        (settings.host, settings.port, model)
    };
    record_pre_apply(
        &state,
        "atomcode",
        "toggle",
        crate::tools::atomcode::snapshot_paths(),
        "toggle",
    );
    crate::tools::atomcode::toggle(&host, port, &model)
}

#[tauri::command]
#[specta::specta]
pub fn open_atomcode_config() -> Result<bool, AppError> {
    crate::tools::atomcode::open_config()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    use super::*;
    use crate::app::state::AppState;
    use crate::test_utils::{cleanup, setup_temp_home, FS_LOCK};

    fn test_state() -> AppState {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        {
            let conn = pool.get().unwrap();
            crate::storage::migrations::run_migrations(&conn).unwrap();
        }
        AppState {
            db: pool,
            gateway_runtime: Arc::new(Mutex::new(
                crate::models::gateway::GatewayRuntimeState::default(),
            )),
            wake: crate::wake::WakeManager::new(),
            pet_click_through: Arc::new(Mutex::new(false)),
        }
    }

    /// Convert a borrowed `AppState` into Tauri's `State` wrapper.
    /// This is test-only: `State` is a single-field struct around `&AppState`.
    unsafe fn as_state<'r>(state: &'r AppState) -> tauri::State<'r, AppState> {
        std::mem::transmute(state)
    }

    #[test]
    fn list_tools_returns_five_clients() {
        let tools = list_tools().unwrap();
        assert_eq!(tools.len(), 5);
        let ids: Vec<_> = tools.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"claude-code"));
        assert!(ids.contains(&"codex"));
        assert!(ids.contains(&"opencode"));
        assert!(ids.contains(&"atomcode"));
        assert!(ids.contains(&"gemini_cli"));
    }

    #[test]
    fn list_tools_reflects_config_existence() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let codex_dir = temp.join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(codex_dir.join("config.toml"), "[model_providers.OpenAI]\n").unwrap();

        let tools = list_tools().unwrap();
        let codex = tools.iter().find(|t| t.id == "codex").unwrap();
        assert!(codex.config_exists);
        cleanup(&temp);
    }

    #[test]
    fn generate_codex_config_uses_gateway_settings_and_token() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        let snippet = generate_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(snippet.contains("model_provider = \"OpenAI\""));
        assert!(snippet.contains("requires_openai_auth = true"));
        assert!(snippet.contains("experimental_bearer_token = \"ag_local_"));
        cleanup(&temp);
    }

    #[test]
    fn detect_codex_config_reports_missing_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let status = detect_codex_config().unwrap();
        assert!(!status.exists);
        assert!(!status.has_agentgate);
        cleanup(&temp);
    }

    #[test]
    fn detect_codex_config_recognises_agentgate_snippet() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let codex_dir = temp.join(".codex");
        std::fs::create_dir_all(&codex_dir).unwrap();
        std::fs::write(
            codex_dir.join("config.toml"),
            crate::tools::codex::generate_snippet("127.0.0.1", 9090, "ag_local_testtoken"),
        )
        .unwrap();
        let status = detect_codex_config().unwrap();
        assert!(status.exists);
        assert!(status.has_agentgate);
        assert!(status.is_agentgate_active);
        assert_eq!(status.current_provider, Some("OpenAI".to_string()));
        cleanup(&temp);
    }

    #[test]
    fn apply_codex_config_creates_hijack_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        let result = apply_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        assert!(crate::tools::codex::config_path().exists());
        let cfg = std::fs::read_to_string(crate::tools::codex::config_path()).unwrap();
        assert!(cfg.contains("model_provider = \"OpenAI\""));
        assert!(cfg.contains("requires_openai_auth = true"));
        cleanup(&temp);
    }

    #[test]
    fn apply_codex_config_records_history() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        let result = apply_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        let clients = clients_with_apply_history(unsafe { as_state(&state) }).unwrap();
        assert!(clients.contains(&"codex".to_string()));
        let history =
            list_client_apply_history(unsafe { as_state(&state) }, "codex".to_string()).unwrap();
        assert!(!history.is_empty());
        assert_eq!(history[0].action, "apply");
        cleanup(&temp);
    }

    #[test]
    fn toggle_codex_provider_round_trip() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        // Start from an official config so apply saves a restore point.
        std::fs::create_dir_all(crate::tools::codex::config_path().parent().unwrap()).unwrap();
        std::fs::write(
            crate::tools::codex::config_path(),
            "model_provider = \"openai\"\n",
        )
        .unwrap();

        let result = apply_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        assert!(crate::tools::codex::detect().is_agentgate_active);

        let result = toggle_codex_provider(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        assert!(!crate::tools::codex::detect().is_agentgate_active);

        let result = toggle_codex_provider(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        assert_eq!(result.new_provider, "agentgate");
        assert!(crate::tools::codex::detect().is_agentgate_active);
        cleanup(&temp);
    }

    #[test]
    fn disable_codex_agentgate_restores_official_config() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        std::fs::create_dir_all(crate::tools::codex::config_path().parent().unwrap()).unwrap();
        std::fs::write(
            crate::tools::codex::config_path(),
            "model_provider = \"openai\"\n[plugins.\"browser@openai-bundled\"]\nenabled = true\n",
        )
        .unwrap();

        let result = apply_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        assert!(crate::tools::codex::detect().is_agentgate_active);

        let result = disable_codex_agentgate(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        let cfg = std::fs::read_to_string(crate::tools::codex::config_path()).unwrap();
        assert!(cfg.contains("model_provider = \"openai\""));
        assert!(cfg.contains("browser@openai-bundled"));
        cleanup(&temp);
    }

    #[test]
    fn clients_with_apply_history_includes_codex_after_apply() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        let result = apply_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        let clients = clients_with_apply_history(unsafe { as_state(&state) }).unwrap();
        assert!(clients.contains(&"codex".to_string()));
        cleanup(&temp);
    }

    #[test]
    fn list_client_apply_history_returns_entries() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        let result = apply_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        let history =
            list_client_apply_history(unsafe { as_state(&state) }, "codex".to_string()).unwrap();
        assert!(!history.is_empty());
        cleanup(&temp);
    }

    #[test]
    fn delete_client_apply_history_removes_entry() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let state = test_state();
        // First apply creates a protected initial snapshot.
        let result = apply_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        // Second apply creates a deletable entry.
        let result = apply_codex_config(unsafe { as_state(&state) }).unwrap();
        assert!(result.success);
        let history =
            list_client_apply_history(unsafe { as_state(&state) }, "codex".to_string()).unwrap();
        assert!(history.len() >= 2);
        let id = history
            .iter()
            .find(|h| !h.is_initial)
            .map(|h| h.id.clone())
            .unwrap();
        delete_client_apply_history(unsafe { as_state(&state) }, id.clone()).unwrap();
        let history =
            list_client_apply_history(unsafe { as_state(&state) }, "codex".to_string()).unwrap();
        assert!(!history.iter().any(|h| h.id == id));
        cleanup(&temp);
    }

    #[test]
    fn detect_client_running_rejects_unknown_client() {
        let err = detect_client_running("unknown".to_string()).unwrap_err();
        assert_eq!(err.code, "VALIDATION_ERROR");
    }

    #[test]
    fn detect_client_running_returns_list_for_known_client() {
        let result = detect_client_running("codex".to_string()).unwrap();
        // On macOS/Linux pgrep may find nothing in CI; empty is a valid shape.
        assert!(result.is_empty() || result.iter().all(|p| !p.command.is_empty()));
    }
}
