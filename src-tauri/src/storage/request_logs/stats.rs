//! 聚合与统计：会话聚合、成本拆分、路由 / 供应商维度统计、仪表盘 stats 与 provider 健康。

use rusqlite::Connection;

use crate::errors::AppError;
use crate::models::request_log::RequestLogFilter;

use super::query::apply_log_filter;

/// 按 session_id 聚合用量：Logs 页「按会话分组」视图用。
/// 同一个 session_id 跨 gateway + client_session 多来源时，source 字段返回 'mixed'。
pub fn aggregate_by_session(
    conn: &Connection,
    filter: &RequestLogFilter,
    limit: i64,
) -> Result<Vec<crate::models::request_log::SessionUsageSummary>, AppError> {
    let limit = limit.clamp(1, 1000);
    // GROUP_CONCAT(DISTINCT source) 让我们事后判断「单源 vs 混合」——SQLite 不支持
    // CASE WHEN COUNT(DISTINCT source) > 1，所以用字符串聚合解决。
    // filter 走和 list 一样的行级 WHERE：只保留匹配的行再 GROUP BY，即「含匹配请求
    // 的会话」——让会话视图跟着客户端/来源/模型等筛选条件走。
    let mut sql = String::from(
        "SELECT
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
        WHERE session_id IS NOT NULL AND session_id != ''",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1;
    apply_log_filter(filter, &mut sql, &mut param_values, &mut idx);
    sql.push_str(&format!(
        " GROUP BY session_id ORDER BY last_seen DESC LIMIT ?{idx}"
    ));
    param_values.push(Box::new(limit));

    let mut stmt = conn.prepare(&sql)?;
    let params_ref: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_ref.as_slice(), |r| {
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

/// 按某维度（model / client）聚合成本与用量——成本仪表盘用。
/// `group_col` 仅接受内部白名单值，杜绝 SQL 注入。
fn aggregate_cost_grouped(
    conn: &Connection,
    group_col: &str,
    since: Option<&str>,
    limit: i64,
) -> Result<Vec<crate::models::request_log::CostBreakdown>, AppError> {
    // 白名单校验：列名直接拼进 SQL，绝不允许外部值。
    debug_assert!(matches!(group_col, "model" | "client"));
    let limit = limit.clamp(1, 1000);
    // since 为 None 时统计全量；为 Some 时只统计该时间点之后（与 Dashboard 的
    // rangeDays 区间口径一致）。
    // timestamp 过滤用条件拼接而非 `(?1 IS NULL OR timestamp >= ?1)`——后者的
    // OR-null 会让 SQLite 放弃 timestamp 索引退化成全表扫(大库冷缓存首屏很慢)。
    // since 有值时拼 `AND timestamp >= ?`,走索引;无值(全量统计)时不拼。
    let (time_clause, limit_idx) = if since.is_some() {
        (" AND timestamp >= ?1", 2)
    } else {
        ("", 1)
    };
    let sql = format!(
        "SELECT {col} AS k,
            MAX(provider) AS provider,
            COUNT(*) AS request_count,
            COALESCE(SUM(input_tokens), 0) AS input_tokens,
            COALESCE(SUM(output_tokens), 0) AS output_tokens,
            COALESCE(SUM(cache_read_tokens), 0) AS cache_read_tokens,
            COALESCE(SUM(cache_write_tokens), 0) AS cache_write_tokens,
            COALESCE(SUM(cost), 0.0) AS cost
        FROM request_logs
        WHERE {col} IS NOT NULL AND {col} != ''{time_clause}
          -- 过滤无 token 用量的噪音条目（失败请求 / synthetic 错误兜底 / 上游未返回
          -- usage 的直通请求），它们对成本统计零贡献。
          AND (COALESCE(input_tokens, 0) + COALESCE(output_tokens, 0)) > 0
        GROUP BY {col}
        ORDER BY cost DESC, request_count DESC
        LIMIT ?{limit_idx}",
        col = group_col
    );

    let mut stmt = conn.prepare(&sql)?;
    // 顺序与 SQL 占位符一致:since 有值时 ?1=timestamp、?2=limit;无值时 ?1=limit。
    let mut params: Vec<&dyn rusqlite::types::ToSql> = Vec::new();
    if let Some(ref s) = since {
        params.push(s);
    }
    params.push(&limit);
    let rows = stmt.query_map(params.as_slice(), |r| {
        Ok(crate::models::request_log::CostBreakdown {
            key: r.get(0)?,
            provider: r.get(1)?,
            request_count: r.get(2)?,
            input_tokens: r.get(3)?,
            output_tokens: r.get(4)?,
            cache_read_tokens: r.get(5)?,
            cache_write_tokens: r.get(6)?,
            cost: r.get(7)?,
            has_price: true, // 占位，按模型聚合时下面用 pricing 表覆盖
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        let mut item = r?;
        // 仅按模型聚合时判断该模型有没有价：用于 UI 区分"真免费"和"缺价算不出"。
        if group_col == "model" {
            item.has_price = crate::storage::pricing::get_price(
                conn,
                item.provider.as_deref().unwrap_or(""),
                &item.key,
            )
            .is_some();
        }
        out.push(item);
    }
    Ok(out)
}

/// 按模型聚合成本，按成本倒序。成本仪表盘「钱花在哪个模型」用。
/// since=None 统计全量；Some 时只统计该时间之后。
pub fn aggregate_cost_by_model(
    conn: &Connection,
    since: Option<&str>,
    limit: i64,
) -> Result<Vec<crate::models::request_log::CostBreakdown>, AppError> {
    aggregate_cost_grouped(conn, "model", since, limit)
}

/// 按客户端聚合成本，按成本倒序。成本仪表盘「哪个客户端花得多」用。
pub fn aggregate_cost_by_client(
    conn: &Connection,
    since: Option<&str>,
    limit: i64,
) -> Result<Vec<crate::models::request_log::CostBreakdown>, AppError> {
    aggregate_cost_grouped(conn, "client", since, limit)
}

pub fn aggregate_route_profile_stats(
    conn: &Connection,
    since: Option<&str>,
) -> Result<Vec<crate::models::route_profile::RouteProfileStats>, AppError> {
    let mut sql = String::from(
        "SELECT
            COALESCE(
                json_extract(request_logs.trace_json, '$.route_decision.profile_id'),
                legacy_profile.id
            ) AS profile_id,
            COUNT(*) AS request_count,
            SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END) AS success_count,
            SUM(CASE WHEN status_code < 200 OR status_code >= 400 THEN 1 ELSE 0 END) AS error_count,
            COALESCE(AVG(latency_ms), 0) AS avg_latency_ms,
            COALESCE(SUM(cost), 0.0) AS cost
         FROM request_logs
         LEFT JOIN route_profiles legacy_profile
           ON json_extract(request_logs.trace_json, '$.route_decision.profile_id') IS NULL
          AND legacy_profile.enabled = 1
          AND legacy_profile.is_default = 1
          AND legacy_profile.input_protocol = CASE request_logs.route
              WHEN '/v1/responses' THEN 'openai_responses'
              WHEN '/v1/chat/completions' THEN 'openai_chat_completions'
              WHEN '/v1/messages' THEN 'anthropic_messages'
              ELSE NULL
          END
         WHERE source = 'gateway'
           AND (
             json_extract(request_logs.trace_json, '$.route_decision.profile_id') IS NOT NULL
             OR legacy_profile.id IS NOT NULL
           )",
    );
    if since.is_some() {
        sql.push_str(" AND timestamp >= ?1");
    }
    sql.push_str(" GROUP BY profile_id");

    let mut stmt = conn.prepare(&sql)?;
    let map_row = |r: &rusqlite::Row<'_>| {
        let request_count: i64 = r.get(1)?;
        let success_count: i64 = r.get(2)?;
        let error_count: i64 = r.get(3)?;
        Ok(crate::models::route_profile::RouteProfileStats {
            route_profile_id: r.get(0)?,
            request_count,
            success_count,
            error_count,
            success_rate: if request_count > 0 {
                success_count as f64 / request_count as f64
            } else {
                0.0
            },
            avg_latency_ms: r.get::<_, f64>(4)?.round() as i64,
            cost: r.get(5)?,
        })
    };

    let rows = if let Some(since) = since {
        stmt.query_map([since], map_row)?
    } else {
        stmt.query_map([], map_row)?
    };
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn aggregate_provider_detail_stats(
    conn: &Connection,
    provider_name: &str,
    since: Option<&str>,
    limit: i64,
) -> Result<crate::models::request_log::ProviderDetailStats, AppError> {
    let limit = limit.clamp(1, 200);

    let mut model_sql = String::from(
        "SELECT
            model,
            COUNT(*) AS request_count,
            COALESCE(SUM(CASE WHEN status_code >= 200 AND status_code < 300 THEN 1 ELSE 0 END), 0) AS success_count,
            COALESCE(SUM(CASE WHEN status_code < 200 OR status_code >= 400 THEN 1 ELSE 0 END), 0) AS error_count,
            COALESCE(AVG(latency_ms), 0) AS avg_latency_ms,
            COALESCE(SUM(cost), 0.0) AS cost
         FROM request_logs
         WHERE source = 'gateway'
           AND provider = ?1
           AND model IS NOT NULL
           AND model != ''",
    );
    if since.is_some() {
        model_sql.push_str(" AND timestamp >= ?2");
    }
    model_sql.push_str(" GROUP BY model ORDER BY cost DESC, request_count DESC");

    let mut model_stmt = conn.prepare(&model_sql)?;
    let map_model = |r: &rusqlite::Row<'_>| {
        let request_count: i64 = r.get(1)?;
        let success_count: i64 = r.get(2)?;
        Ok(crate::models::request_log::ProviderModelStats {
            model: r.get(0)?,
            request_count,
            success_count,
            error_count: r.get(3)?,
            success_rate: if request_count > 0 {
                success_count as f64 / request_count as f64
            } else {
                0.0
            },
            avg_latency_ms: r.get::<_, f64>(4)?.round() as i64,
            cost: r.get(5)?,
        })
    };
    let model_rows = if let Some(since) = since {
        model_stmt.query_map(rusqlite::params![provider_name, since], map_model)?
    } else {
        model_stmt.query_map(rusqlite::params![provider_name], map_model)?
    };
    let mut model_stats = Vec::new();
    for row in model_rows {
        model_stats.push(row?);
    }

    let mut latency_sql = String::from(
        "SELECT timestamp, model, latency_ms, status_code
         FROM request_logs
         WHERE source = 'gateway'
           AND provider = ?1
           AND latency_ms IS NOT NULL",
    );
    if since.is_some() {
        latency_sql.push_str(" AND timestamp >= ?2");
    }
    latency_sql.push_str(" ORDER BY timestamp DESC LIMIT ?");
    let limit_index = if since.is_some() { "3" } else { "2" };
    latency_sql.push_str(limit_index);

    let mut latency_stmt = conn.prepare(&latency_sql)?;
    let map_latency = |r: &rusqlite::Row<'_>| {
        Ok(crate::models::request_log::ProviderLatencyPoint {
            timestamp: r.get(0)?,
            model: r.get(1)?,
            latency_ms: r.get(2)?,
            status_code: r.get(3)?,
        })
    };
    let latency_rows = if let Some(since) = since {
        latency_stmt.query_map(rusqlite::params![provider_name, since, limit], map_latency)?
    } else {
        latency_stmt.query_map(rusqlite::params![provider_name, limit], map_latency)?
    };
    let mut latency_points = Vec::new();
    for row in latency_rows {
        latency_points.push(row?);
    }
    latency_points.reverse();

    Ok(crate::models::request_log::ProviderDetailStats {
        provider: provider_name.to_string(),
        latency_points,
        model_stats,
    })
}

/// 各 provider 近 N 小时成功请求(2xx)的平均延迟(ms)——延迟感知路由用。
/// key 为 provider 名（与日志写入一致）。只算 **网关来源** 的成功请求：客户端会话
/// 同步导入的 latency 是客户端自记、不反映网关到上游的真实延迟，不能用于路由决策
/// （与 get_provider_health 的 source='gateway' 口径一致）。
pub fn avg_latency_by_provider(
    conn: &Connection,
    lookback_hours: i64,
) -> Result<std::collections::HashMap<String, f64>, AppError> {
    let since = (chrono::Utc::now() - chrono::Duration::hours(lookback_hours.max(1))).to_rfc3339();
    let mut stmt = conn.prepare(
        "SELECT provider, AVG(latency_ms)
         FROM request_logs
         WHERE provider IS NOT NULL AND provider != ''
           AND source = 'gateway'
           AND latency_ms IS NOT NULL
           AND status_code >= 200 AND status_code < 300
           AND timestamp >= ?1
         GROUP BY provider",
    )?;
    let rows = stmt.query_map([&since], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
    })?;
    let mut out = std::collections::HashMap::new();
    for r in rows {
        let (k, v) = r?;
        out.insert(k, v);
    }
    Ok(out)
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
    for (day, count, errs, inp, outp, cost, cw, cr) in rows.flatten() {
        daily_map.insert(day, (count, errs, inp, outp, cost, cw, cr));
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

    // 今日 codex_compact 计数 — 单独查 trace_json,跟主聚合解耦。
    let today_codex_compact: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM request_logs
             WHERE source = 'gateway' AND timestamp LIKE ?1
               AND trace_json IS NOT NULL
               AND trace_json LIKE '%\"mode\":\"codex_compact\"%'",
            [&today_prefix],
            |r| r.get(0),
        )
        .unwrap_or(0);

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
        today_codex_compact,
        daily,
        providers,
    })
}

use serde::Serialize;

#[derive(Debug, Clone, Serialize, specta::Type)]
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
    /// 今日触发本地 Codex remote compaction 的次数(trace.mode = "codex_compact")。
    pub today_codex_compact: i64,
    pub daily: Vec<DailyStat>,
    pub providers: Vec<ProviderStat>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, specta::Type)]
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

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RecentError {
    pub timestamp: String,
    pub status_code: i64,
    pub message: String,
}
