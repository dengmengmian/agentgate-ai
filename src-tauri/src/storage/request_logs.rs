use rusqlite::Connection;

use crate::errors::AppError;
use crate::models::request_log::{RequestLogDetail, RequestLogFilter, RequestLogListItem};

/// Count rows matching the filter — used by the Logs page to compute the
/// total page count without fetching all rows. Shares the same WHERE-clause
/// construction as `list()` to guarantee identical filtering semantics.
pub fn count(conn: &Connection, filter: &RequestLogFilter) -> Result<i64, AppError> {
    let mut sql = String::from("SELECT COUNT(*) FROM request_logs WHERE 1=1");
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    apply_log_filter(filter, &mut sql, &mut param_values, &mut idx);

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|b| &**b as &dyn rusqlite::types::ToSql)
        .collect();
    let total: i64 = conn
        .query_row(&sql, rusqlite::params_from_iter(params.iter()), |r| {
            r.get(0)
        })
        .map_err(|e| AppError::from(e))?;
    Ok(total)
}

pub fn list(
    conn: &Connection,
    filter: RequestLogFilter,
) -> Result<Vec<RequestLogListItem>, AppError> {
    let mut sql = String::from(
        "SELECT id, request_id, timestamp, client, provider, model, route, status_code, latency_ms, error_message, source, session_id
         FROM request_logs WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;

    apply_log_filter(&filter, &mut sql, &mut param_values, &mut idx);

    sql.push_str(" ORDER BY timestamp DESC");

    let limit = filter.limit.unwrap_or(100);
    let offset = filter.offset.unwrap_or(0);
    sql.push_str(&format!(" LIMIT ?{idx} OFFSET ?{}", idx + 1));
    param_values.push(Box::new(limit));
    param_values.push(Box::new(offset));

    let mut stmt = conn.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();

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
            source: row.get(10)?,
            session_id: row.get(11)?,
        })
    })?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

/// 把 RequestLogFilter 的过滤条件转 WHERE 子句。count / list / aggregate_by_session
/// 共享，保证过滤语义一致。
fn apply_log_filter(
    filter: &RequestLogFilter,
    sql: &mut String,
    param_values: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    idx: &mut usize,
) {
    if let Some(ref client) = filter.client {
        sql.push_str(&format!(" AND client = ?{idx}"));
        param_values.push(Box::new(client.clone()));
        *idx += 1;
    }
    if let Some(ref provider) = filter.provider {
        sql.push_str(&format!(" AND provider = ?{idx}"));
        param_values.push(Box::new(provider.clone()));
        *idx += 1;
    }
    if let Some(ref source) = filter.source {
        // 'session_log' = 所有非 gateway 来源的合集（聚合视图用）。
        if source == "session_log" {
            sql.push_str(" AND source IS NOT NULL AND source != 'gateway'");
        } else {
            sql.push_str(&format!(" AND source = ?{idx}"));
            param_values.push(Box::new(source.clone()));
            *idx += 1;
        }
    }
    if let Some(ref sid) = filter.session_id {
        sql.push_str(&format!(" AND session_id = ?{idx}"));
        param_values.push(Box::new(sid.clone()));
        *idx += 1;
    }
    if let Some(ref status) = filter.status {
        match status.as_str() {
            "success" => {
                sql.push_str(&format!(
                    " AND status_code >= ?{} AND status_code < ?{}",
                    idx,
                    *idx + 1
                ));
                param_values.push(Box::new(200i64));
                param_values.push(Box::new(300i64));
                *idx += 2;
            }
            "error" => {
                sql.push_str(&format!(
                    " AND (status_code >= ?{} OR status_code < ?{})",
                    idx,
                    *idx + 1
                ));
                param_values.push(Box::new(400i64));
                param_values.push(Box::new(200i64));
                *idx += 2;
            }
            _ => {}
        }
    }
    if let Some(ref keyword) = filter.keyword {
        let like = format!("%{keyword}%");
        sql.push_str(&format!(
            " AND (request_id LIKE ?{idx} OR error_message LIKE ?{} OR model LIKE ?{} OR route LIKE ?{})",
            *idx + 1, *idx + 2, *idx + 3
        ));
        param_values.push(Box::new(like.clone()));
        param_values.push(Box::new(like.clone()));
        param_values.push(Box::new(like.clone()));
        param_values.push(Box::new(like));
        *idx += 4;
    }
}

pub fn get_detail(conn: &Connection, id: &str) -> Result<RequestLogDetail, AppError> {
    conn.query_row(
        "SELECT id, request_id, timestamp, client, provider, model, route, status_code,
                latency_ms, input_tokens, output_tokens, raw_request, converted_request,
                raw_response, converted_response, sse_events, tool_calls, error_message, trace_json,
                source, session_id, external_id
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
                source: row.get(19)?,
                session_id: row.get(20)?,
                external_id: row.get(21)?,
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
    cache_write_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    source: Option<&str>,
    session_id: Option<&str>,
    external_id: Option<&str>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    // 缺省视为 'gateway' —— 旧调用方迁移期间还没传，保持以前的语义。
    let source = source.unwrap_or("gateway");

    conn.execute(
        "INSERT INTO request_logs (id, request_id, timestamp, client, provider, model, route,
                status_code, latency_ms, raw_request, converted_request, raw_response,
                converted_response, sse_events, tool_calls, error_message, trace_json,
                input_tokens, output_tokens, cost, cache_write_tokens, cache_read_tokens,
                source, session_id, external_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
        rusqlite::params![
            &id, request_id, &now, client, provider, model, route,
            status_code, latency_ms, raw_request, converted_request, raw_response,
            converted_response, sse_events, tool_calls, error_message, trace_json,
            input_tokens, output_tokens, cost, cache_write_tokens, cache_read_tokens,
            source, session_id, external_id,
        ],
    )?;
    Ok(())
}

/// 给客户端会话日志同步器用：插入一条来自客户端本地日志的请求记录。
/// 与 `insert` 的差别：
///   - timestamp 来自调用方（文件里的事件时间），不是 now()
///   - 没有 raw_request / converted_request / response / SSE / tool_calls / error_message
///     —— 客户端日志只能给到 usage，不可能反推完整请求
///   - source / session_id / external_id 必填
///
/// 调用方应当先用 `external_ids_for_source` 过滤去重，再批量调这个函数。
#[allow(clippy::too_many_arguments)]
pub fn insert_session_log(
    conn: &Connection,
    timestamp: &str,
    client: &str,
    provider: &str,
    model: &str,
    route: &str,
    source: &str,
    session_id: &str,
    external_id: &str,
    input_tokens: Option<i64>,
    output_tokens: Option<i64>,
    cache_write_tokens: Option<i64>,
    cache_read_tokens: Option<i64>,
    cost: Option<f64>,
) -> Result<(), AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    // request_id 我们没有真实值——客户端日志的 message_id 作为 request_id，方便用户在
    // Logs 详情里通过 request_id 列追溯到原文件里的那条消息。external_id 同时填同样的值，
    // 用作幂等 key 防止重复同步。
    conn.execute(
        "INSERT INTO request_logs (id, request_id, timestamp, client, provider, model, route,
                status_code, latency_ms,
                input_tokens, output_tokens, cost, cache_write_tokens, cache_read_tokens,
                source, session_id, external_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 200, 0, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            &id,
            external_id,
            timestamp,
            client,
            provider,
            model,
            route,
            input_tokens,
            output_tokens,
            cost,
            cache_write_tokens,
            cache_read_tokens,
            source,
            session_id,
            external_id,
        ],
    )?;
    Ok(())
}

/// 给客户端日志同步器用：从 DB 里查询某个 source 下已存在的 external_id 集合。
/// 同步前先调一次，把扫到的条目和这个集合做差集，避免重复插入。
pub fn external_ids_for_source(
    conn: &Connection,
    source: &str,
    candidates: &[String],
) -> Result<std::collections::HashSet<String>, AppError> {
    if candidates.is_empty() {
        return Ok(std::collections::HashSet::new());
    }
    // SQLite 单语句最多约 32k 个 placeholder；这里取 800 为一批，留足余量。
    let mut found = std::collections::HashSet::new();
    for chunk in candidates.chunks(800) {
        let placeholders = (1..=chunk.len())
            .map(|i| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT external_id FROM request_logs
             WHERE source = ?1 AND external_id IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut params: Vec<&dyn rusqlite::types::ToSql> =
            vec![&source as &dyn rusqlite::types::ToSql];
        for c in chunk {
            params.push(c as &dyn rusqlite::types::ToSql);
        }
        let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
            r.get::<_, String>(0)
        })?;
        for r in rows {
            if let Ok(id) = r {
                found.insert(id);
            }
        }
    }
    Ok(found)
}

/// 按 session_id 聚合用量：Logs 页「按会话分组」视图用。
/// 同一个 session_id 跨 gateway + client_session 多来源时，source 字段返回 'mixed'。
pub fn aggregate_by_session(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<crate::models::request_log::SessionUsageSummary>, AppError> {
    let limit = limit.clamp(1, 1000);
    // GROUP_CONCAT(DISTINCT source) 让我们事后判断「单源 vs 混合」——SQLite 不支持
    // CASE WHEN COUNT(DISTINCT source) > 1，所以用字符串聚合解决。
    let sql = "SELECT
        session_id,
        GROUP_CONCAT(DISTINCT source) AS sources,
        MAX(provider) AS provider,
        MAX(model) AS model,
        MIN(timestamp) AS first_seen,
        MAX(timestamp) AS last_seen,
        COUNT(*) AS request_count,
        COALESCE(SUM(input_tokens), 0) AS input_tokens,
        COALESCE(SUM(output_tokens), 0) AS output_tokens,
        COALESCE(SUM(cache_read_tokens), 0) AS cache_read_tokens,
        COALESCE(SUM(cache_write_tokens), 0) AS cache_write_tokens,
        COALESCE(SUM(cost), 0.0) AS cost
        FROM request_logs
        WHERE session_id IS NOT NULL AND session_id != ''
        GROUP BY session_id
        ORDER BY last_seen DESC
        LIMIT ?1";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([limit], |r| {
        let sources: String = r.get::<_, Option<String>>(1)?.unwrap_or_default();
        let single_source = !sources.contains(',');
        Ok(crate::models::request_log::SessionUsageSummary {
            session_id: r.get(0)?,
            source: if single_source {
                sources
            } else {
                "mixed".to_string()
            },
            provider: r.get(2)?,
            model: r.get(3)?,
            first_seen: r.get(4)?,
            last_seen: r.get(5)?,
            request_count: r.get(6)?,
            input_tokens: r.get(7)?,
            output_tokens: r.get(8)?,
            cache_read_tokens: r.get(9)?,
            cache_write_tokens: r.get(10)?,
            cost: r.get(11)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Pull `(cache_write_tokens, cache_read_tokens)` out of any supported upstream
/// usage shape. Returns `(None, None)` when neither is present so the row
/// keeps "unknown" semantics rather than misleading zeroes.
///
/// Recognised shapes:
///   - Anthropic Messages: `cache_creation_input_tokens` / `cache_read_input_tokens`
///   - OpenAI Responses: `input_tokens_details.cached_tokens` (Read only)
///   - OpenAI Chat Completions: `prompt_tokens_details.cached_tokens` (Read only)
///   - Bare field used by some Chinese providers: `cached_tokens` (Read only)
pub fn extract_cache_tokens(usage: &serde_json::Value) -> (Option<i64>, Option<i64>) {
    let write = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_i64());
    let read = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(|v| v.as_i64())
        })
        .or_else(|| {
            usage
                .pointer("/prompt_tokens_details/cached_tokens")
                .and_then(|v| v.as_i64())
        })
        .or_else(|| usage.get("cached_tokens").and_then(|v| v.as_i64()));
    (write, read)
}

pub fn clear(conn: &Connection) -> Result<bool, AppError> {
    conn.execute("DELETE FROM request_logs", [])?;
    Ok(true)
}

/// Get request statistics.
/// Consolidated into fewer queries to reduce lock hold time.
pub fn get_stats(conn: &Connection) -> Result<RequestStats, AppError> {
    get_stats_for_range(conn, 7)
}

/// Compute stats over a sliding window of `daily_window` past days. The
/// always-on totals (lifetime) plus today aggregates are independent of the
/// window; only the `daily` Vec changes shape (length == daily_window, today
/// last). Today's `cached_tokens` etc. are still pulled from the lifetime
/// query path so dashboard cards stay correct regardless of selected range.
pub fn get_stats_for_range(conn: &Connection, daily_window: i64) -> Result<RequestStats, AppError> {
    let daily_window = daily_window.clamp(1, 365);
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let today_prefix = format!("{today}%");

    // Request quality metrics are gateway-only. Client-local session imports
    // are usage records, not live proxy requests; they carry synthetic
    // status_code=200 and latency_ms=0, so including them would distort
    // success rate and latency. Token/cost/cache aggregates still include all
    // sources so session sync can fill the user's usage picture.
    let (
        total,
        success,
        errors,
        avg_latency,
        total_input_tokens,
        total_output_tokens,
        today_total,
        today_errors,
        today_input_tokens,
        today_output_tokens,
        total_cost,
        today_cost,
        total_cache_write,
        total_cache_read,
        today_cache_write,
        today_cache_read,
    ): (i64, i64, i64, f64, i64, i64, i64, i64, i64, i64, f64, f64, i64, i64, i64, i64) = conn.query_row(
        "SELECT
            COALESCE(SUM(CASE WHEN source = 'gateway' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN source = 'gateway' AND status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN source = 'gateway' AND (status_code >= 400 OR status_code < 200) THEN 1 ELSE 0 END), 0),
            COALESCE(AVG(CASE WHEN source = 'gateway' AND status_code >= 200 AND status_code < 300 THEN latency_ms END), 0),
            COALESCE(SUM(input_tokens), 0),
            COALESCE(SUM(output_tokens), 0),
            COALESCE(SUM(CASE WHEN source = 'gateway' AND timestamp LIKE ?1 THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN source = 'gateway' AND timestamp LIKE ?1 AND (status_code >= 400 OR status_code < 200) THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN input_tokens ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN output_tokens ELSE 0 END), 0),
            COALESCE(SUM(cost), 0.0),
            COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN cost ELSE 0.0 END), 0.0),
            COALESCE(SUM(cache_write_tokens), 0),
            COALESCE(SUM(cache_read_tokens), 0),
            COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN cache_write_tokens ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN timestamp LIKE ?1 THEN cache_read_tokens ELSE 0 END), 0)
        FROM request_logs",
        [&today_prefix],
        |r| {
            Ok((
                r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?,
                r.get(12)?, r.get(13)?, r.get(14)?, r.get(15)?,
            ))
        },
    )?;

    // Daily aggregation over the requested window — single GROUP BY query.
    let window_start = (chrono::Utc::now() - chrono::Duration::days(daily_window - 1))
        .format("%Y-%m-%d")
        .to_string();
    let mut daily_map: std::collections::HashMap<String, (i64, i64, i64, i64, f64, i64, i64)> =
        std::collections::HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT substr(timestamp, 1, 10) as day,
                COALESCE(SUM(CASE WHEN source = 'gateway' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN source = 'gateway' AND (status_code >= 400 OR status_code < 200) THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cost), 0.0),
                COALESCE(SUM(cache_write_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0)
         FROM request_logs
         WHERE timestamp >= ?1
         GROUP BY day",
    )?;
    let rows = stmt.query_map([&window_start], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, f64>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
        ))
    })?;
    for row in rows {
        if let Ok((day, count, errs, inp, outp, cost, cw, cr)) = row {
            daily_map.insert(day, (count, errs, inp, outp, cost, cw, cr));
        }
    }
    let mut daily = Vec::new();
    for i in (0..daily_window).rev() {
        let day = (chrono::Utc::now() - chrono::Duration::days(i))
            .format("%Y-%m-%d")
            .to_string();
        let (count, errs, inp, outp, cost, cw, cr) = daily_map
            .get(&day)
            .copied()
            .unwrap_or((0, 0, 0, 0, 0.0, 0, 0));
        daily.push(DailyStat {
            date: day,
            total: count,
            errors: errs,
            success: count - errs,
            input_tokens: inp,
            output_tokens: outp,
            cost,
            cache_write_tokens: cw,
            cache_read_tokens: cr,
        });
    }

    // Top providers describes live gateway routing, not local session imports.
    let mut stmt = conn.prepare(
        "SELECT provider, COUNT(*) as cnt FROM request_logs WHERE source = 'gateway' AND provider IS NOT NULL GROUP BY provider ORDER BY cnt DESC LIMIT 5",
    )?;
    let providers: Vec<ProviderStat> = stmt
        .query_map([], |r| {
            Ok(ProviderStat {
                name: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(RequestStats {
        total,
        success,
        errors,
        success_rate: if total > 0 {
            (success as f64 / total as f64 * 100.0).round()
        } else {
            0.0
        },
        avg_latency_ms: avg_latency.round() as i64,
        today_total,
        today_errors,
        total_input_tokens,
        total_output_tokens,
        today_input_tokens,
        today_output_tokens,
        total_cost,
        today_cost,
        total_cache_write_tokens: total_cache_write,
        total_cache_read_tokens: total_cache_read,
        today_cache_write_tokens: today_cache_write,
        today_cache_read_tokens: today_cache_read,
        daily,
        providers,
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
    pub total_cache_write_tokens: i64,
    pub total_cache_read_tokens: i64,
    pub today_cache_write_tokens: i64,
    pub today_cache_read_tokens: i64,
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
    pub cache_write_tokens: i64,
    pub cache_read_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderStat {
    pub name: String,
    pub count: i64,
}

/// Get health stats for a specific provider.
pub fn get_provider_health(
    conn: &Connection,
    provider_name: &str,
) -> Result<ProviderHealth, AppError> {
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
                WHERE source = 'gateway' AND provider = ?1 AND timestamp >= ?2 AND status_code >= 200 AND status_code < 300
                ORDER BY latency_ms DESC LIMIT 1 OFFSET (
                    SELECT MAX(0, CAST(COUNT(*) * 0.05 AS INTEGER)) FROM request_logs
                    WHERE source = 'gateway' AND provider = ?1 AND timestamp >= ?2 AND status_code >= 200 AND status_code < 300
                )), 0)
         FROM request_logs WHERE source = 'gateway' AND provider = ?1 AND timestamp >= ?2",
        rusqlite::params![provider_name, &one_hour_ago],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;

    // 24h stats
    let (h24_total, h24_success, h24_avg_latency): (i64, i64, f64) = conn.query_row(
        "SELECT
            COUNT(*),
            COALESCE(SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0),
            COALESCE(AVG(CASE WHEN status_code >= 200 AND status_code < 300 THEN latency_ms END), 0)
         FROM request_logs WHERE source = 'gateway' AND provider = ?1 AND timestamp >= ?2",
        rusqlite::params![provider_name, &one_day_ago],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    // Recent errors (last 10)
    let mut stmt = conn.prepare(
        "SELECT timestamp, status_code, error_message FROM request_logs
         WHERE source = 'gateway' AND provider = ?1 AND (status_code >= 400 OR status_code < 200) AND error_message IS NOT NULL
         ORDER BY timestamp DESC LIMIT 10"
    )?;
    let recent_errors: Vec<RecentError> = stmt
        .query_map(rusqlite::params![provider_name], |r| {
            Ok(RecentError {
                timestamp: r.get(0)?,
                status_code: r.get(1)?,
                message: r.get::<_, String>(2).unwrap_or_default(),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(ProviderHealth {
        provider: provider_name.to_string(),
        h1_total,
        h1_success,
        h1_success_rate: if h1_total > 0 {
            (h1_success as f64 / h1_total as f64 * 100.0).round()
        } else {
            0.0
        },
        h1_avg_latency_ms: h1_avg_latency.round() as i64,
        h1_p95_latency_ms: h1_p95_latency.round() as i64,
        h24_total,
        h24_success,
        h24_success_rate: if h24_total > 0 {
            (h24_success as f64 / h24_total as f64 * 100.0).round()
        } else {
            0.0
        },
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
    let deleted = conn.execute("DELETE FROM request_logs WHERE timestamp < ?1", [&cutoff])?;
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
                trace_json TEXT,
                cache_write_tokens INTEGER,
                cache_read_tokens INTEGER,
                source TEXT,
                session_id TEXT,
                external_id TEXT
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
        assert_eq!(stats.total_cache_write_tokens, 0);
        assert_eq!(stats.total_cache_read_tokens, 0);
        assert_eq!(stats.today_cache_write_tokens, 0);
        assert_eq!(stats.today_cache_read_tokens, 0);
        assert_eq!(stats.daily.len(), 7);
        assert!(stats.providers.is_empty());
    }

    #[test]
    fn stats_for_range_returns_matching_window_size() {
        let conn = empty_logs_db();
        assert_eq!(get_stats_for_range(&conn, 1).unwrap().daily.len(), 1);
        assert_eq!(get_stats_for_range(&conn, 14).unwrap().daily.len(), 14);
        assert_eq!(get_stats_for_range(&conn, 30).unwrap().daily.len(), 30);
    }

    #[test]
    fn stats_for_range_clamps_negative_and_huge_values() {
        let conn = empty_logs_db();
        // Below 1 clamps to 1, above 365 clamps to 365.
        assert_eq!(get_stats_for_range(&conn, 0).unwrap().daily.len(), 1);
        assert_eq!(get_stats_for_range(&conn, -5).unwrap().daily.len(), 1);
        assert_eq!(get_stats_for_range(&conn, 999).unwrap().daily.len(), 365);
    }

    #[test]
    fn stats_keep_session_imports_out_of_gateway_quality_metrics() {
        let conn = empty_logs_db();
        insert(
            &conn,
            "req_gateway",
            "Codex",
            "LiveProvider",
            "gpt-live",
            "/v1/responses",
            200,
            123,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(10),
            Some(5),
            Some(0.01),
            None,
            None,
            Some("gateway"),
            Some("session_live"),
            Some("req_gateway"),
        )
        .unwrap();
        insert_session_log(
            &conn,
            &chrono::Utc::now().to_rfc3339(),
            "Codex",
            "openai_official",
            "gpt-history",
            "/v1/responses",
            "codex_session",
            "session_history",
            "session_history:1",
            Some(1000),
            Some(100),
            None,
            Some(50),
            Some(0.25),
        )
        .unwrap();

        let stats = get_stats_for_range(&conn, 1).unwrap();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.success, 1);
        assert_eq!(stats.errors, 0);
        assert_eq!(stats.avg_latency_ms, 123);
        assert_eq!(stats.today_total, 1);
        assert_eq!(stats.total_input_tokens, 1010);
        assert_eq!(stats.total_output_tokens, 105);
        assert_eq!(stats.total_cache_read_tokens, 50);
        assert!((stats.total_cost - 0.26).abs() < f64::EPSILON);
        assert_eq!(stats.providers.len(), 1);
        assert_eq!(stats.providers[0].name, "LiveProvider");
        assert_eq!(stats.daily[0].total, 1);
        assert_eq!(stats.daily[0].input_tokens, 1010);
    }

    #[test]
    fn extract_cache_tokens_anthropic_format() {
        let usage = serde_json::json!({
            "input_tokens": 100,
            "cache_creation_input_tokens": 80,
            "cache_read_input_tokens": 20,
        });
        let (w, r) = extract_cache_tokens(&usage);
        assert_eq!(w, Some(80));
        assert_eq!(r, Some(20));
    }

    #[test]
    fn extract_cache_tokens_openai_responses_format() {
        let usage = serde_json::json!({
            "input_tokens": 100,
            "input_tokens_details": {"cached_tokens": 60},
            "output_tokens": 30,
        });
        let (w, r) = extract_cache_tokens(&usage);
        assert_eq!(w, None, "OpenAI Responses doesn't surface cache writes");
        assert_eq!(r, Some(60));
    }

    #[test]
    fn extract_cache_tokens_openai_chat_completions_format() {
        let usage = serde_json::json!({
            "prompt_tokens": 100,
            "prompt_tokens_details": {"cached_tokens": 45},
            "completion_tokens": 20,
        });
        let (w, r) = extract_cache_tokens(&usage);
        assert_eq!(w, None);
        assert_eq!(r, Some(45));
    }

    #[test]
    fn extract_cache_tokens_bare_field() {
        let usage = serde_json::json!({"cached_tokens": 7});
        let (w, r) = extract_cache_tokens(&usage);
        assert_eq!(w, None);
        assert_eq!(r, Some(7));
    }

    #[test]
    fn extract_cache_tokens_empty_usage_returns_none() {
        let usage = serde_json::json!({});
        let (w, r) = extract_cache_tokens(&usage);
        assert_eq!(w, None);
        assert_eq!(r, None);
    }

    #[test]
    fn extract_cache_tokens_prefers_anthropic_over_openai_when_both_present() {
        // Pathological: provider that emits both keys. Anthropic Write field
        // is unambiguous; Read fields are equivalent so we don't care which
        // wins for Read as long as both are non-null.
        let usage = serde_json::json!({
            "cache_creation_input_tokens": 50,
            "cache_read_input_tokens": 25,
            "input_tokens_details": {"cached_tokens": 99},
        });
        let (w, r) = extract_cache_tokens(&usage);
        assert_eq!(w, Some(50));
        assert_eq!(
            r,
            Some(25),
            "anthropic cache_read takes priority over openai cached_tokens"
        );
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
