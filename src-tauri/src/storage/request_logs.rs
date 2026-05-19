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
    cost: Option<f64>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO request_logs (id, request_id, timestamp, client, provider, model, route,
                status_code, latency_ms, raw_request, converted_request, raw_response,
                converted_response, sse_events, tool_calls, error_message, trace_json,
                input_tokens, output_tokens, cost)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
        rusqlite::params![
            &id, request_id, &now, client, provider, model, route,
            status_code, latency_ms, raw_request, converted_request, raw_response,
            converted_response, sse_events, tool_calls, error_message, trace_json,
            input_tokens, output_tokens, cost,
        ],
    )?;
    Ok(())
}

pub fn clear(conn: &Connection) -> Result<bool, AppError> {
    conn.execute("DELETE FROM request_logs", [])?;
    Ok(true)
}

/// Get request statistics.
/// Consolidated into fewer queries to reduce lock hold time.
pub fn get_stats(conn: &Connection) -> Result<RequestStats, AppError> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let today_prefix = format!("{today}%");

    // Single query for all global + today aggregates
    let (total, success, errors, avg_latency, total_input_tokens, total_output_tokens,
         today_total, today_errors, today_input_tokens, today_output_tokens,
         total_cost, today_cost): (i64, i64, i64, f64, i64, i64, i64, i64, i64, i64, f64, f64) =
        conn.query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status_code >= 400 OR status_code < 200 THEN 1 ELSE 0 END), 0),
                COALESCE(AVG(CASE WHEN status_code >= 200 AND status_code < 300 THEN latency_ms END), 0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN timestamp LIKE ?1 AND (status_code >= 400 OR status_code < 200) THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN input_tokens ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN output_tokens ELSE 0 END), 0),
                COALESCE(SUM(cost), 0.0),
                COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN cost ELSE 0.0 END), 0.0)
            FROM request_logs",
            [&today_prefix],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?)),
        )?;

    // Daily stats (last 7 days) — single query with GROUP BY
    let seven_days_ago = (chrono::Utc::now() - chrono::Duration::days(6)).format("%Y-%m-%d").to_string();
    let mut daily_map: std::collections::HashMap<String, (i64, i64, i64, i64, f64)> = std::collections::HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT substr(timestamp, 1, 10) as day,
                COUNT(*),
                SUM(CASE WHEN status_code >= 400 OR status_code < 200 THEN 1 ELSE 0 END),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cost), 0.0)
         FROM request_logs
         WHERE timestamp >= ?1
         GROUP BY day"
    )?;
    let rows = stmt.query_map([&seven_days_ago], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, f64>(5)?))
    })?;
    for row in rows {
        if let Ok((day, count, errs, inp, outp, cost)) = row {
            daily_map.insert(day, (count, errs, inp, outp, cost));
        }
    }
    let mut daily = Vec::new();
    for i in (0..7).rev() {
        let day = (chrono::Utc::now() - chrono::Duration::days(i)).format("%Y-%m-%d").to_string();
        let (count, errs, inp, outp, cost) = daily_map.get(&day).copied().unwrap_or((0, 0, 0, 0, 0.0));
        daily.push(DailyStat { date: day, total: count, errors: errs, success: count - errs, input_tokens: inp, output_tokens: outp, cost });
    }

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
        total_cost, today_cost,
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
    pub total_cost: f64,
    pub today_cost: f64,
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
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStat {
    pub name: String,
    pub count: i64,
}

/// Get health stats for a specific provider.
pub fn get_provider_health(conn: &Connection, provider_name: &str) -> Result<ProviderHealth, AppError> {
    let now = chrono::Utc::now();
    let one_hour_ago = (now - chrono::Duration::hours(1)).to_rfc3339();
    let one_day_ago = (now - chrono::Duration::hours(24)).to_rfc3339();

    // 1h stats
    let (h1_total, h1_success, h1_avg_latency, h1_p95_latency): (i64, i64, f64, f64) = conn.query_row(
        "SELECT
            COUNT(*),
            COALESCE(SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0),
            COALESCE(AVG(CASE WHEN status_code >= 200 AND status_code < 300 THEN latency_ms END), 0),
            COALESCE((SELECT latency_ms FROM request_logs
                WHERE provider = ?1 AND timestamp >= ?2 AND status_code >= 200 AND status_code < 300
                ORDER BY latency_ms DESC LIMIT 1 OFFSET (
                    SELECT MAX(0, CAST(COUNT(*) * 0.05 AS INTEGER)) FROM request_logs
                    WHERE provider = ?1 AND timestamp >= ?2 AND status_code >= 200 AND status_code < 300
                )), 0)
         FROM request_logs WHERE provider = ?1 AND timestamp >= ?2",
        rusqlite::params![provider_name, &one_hour_ago],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;

    // 24h stats
    let (h24_total, h24_success, h24_avg_latency): (i64, i64, f64) = conn.query_row(
        "SELECT
            COUNT(*),
            COALESCE(SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0),
            COALESCE(AVG(CASE WHEN status_code >= 200 AND status_code < 300 THEN latency_ms END), 0)
         FROM request_logs WHERE provider = ?1 AND timestamp >= ?2",
        rusqlite::params![provider_name, &one_day_ago],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    // Recent errors (last 10)
    let mut stmt = conn.prepare(
        "SELECT timestamp, status_code, error_message FROM request_logs
         WHERE provider = ?1 AND (status_code >= 400 OR status_code < 200) AND error_message IS NOT NULL
         ORDER BY timestamp DESC LIMIT 10"
    )?;
    let recent_errors: Vec<RecentError> = stmt.query_map(rusqlite::params![provider_name], |r| {
        Ok(RecentError {
            timestamp: r.get(0)?,
            status_code: r.get(1)?,
            message: r.get::<_, String>(2).unwrap_or_default(),
        })
    })?.filter_map(|r| r.ok()).collect();

    Ok(ProviderHealth {
        provider: provider_name.to_string(),
        h1_total, h1_success,
        h1_success_rate: if h1_total > 0 { (h1_success as f64 / h1_total as f64 * 100.0).round() } else { 0.0 },
        h1_avg_latency_ms: h1_avg_latency.round() as i64,
        h1_p95_latency_ms: h1_p95_latency.round() as i64,
        h24_total, h24_success,
        h24_success_rate: if h24_total > 0 { (h24_success as f64 / h24_total as f64 * 100.0).round() } else { 0.0 },
        h24_avg_latency_ms: h24_avg_latency.round() as i64,
        recent_errors,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderHealth {
    pub provider: String,
    pub h1_total: i64,
    pub h1_success: i64,
    pub h1_success_rate: f64,
    pub h1_avg_latency_ms: i64,
    pub h1_p95_latency_ms: i64,
    pub h24_total: i64,
    pub h24_success: i64,
    pub h24_success_rate: f64,
    pub h24_avg_latency_ms: i64,
    pub recent_errors: Vec<RecentError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentError {
    pub timestamp: String,
    pub status_code: i64,
    pub message: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_logs_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE request_logs (
                id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                client TEXT,
                provider TEXT,
                model TEXT,
                route TEXT,
                status_code INTEGER,
                latency_ms INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                raw_request TEXT,
                converted_request TEXT,
                raw_response TEXT,
                converted_response TEXT,
                sse_events TEXT,
                tool_calls TEXT,
                error_message TEXT,
                cost REAL,
                trace_json TEXT
            );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn stats_on_empty_logs_are_zero() {
        let conn = empty_logs_db();
        let stats = get_stats(&conn).unwrap();

        assert_eq!(stats.total, 0);
        assert_eq!(stats.success, 0);
        assert_eq!(stats.errors, 0);
        assert_eq!(stats.today_total, 0);
        assert_eq!(stats.today_errors, 0);
        assert_eq!(stats.total_input_tokens, 0);
        assert_eq!(stats.total_output_tokens, 0);
        assert_eq!(stats.total_cost, 0.0);
        assert_eq!(stats.daily.len(), 7);
        assert!(stats.providers.is_empty());
    }

    #[test]
    fn provider_health_on_empty_logs_is_zero() {
        let conn = empty_logs_db();
        let health = get_provider_health(&conn, "DeepSeek").unwrap();

        assert_eq!(health.h1_total, 0);
        assert_eq!(health.h1_success, 0);
        assert_eq!(health.h1_success_rate, 0.0);
        assert_eq!(health.h1_avg_latency_ms, 0);
        assert_eq!(health.h1_p95_latency_ms, 0);
        assert_eq!(health.h24_total, 0);
        assert_eq!(health.h24_success, 0);
        assert_eq!(health.h24_success_rate, 0.0);
        assert_eq!(health.h24_avg_latency_ms, 0);
        assert!(health.recent_errors.is_empty());
    }
}
