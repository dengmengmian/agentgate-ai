use rusqlite::Connection;

use crate::errors::AppError;
use crate::models::request_log::{RequestLogDetail, RequestLogFilter, RequestLogListItem};

pub fn list(conn: &Connection, filter: RequestLogFilter) -> Result<Vec<RequestLogListItem>, AppError> {
    let mut sql = String::from(
        "SELECT id, request_id, timestamp, client, provider, model, route, status_code, latency_ms, error_message
         FROM request_logs WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    if let Some(ref client) = filter.client {
        sql.push_str(&format!(" AND client = ?{idx}"));
        param_values.push(Box::new(client.clone()));
        idx += 1;
    }

    if let Some(ref provider) = filter.provider {
        sql.push_str(&format!(" AND provider = ?{idx}"));
        param_values.push(Box::new(provider.clone()));
        idx += 1;
    }

    if let Some(ref status) = filter.status {
        match status.as_str() {
            "success" => {
                sql.push_str(&format!(" AND status_code >= ?{} AND status_code < ?{}", idx, idx + 1));
                param_values.push(Box::new(200i64));
                param_values.push(Box::new(300i64));
                idx += 2;
            }
            "error" => {
                sql.push_str(&format!(" AND (status_code >= ?{} OR status_code < ?{})", idx, idx + 1));
                param_values.push(Box::new(400i64));
                param_values.push(Box::new(200i64));
                idx += 2;
            }
            _ => {}
        }
    }

    if let Some(ref keyword) = filter.keyword {
        let like = format!("%{keyword}%");
        sql.push_str(&format!(
            " AND (request_id LIKE ?{idx} OR error_message LIKE ?{} OR model LIKE ?{} OR route LIKE ?{})",
            idx + 1,
            idx + 2,
            idx + 3
        ));
        param_values.push(Box::new(like.clone()));
        param_values.push(Box::new(like.clone()));
        param_values.push(Box::new(like.clone()));
        param_values.push(Box::new(like));
        idx += 4;
    }

    sql.push_str(" ORDER BY timestamp DESC");

    let limit = filter.limit.unwrap_or(100);
    let offset = filter.offset.unwrap_or(0);
    sql.push_str(&format!(" LIMIT ?{idx} OFFSET ?{}", idx + 1));
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let mut stmt = conn.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

    let rows = stmt.query_map(params_ref.as_slice(), |row| {
        Ok(RequestLogListItem {
            id: row.get(0)?,
            request_id: row.get(1)?,
            timestamp: row.get(2)?,
            client: row.get(3)?,
            provider: row.get(4)?,
            model: row.get(5)?,
            route: row.get(6)?,
            status_code: row.get(7)?,
            latency_ms: row.get(8)?,
            error_message: row.get(9)?,
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

pub fn get_detail(conn: &Connection, id: &str) -> Result<RequestLogDetail, AppError> {
    conn.query_row(
        "SELECT id, request_id, timestamp, client, provider, model, route, status_code,
                latency_ms, input_tokens, output_tokens, raw_request, converted_request,
                raw_response, converted_response, sse_events, tool_calls, error_message, trace_json
         FROM request_logs WHERE id = ?1",
        [id],
        |row| {
            Ok(RequestLogDetail {
                id: row.get(0)?,
                request_id: row.get(1)?,
                timestamp: row.get(2)?,
                client: row.get(3)?,
                provider: row.get(4)?,
                model: row.get(5)?,
                route: row.get(6)?,
                status_code: row.get(7)?,
                latency_ms: row.get(8)?,
                input_tokens: row.get(9)?,
                output_tokens: row.get(10)?,
                raw_request: row.get(11)?,
                converted_request: row.get(12)?,
                raw_response: row.get(13)?,
                converted_response: row.get(14)?,
                sse_events: row.get(15)?,
                tool_calls: row.get(16)?,
                error_message: row.get(17)?,
                trace_json: row.get(18)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => AppError::not_found("RequestLog", id),
        other => AppError::database(other),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn insert(
    conn: &Connection,
    request_id: &str,
    client: &str,
    provider: &str,
    model: &str,
    route: &str,
    status_code: i64,
    latency_ms: i64,
    raw_request: Option<&str>,
    converted_request: Option<&str>,
    raw_response: Option<&str>,
    converted_response: Option<&str>,
    sse_events: Option<&str>,
    tool_calls: Option<&str>,
    error_message: Option<&str>,
    trace_json: Option<&str>,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO request_logs (id, request_id, timestamp, client, provider, model, route,
                status_code, latency_ms, raw_request, converted_request, raw_response,
                converted_response, sse_events, tool_calls, error_message, trace_json,
                input_tokens, output_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        rusqlite::params![
            &id, request_id, &now, client, provider, model, route,
            status_code, latency_ms, raw_request, converted_request, raw_response,
            converted_response, sse_events, tool_calls, error_message, trace_json,
            input_tokens, output_tokens,
        ],
    )?;
    Ok(())
}

pub fn clear(conn: &Connection) -> Result<bool, AppError> {
    conn.execute("DELETE FROM request_logs", [])?;
    Ok(true)
}

/// Get request statistics.
pub fn get_stats(conn: &Connection) -> Result<RequestStats, AppError> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs", [], |r| r.get(0))?;
    let success: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs WHERE status_code >= 200 AND status_code < 300", [], |r| r.get(0))?;
    let errors: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs WHERE status_code >= 400 OR status_code < 200", [], |r| r.get(0))?;
    let avg_latency: f64 = conn.query_row("SELECT COALESCE(AVG(latency_ms), 0) FROM request_logs WHERE status_code >= 200 AND status_code < 300", [], |r| r.get(0))?;

    // Today's stats
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let today_total: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs WHERE timestamp LIKE ?1", [&format!("{today}%")], |r| r.get(0))?;
    let today_errors: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs WHERE timestamp LIKE ?1 AND (status_code >= 400 OR status_code < 200)", [&format!("{today}%")], |r| r.get(0))?;

    // Daily stats (last 7 days)
    let mut daily = Vec::new();
    for i in 0..7 {
        let day = (chrono::Utc::now() - chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs WHERE timestamp LIKE ?1", [&format!("{day}%")], |r| r.get(0))?;
        let errs: i64 = conn.query_row("SELECT COUNT(*) FROM request_logs WHERE timestamp LIKE ?1 AND (status_code >= 400 OR status_code < 200)", [&format!("{day}%")], |r| r.get(0))?;
        let day_input: i64 = conn.query_row("SELECT COALESCE(SUM(input_tokens), 0) FROM request_logs WHERE timestamp LIKE ?1", [&format!("{day}%")], |r| r.get(0))?;
        let day_output: i64 = conn.query_row("SELECT COALESCE(SUM(output_tokens), 0) FROM request_logs WHERE timestamp LIKE ?1", [&format!("{day}%")], |r| r.get(0))?;
        daily.push(DailyStat { date: day, total: count, errors: errs, success: count - errs, input_tokens: day_input, output_tokens: day_output });
    }
    daily.reverse();

    // Token totals
    let total_input_tokens: i64 = conn.query_row("SELECT COALESCE(SUM(input_tokens), 0) FROM request_logs", [], |r| r.get(0))?;
    let total_output_tokens: i64 = conn.query_row("SELECT COALESCE(SUM(output_tokens), 0) FROM request_logs", [], |r| r.get(0))?;
    let today_input_tokens: i64 = conn.query_row("SELECT COALESCE(SUM(input_tokens), 0) FROM request_logs WHERE timestamp LIKE ?1", [&format!("{today}%")], |r| r.get(0))?;
    let today_output_tokens: i64 = conn.query_row("SELECT COALESCE(SUM(output_tokens), 0) FROM request_logs WHERE timestamp LIKE ?1", [&format!("{today}%")], |r| r.get(0))?;

    // Top providers
    let mut stmt = conn.prepare("SELECT provider, COUNT(*) as cnt FROM request_logs WHERE provider IS NOT NULL GROUP BY provider ORDER BY cnt DESC LIMIT 5")?;
    let providers: Vec<ProviderStat> = stmt.query_map([], |r| {
        Ok(ProviderStat { name: r.get(0)?, count: r.get(1)? })
    })?.filter_map(|r| r.ok()).collect();

    Ok(RequestStats {
        total, success, errors,
        success_rate: if total > 0 { (success as f64 / total as f64 * 100.0).round() } else { 0.0 },
        avg_latency_ms: avg_latency.round() as i64,
        today_total, today_errors,
        total_input_tokens, total_output_tokens,
        today_input_tokens, today_output_tokens,
        daily, providers,
    })
}

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RequestStats {
    pub total: i64,
    pub success: i64,
    pub errors: i64,
    pub success_rate: f64,
    pub avg_latency_ms: i64,
    pub today_total: i64,
    pub today_errors: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub today_input_tokens: i64,
    pub today_output_tokens: i64,
    pub daily: Vec<DailyStat>,
    pub providers: Vec<ProviderStat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyStat {
    pub date: String,
    pub total: i64,
    pub errors: i64,
    pub success: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStat {
    pub name: String,
    pub count: i64,
}

/// Delete logs older than `retention_days`. Returns the number of deleted rows.
pub fn cleanup_older_than(conn: &Connection, retention_days: i64) -> Result<usize, AppError> {
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(retention_days)).to_rfc3339();
    let deleted = conn.execute(
        "DELETE FROM request_logs WHERE timestamp < ?1",
        [&cutoff],
    )?;
    Ok(deleted)
}
