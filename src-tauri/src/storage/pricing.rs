use rusqlite::{params, Connection};
use serde::Serialize;

use crate::errors::AppError;
use crate::storage::generated_provider_catalog as catalog;

#[derive(Debug, Clone, Serialize)]
pub struct ModelPricing {
    pub id: String,
    pub provider: String,
    pub model_pattern: String,
    pub input_price: f64,  // $/1M input tokens
    pub output_price: f64, // $/1M output tokens
    pub is_custom: bool,
    pub updated_at: String,
}

/// Ensure the model_pricing table has default entries.
pub fn ensure_defaults(conn: &Connection) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    for (provider, model, input_price, output_price) in catalog::MODEL_PRICING_DEFAULTS {
        insert_default(conn, &now, provider, model, *input_price, *output_price)?;
    }
    Ok(())
}

fn insert_default(
    conn: &Connection,
    now: &str,
    provider: &str,
    model: &str,
    input_price: f64,
    output_price: f64,
) -> Result<(), AppError> {
    let id = format!("default_{provider}_{model}");
    conn.execute(
        "INSERT OR IGNORE INTO model_pricing (id, provider, model_pattern, input_price, output_price, is_custom, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
        params![&id, provider, model, input_price, output_price, now],
    )?;
    Ok(())
}

/// Strip a trailing `[qualifier]` suffix like `mimo-v2.5-pro[1m]` →
/// `mimo-v2.5-pro`. MiMo and DeepSeek use this suffix to request long
/// context on the Claude Code endpoint, but the pricing table is keyed by
/// base model id, so matching needs the suffix removed.
fn strip_model_qualifier(model: &str) -> &str {
    if let Some(stripped) = model.strip_suffix(']') {
        if let Some(open) = stripped.rfind('[') {
            return &stripped[..open];
        }
    }
    model
}

/// Get the price for a specific provider + model.
/// Priority: exact custom match → exact default match → base-id match (qualifier
/// stripped) → wildcard custom → wildcard default → None.
pub fn get_price(conn: &Connection, provider: &str, model: &str) -> Option<(f64, f64)> {
    let provider_lower = provider.to_lowercase();

    // 1. Exact match (custom first, case-insensitive on provider)
    if let Ok(row) = conn.query_row(
        "SELECT input_price, output_price FROM model_pricing
         WHERE LOWER(provider) = ?1 AND model_pattern = ?2
         ORDER BY is_custom DESC LIMIT 1",
        params![&provider_lower, model],
        |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
    ) {
        return Some(row);
    }

    // 2. Base-id match after stripping a `[qualifier]` suffix.
    let base = strip_model_qualifier(model);
    if base != model {
        if let Ok(row) = conn.query_row(
            "SELECT input_price, output_price FROM model_pricing
             WHERE LOWER(provider) = ?1 AND model_pattern = ?2
             ORDER BY is_custom DESC LIMIT 1",
            params![&provider_lower, base],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
        ) {
            return Some(row);
        }
    }

    // 3. Wildcard match
    if let Ok(row) = conn.query_row(
        "SELECT input_price, output_price FROM model_pricing
         WHERE LOWER(provider) = ?1 AND model_pattern = '*'
         ORDER BY is_custom DESC LIMIT 1",
        params![&provider_lower],
        |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
    ) {
        return Some(row);
    }

    None
}

/// Calculate cost in USD from token counts and prices.
pub fn calculate_cost(input_tokens: Option<i64>, output_tokens: Option<i64>, input_price: f64, output_price: f64) -> f64 {
    let input = input_tokens.unwrap_or(0) as f64;
    let output = output_tokens.unwrap_or(0) as f64;
    (input * input_price + output * output_price) / 1_000_000.0
}

/// Calculate cost for a request, looking up the price from the DB.
pub fn calculate_cost_for_request(
    conn: &Connection,
    provider: &str,
    model: &str,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> Option<f64> {
    let (input_price, output_price) = get_price(conn, provider, model)?;
    let cost = calculate_cost(input_tokens, output_tokens, input_price, output_price);
    Some(cost)
}

/// Backfill cost for existing request_logs that have tokens but no cost.
pub fn backfill_costs(conn: &Connection) -> Result<u64, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, model, input_tokens, output_tokens FROM request_logs WHERE cost IS NULL AND (input_tokens IS NOT NULL OR output_tokens IS NOT NULL)"
    )?;
    let rows: Vec<(String, String, String, Option<i64>, Option<i64>)> = stmt.query_map([], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
    })?.filter_map(|r| r.ok()).collect();

    let mut updated = 0u64;
    for (id, provider, model, input_tokens, output_tokens) in &rows {
        if let Some(cost) = calculate_cost_for_request(conn, provider, model, *input_tokens, *output_tokens) {
            conn.execute("UPDATE request_logs SET cost = ?1 WHERE id = ?2", params![cost, id])?;
            updated += 1;
        }
    }
    Ok(updated)
}

/// List all pricing entries (default + custom).
pub fn list_all(conn: &Connection) -> Result<Vec<ModelPricing>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, provider, model_pattern, input_price, output_price, is_custom, updated_at
         FROM model_pricing ORDER BY provider, model_pattern",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ModelPricing {
            id: row.get(0)?,
            provider: row.get(1)?,
            model_pattern: row.get(2)?,
            input_price: row.get(3)?,
            output_price: row.get(4)?,
            is_custom: row.get::<_, i64>(5)? != 0,
            updated_at: row.get(6)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Add or update a custom pricing entry.
pub fn upsert_custom(
    conn: &Connection,
    provider: &str,
    model_pattern: &str,
    input_price: f64,
    output_price: f64,
) -> Result<ModelPricing, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = format!("custom_{provider}_{model_pattern}");

    conn.execute(
        "INSERT INTO model_pricing (id, provider, model_pattern, input_price, output_price, is_custom, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)
         ON CONFLICT(id) DO UPDATE SET input_price=?4, output_price=?5, updated_at=?6",
        params![&id, provider, model_pattern, input_price, output_price, &now],
    )?;

    Ok(ModelPricing {
        id,
        provider: provider.to_string(),
        model_pattern: model_pattern.to_string(),
        input_price,
        output_price,
        is_custom: true,
        updated_at: now,
    })
}

/// Delete a custom pricing entry. Cannot delete built-in defaults.
pub fn delete_custom(conn: &Connection, id: &str) -> Result<bool, AppError> {
    let rows = conn.execute(
        "DELETE FROM model_pricing WHERE id = ?1 AND is_custom = 1",
        [id],
    )?;
    Ok(rows > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE model_pricing (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model_pattern TEXT NOT NULL,
                input_price REAL NOT NULL,
                output_price REAL NOT NULL,
                is_custom INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            )"
        ).unwrap();
        conn
    }

    #[test]
    fn test_ensure_defaults() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        let all = list_all(&conn).unwrap();
        assert!(all.len() >= 10);
        assert!(all.iter().all(|p| !p.is_custom));
    }

    #[test]
    fn test_get_price_exact() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        let price = get_price(&conn, "deepseek", "deepseek-v4-pro");
        assert!(price.is_some());
        let (inp, out) = price.unwrap();
        assert!((inp - 2.0).abs() < 0.01);
        assert!((out - 8.0).abs() < 0.01);
    }

    #[test]
    fn mimo_defaults_present() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        let (inp, out) = get_price(&conn, "mimo", "mimo-v2.5-pro").expect("v2.5-pro priced");
        assert!((inp - 0.435).abs() < 1e-4);
        assert!((out - 0.87).abs() < 1e-4);

        let (inp, out) = get_price(&conn, "mimo", "mimo-v2-flash").expect("flash priced");
        assert!((inp - 0.10).abs() < 1e-4);
        assert!((out - 0.30).abs() < 1e-4);

        // TTS family is free
        let (inp, out) = get_price(&conn, "mimo", "mimo-v2.5-tts").expect("tts priced");
        assert_eq!(inp, 0.0);
        assert_eq!(out, 0.0);
    }

    #[test]
    fn qualifier_suffix_strips_to_base_id() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        // [1m] suffix used by Claude Code path on MiMo / DeepSeek must
        // still match the base-id price entry.
        let mimo = get_price(&conn, "mimo", "mimo-v2.5-pro[1m]")
            .expect("mimo-v2.5-pro[1m] should resolve to mimo-v2.5-pro");
        let ds = get_price(&conn, "deepseek", "deepseek-v4-pro[1m]")
            .expect("deepseek-v4-pro[1m] should resolve to deepseek-v4-pro");
        assert!((mimo.0 - 0.435).abs() < 1e-4);
        assert!((ds.0 - 2.0).abs() < 1e-4);
    }

    #[test]
    fn unknown_qualifier_falls_through_to_base() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        // Any qualifier is stripped before matching; bogus qualifier still resolves.
        let price = get_price(&conn, "mimo", "mimo-v2.5-pro[128k]");
        assert!(price.is_some());
    }

    #[test]
    fn test_get_price_wildcard() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        let price = get_price(&conn, "groq", "llama-3.3-70b");
        assert!(price.is_some());
        let (inp, out) = price.unwrap();
        assert!((inp - 0.0).abs() < 0.01);
        assert!((out - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_get_price_unknown() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        assert!(get_price(&conn, "unknown_provider", "unknown_model").is_none());
    }

    #[test]
    fn test_calculate_cost() {
        // 1000 input tokens at $2/1M + 500 output tokens at $8/1M
        let cost = calculate_cost(Some(1000), Some(500), 2.0, 8.0);
        assert!((cost - 0.006).abs() < 0.0001);
    }

    #[test]
    fn test_custom_overrides_default() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        upsert_custom(&conn, "deepseek", "deepseek-v4-pro", 99.0, 99.0).unwrap();
        let price = get_price(&conn, "deepseek", "deepseek-v4-pro").unwrap();
        assert!((price.0 - 99.0).abs() < 0.01);
    }

    #[test]
    fn test_delete_custom() {
        let conn = setup_db();
        ensure_defaults(&conn).unwrap();
        upsert_custom(&conn, "test", "model", 1.0, 2.0).unwrap();
        assert!(delete_custom(&conn, "custom_test_model").unwrap());
        assert!(!delete_custom(&conn, "default_deepseek_deepseek-v4-pro").unwrap()); // can't delete defaults
    }
}
