use tauri::State;

use crate::app::state::AppState;
use crate::errors::AppError;
use crate::storage;

// ── Config Import / Export ────────────────────────────────────

/// 导出当前配置为 JSON 字符串。前端拿到后用 Tauri dialog 保存到磁盘。
///
/// `include_secrets = false`（默认）会把 api_key 字段全部置空——导出文件可以
/// 安全分享/截图；用户在新机器导入后重新填密钥即可。`include_secrets = true`
/// 会把明文密钥写入文件，仅用于自己换机迁移这种场景，前端需要明确警告。
#[tauri::command]
#[specta::specta]
pub fn export_config_json(
    include_secrets: bool,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let dump = storage::config_backups::export(&conn, include_secrets)?;
    serde_json::to_string_pretty(&dump)
        .map_err(|e| AppError::internal(format!("Serialize export: {e}")))
}

/// 从前端拿到的 JSON 字符串还原配置。**replace 语义**：providers / route_profiles
/// / route_profile_providers 三张表会被先清空再重建。运行时状态（provider_runtime_status）
/// 一并清空（指向已不存在的 provider_id 没意义）；request_logs / pricing 等
/// 历史数据不受影响。
#[tauri::command]
#[specta::specta]
pub fn import_config_json(
    json: String,
    state: State<'_, AppState>,
) -> Result<storage::config_backups::ImportSummary, AppError> {
    let payload: storage::config_backups::ConfigExport =
        serde_json::from_str(&json).map_err(|e| {
            AppError::new(
                crate::errors::codes::CONFIG_IMPORT_PARSE_ERROR,
                format!("Invalid config JSON: {e}"),
            )
            .with_suggestion(
                "Make sure the file is an AgentGate config export, not a different JSON file.",
            )
        })?;
    let mut conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::config_backups::import(&mut conn, &payload)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    use super::*;
    use crate::app::state::AppState;
    use crate::models::provider::CreateProviderInput;
    use crate::storage;

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

    unsafe fn as_state<'r>(state: &'r AppState) -> tauri::State<'r, AppState> {
        std::mem::transmute(state)
    }

    fn seed_provider(state: &AppState) {
        let conn = state.db.get().unwrap();
        storage::providers::create(
            &conn,
            CreateProviderInput {
                name: "ExportProvider".to_string(),
                provider_type: "openai".to_string(),
                base_url: "https://api.openai.com".to_string(),
                api_key: Some("sk-secret".to_string()),
                default_model: "gpt-4".to_string(),
                protocol: r#"["openai_chat_completions"]"#.to_string(),
                timeout_seconds: Some(120),
                ..Default::default()
            },
        )
        .unwrap();
    }

    #[test]
    fn export_config_json_strips_secrets_by_default() {
        let state = test_state();
        seed_provider(&state);
        let json = export_config_json(false, unsafe { as_state(&state) }).unwrap();
        let export: serde_json::Value = serde_json::from_str(&json).unwrap();
        let provider = export["providers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "ExportProvider")
            .unwrap();
        assert!(provider["api_key"].is_null());
    }

    #[test]
    fn export_config_json_includes_secrets_when_requested() {
        let state = test_state();
        seed_provider(&state);
        let json = export_config_json(true, unsafe { as_state(&state) }).unwrap();
        let export: serde_json::Value = serde_json::from_str(&json).unwrap();
        let provider = export["providers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "ExportProvider")
            .unwrap();
        assert_eq!(provider["api_key"].as_str().unwrap(), "sk-secret");
    }

    #[test]
    fn import_config_json_round_trips() {
        let state = test_state();
        seed_provider(&state);
        let json = export_config_json(true, unsafe { as_state(&state) }).unwrap();

        let summary = import_config_json(json, unsafe { as_state(&state) }).unwrap();
        assert!(summary.providers_imported > 0);
        assert!(summary.secrets_applied);

        let providers = storage::providers::list_all(&state.db.get().unwrap()).unwrap();
        assert!(providers
            .iter()
            .any(|p| p.name == "ExportProvider" && p.api_key.as_deref() == Some("sk-secret")));
    }

    #[test]
    fn import_config_json_rejects_invalid_json() {
        let state = test_state();
        let err =
            import_config_json("not json".to_string(), unsafe { as_state(&state) }).unwrap_err();
        assert_eq!(err.code, "CONFIG_IMPORT_PARSE_ERROR");
    }
}
