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
