use tauri::State;

use crate::app::state::AppState;
use crate::errors::AppError;

// ── Pricing Commands ──────────────────────────────────────────

#[tauri::command]
pub fn list_model_pricing(
    state: State<'_, AppState>,
) -> Result<Vec<crate::storage::pricing::ModelPricing>, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::list_all(&conn)
}

#[tauri::command]
pub fn upsert_model_pricing(
    state: State<'_, AppState>,
    provider: String,
    model_pattern: String,
    input_price: f64,
    output_price: f64,
) -> Result<crate::storage::pricing::ModelPricing, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::upsert_custom(
        &conn,
        &provider,
        &model_pattern,
        input_price,
        output_price,
    )
}

#[tauri::command]
pub fn delete_model_pricing(state: State<'_, AppState>, id: String) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    crate::storage::pricing::delete_custom(&conn, &id)
}

