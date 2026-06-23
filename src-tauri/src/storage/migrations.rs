use rusqlite::Connection;

use crate::errors::AppError;

/// 当前 schema 版本。每加一段新迁移就 +1,放到 `run_versioned_migrations`
/// 里 match 上对应的 version。读 `PRAGMA user_version` 决定该跑哪些。
const CURRENT_SCHEMA_VERSION: u32 = 6;

fn get_user_version(conn: &Connection) -> Result<u32, AppError> {
    let v: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    Ok(v)
}

fn set_user_version(conn: &Connection, v: u32) -> Result<(), AppError> {
    // PRAGMA 不支持 ?  参数,只能拼字符串。v 是内部常量,无注入风险。
    conn.execute_batch(&format!("PRAGMA user_version = {v};"))?;
    Ok(())
}

/// 主入口:按 `PRAGMA user_version` 跳过已应用的迁移。
///
/// - v0(默认):说明要么是新装,要么是 1.3.6 及更早版本——schema 由
///   `legacy_baseline_v1` 用 `IF NOT EXISTS` / `is_ok` 探嗅式迁移建出来。跑一次,
///   再 `set_user_version(1)`。对老用户:跑一次幂等无副作用;对新用户:从零建好。
/// - v1+:跳过 baseline,只跑 `run_versioned_migrations` 里 version > current 的步骤。
///
/// 以后再加迁移:CURRENT_SCHEMA_VERSION + 1,在 `run_versioned_migrations` 加
/// 对应 match arm,**不要**改 legacy_baseline_v1(保持已发布版本里的语义)。
pub fn run_migrations(conn: &Connection) -> Result<(), AppError> {
    let current = get_user_version(conn)?;

    // 降级保护:用户装了新版(写了更高 user_version)又回退到旧版 app 时,
    // 旧 app 看到高于自己认知的 schema 会读到不存在的列、静默出错。明确报错而不是硬跑。
    if current > CURRENT_SCHEMA_VERSION {
        return Err(AppError::internal(format!(
            "数据库 schema 版本 {current} 高于当前应用支持的 {CURRENT_SCHEMA_VERSION},请升级 AgentGate 后再启动"
        )));
    }

    if current < 1 {
        legacy_baseline_v1(conn)?;
        set_user_version(conn, 1)?;
    }

    run_versioned_migrations(conn, current.max(1))?;

    // 防御性自检:跑完所有迁移后,user_version 必须等于 CURRENT_SCHEMA_VERSION。
    // 不等说明加了新 version 但忘了在 `run_versioned_migrations` 里 set_user_version,
    // 或 set 错值。debug build 直接 panic 提示;release 静默(避免锁死用户启动)。
    let final_version = get_user_version(conn)?;
    debug_assert_eq!(
        final_version, CURRENT_SCHEMA_VERSION,
        "schema version drift: expected {CURRENT_SCHEMA_VERSION}, got {final_version}"
    );

    Ok(())
}

/// 新迁移按 version 分支。
fn run_versioned_migrations(conn: &Connection, from_version: u32) -> Result<(), AppError> {
    if from_version < 2 {
        // v2:Codex remote compaction v2 本地实现的两个开关字段。
        // 默认 enabled=1(开)+ summary_max_tokens=1500。
        // legacy_baseline_v1 已经创了 gateway_settings 表,这里只加列。
        conn.execute_batch(
            "ALTER TABLE gateway_settings ADD COLUMN codex_compact_enabled INTEGER NOT NULL DEFAULT 1;
             ALTER TABLE gateway_settings ADD COLUMN codex_compact_summary_max_tokens INTEGER NOT NULL DEFAULT 1500;",
        )?;
        set_user_version(conn, 2)?;
    }
    if from_version < 3 {
        // v3:per-model 上下文窗口覆盖({model_id → window_tokens} JSON)。
        // 用户在 UI 覆盖 catalog 内置窗口;auto_compact 据此算自压缩阈值。
        conn.execute_batch("ALTER TABLE providers ADD COLUMN model_context_windows TEXT;")?;
        set_user_version(conn, 3)?;
    }
    if from_version < 4 {
        // v4:(source, timestamp) 复合索引。Dashboard 的"按策略成本"等查询带
        // `WHERE source='gateway' AND timestamp >= ?`,此前只有单列 timestamp 索引,
        // 时间区间内所有 source 的行(含数万条 session 用量)都要回表过滤 source。
        // 复合索引让 gateway 行的时间区间被直接定位,大库冷缓存首屏明显变快。
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_source_timestamp
                ON request_logs(source, timestamp);",
        )?;
        set_user_version(conn, 4)?;
    }
    if from_version < 5 {
        // v5:lifetime 统计的覆盖索引。get_stats / get_runtime_kpis 每 5s 对全表
        // SUM(tokens/cost/cache),而 request_logs 行含数 KB 的 trace_json,全表扫
        // 等于读整个库文件(实测 306MB 库:74ms 热缓存,冷缓存秒级)。覆盖索引把
        // 聚合需要的窄列单独存一份(~3MB),实测同查询降到 15ms 且冷缓存不再读大行。
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_request_logs_stats
                ON request_logs(source, timestamp, status_code, latency_ms,
                                input_tokens, output_tokens, cost,
                                cache_write_tokens, cache_read_tokens);",
        )?;
        set_user_version(conn, 5)?;
    }
    if from_version < 6 {
        // v6:今日花费预警的开关 + 阈值。带幂等守卫——之前这两列错误地塞在
        // legacy_baseline_v1 里(只对全新 DB 跑),已 current>=1 的存量用户漏列、
        // 升级后 get() 崩;此处补回。守卫还能兼容"曾被手动加过列"的 DB,避免
        // 重复 ALTER 报错。
        let has_cost_alert = conn
            .prepare("SELECT cost_alert_enabled FROM gateway_settings LIMIT 0")
            .is_ok();
        if !has_cost_alert {
            conn.execute_batch(
                "ALTER TABLE gateway_settings ADD COLUMN cost_alert_enabled INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE gateway_settings ADD COLUMN cost_alert_threshold REAL;",
            )?;
        }
        set_user_version(conn, 6)?;
    }
    Ok(())
}

/// 1.3.6 及更早版本累计下来的所有 schema 操作。**所有语句必须 idempotent**
/// (`CREATE IF NOT EXISTS` / `is_ok` 守卫 / 等价手段),因为已有用户首次升级
/// 会跑这里一次,跑完才会 set user_version=1。
fn legacy_baseline_v1(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS providers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            provider_type TEXT NOT NULL,
            base_url TEXT NOT NULL,
            api_key TEXT,
            default_model TEXT NOT NULL,
            reasoning_model TEXT,
            protocol TEXT NOT NULL,
            timeout_seconds INTEGER NOT NULL DEFAULT 120,
            status TEXT NOT NULL DEFAULT 'not_tested',
            enabled INTEGER NOT NULL DEFAULT 1,
            is_active INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS gateway_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            host TEXT NOT NULL DEFAULT '127.0.0.1',
            port INTEGER NOT NULL DEFAULT 9090,
            active_provider_id TEXT,
            input_protocol TEXT NOT NULL DEFAULT 'openai_responses',
            output_protocol TEXT NOT NULL DEFAULT 'openai_chat_completions',
            auto_start INTEGER NOT NULL DEFAULT 0,
            log_retention_days INTEGER NOT NULL DEFAULT 14,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS request_logs (
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
            error_message TEXT
        );

        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        ",
    )?;

    // Phase 6: route profiles tables
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS route_profiles (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            client_type TEXT NOT NULL,
            input_protocol TEXT NOT NULL,
            mode TEXT NOT NULL DEFAULT 'manual',
            active_provider_id TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            is_default INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS route_profile_providers (
            id TEXT PRIMARY KEY,
            route_profile_id TEXT NOT NULL,
            provider_id TEXT NOT NULL,
            priority INTEGER NOT NULL,
            enabled INTEGER NOT NULL DEFAULT 1,
            model_override TEXT,
            max_retries INTEGER NOT NULL DEFAULT 0,
            cooldown_seconds INTEGER NOT NULL DEFAULT 600,
            failover_on_status_codes TEXT,
            failover_on_error_keywords TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS provider_runtime_status (
            provider_id TEXT PRIMARY KEY,
            available INTEGER NOT NULL DEFAULT 1,
            consecutive_failures INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            last_error_code TEXT,
            last_error_at TEXT,
            cooldown_until TEXT,
            quota_exhausted INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        );
        ",
    )?;

    // Migration: add supported_models column to providers
    let has_sm: bool = conn
        .prepare("SELECT supported_models FROM providers LIMIT 0")
        .is_ok();
    if !has_sm {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN supported_models TEXT;")?;
    }

    // Migration: add model_mapping column to providers
    let has_mm: bool = conn
        .prepare("SELECT model_mapping FROM providers LIMIT 0")
        .is_ok();
    if !has_mm {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN model_mapping TEXT;")?;
    }

    // Migration: add extra_headers column to providers
    let has_eh: bool = conn
        .prepare("SELECT extra_headers FROM providers LIMIT 0")
        .is_ok();
    if !has_eh {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN extra_headers TEXT;")?;
    }

    // Migration: add anthropic_base_url column to providers
    let has_abu: bool = conn
        .prepare("SELECT anthropic_base_url FROM providers LIMIT 0")
        .is_ok();
    if !has_abu {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN anthropic_base_url TEXT;")?;
    }

    // Migration: add supports_vision column to providers
    let has_sv: bool = conn
        .prepare("SELECT supports_vision FROM providers LIMIT 0")
        .is_ok();
    if !has_sv {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN supports_vision INTEGER;")?;
    }

    // Migration: add responses_base_url column to providers
    let has_rbu: bool = conn
        .prepare("SELECT responses_base_url FROM providers LIMIT 0")
        .is_ok();
    if !has_rbu {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN responses_base_url TEXT;")?;
    }

    // Phase 7b: model_pricing table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS model_pricing (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL,
            model_pattern TEXT NOT NULL,
            input_price REAL NOT NULL,
            output_price REAL NOT NULL,
            is_custom INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL
        )",
    )?;
    // Populate defaults
    crate::storage::pricing::ensure_defaults(conn)?;

    // Migration: add cost column to request_logs
    let has_cost: bool = conn
        .prepare("SELECT cost FROM request_logs LIMIT 0")
        .is_ok();
    if !has_cost {
        conn.execute_batch("ALTER TABLE request_logs ADD COLUMN cost REAL;")?;
    }
    // Migration: add auto_cache_control and supports_cache columns
    let has_acc: bool = conn
        .prepare("SELECT auto_cache_control FROM providers LIMIT 0")
        .is_ok();
    if !has_acc {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN auto_cache_control INTEGER;")?;
    }
    let has_sc: bool = conn
        .prepare("SELECT supports_cache FROM providers LIMIT 0")
        .is_ok();
    if !has_sc {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN supports_cache INTEGER;")?;
    }

    // Migration: route_profiles.selection_strategy —— failover 候选排序策略。
    // 'priority'（默认，手工顺序）/ 'cheapest'（单价升序）/ 'fastest'（近期延迟升序）。
    // 默认 'priority' 保证旧路由行为不变。
    let has_sel_strategy: bool = conn
        .prepare("SELECT selection_strategy FROM route_profiles LIMIT 0")
        .is_ok();
    if !has_sel_strategy {
        conn.execute_batch(
            "ALTER TABLE route_profiles ADD COLUMN selection_strategy TEXT NOT NULL DEFAULT 'priority';",
        )?;
    }

    // Migration: provider_runtime_status 加主动健康探测列。
    // 这些列只反映后台探测结果，仅用于展示，绝不参与路由（available/cooldown 才参与）。
    let has_probe: bool = conn
        .prepare("SELECT last_probe_ok FROM provider_runtime_status LIMIT 0")
        .is_ok();
    if !has_probe {
        conn.execute_batch(
            "ALTER TABLE provider_runtime_status ADD COLUMN last_probe_ok INTEGER;
             ALTER TABLE provider_runtime_status ADD COLUMN last_probe_at TEXT;
             ALTER TABLE provider_runtime_status ADD COLUMN last_probe_latency_ms INTEGER;
             ALTER TABLE provider_runtime_status ADD COLUMN last_probe_error TEXT;",
        )?;
    }

    // Migration: gateway_settings.health_probe_enabled —— 后台健康探测开关，默认关。
    let has_hp: bool = conn
        .prepare("SELECT health_probe_enabled FROM gateway_settings LIMIT 0")
        .is_ok();
    if !has_hp {
        conn.execute_batch(
            "ALTER TABLE gateway_settings ADD COLUMN health_probe_enabled INTEGER NOT NULL DEFAULT 0;",
        )?;
    }

    // 注:gateway_settings 的 cost_alert 列改由 v6 versioned migration 负责
    // (见 run_versioned_migrations)。新列必须走 versioned + bump version,
    // 否则已 current>=1 的存量用户跑不到 baseline、永远漏列(2026-06 踩过)。

    // Migration: add model_capabilities column to providers
    // Stores per-model capability matrix as JSON: {"model_id": ["text","vision","reasoning",...]}
    // Routing layer uses this to pick the right model when request features (image/audio/...)
    // demand a capability the default_model lacks. Supersedes the per-provider supports_vision
    // flag for vision-aware routing while keeping it as a coarse summary.
    let has_mc: bool = conn
        .prepare("SELECT model_capabilities FROM providers LIMIT 0")
        .is_ok();
    if !has_mc {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN model_capabilities TEXT;")?;
    }

    // Migration: add routing_conditions to route_profile_providers
    let has_rc: bool = conn
        .prepare("SELECT routing_conditions FROM route_profile_providers LIMIT 0")
        .is_ok();
    if !has_rc {
        conn.execute_batch(
            "ALTER TABLE route_profile_providers ADD COLUMN routing_conditions TEXT;",
        )?;
    }

    // Backfill cost for logs that have tokens but no cost (runs on every startup,
    // catches newly added pricing defaults and previously unmatched models)
    let _ = crate::storage::pricing::backfill_costs(conn);

    // Phase 7: config_backups table
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS config_backups (
            id TEXT PRIMARY KEY,
            tool_type TEXT NOT NULL,
            source_path TEXT NOT NULL,
            backup_path TEXT NOT NULL,
            backup_kind TEXT NOT NULL,
            created_at TEXT NOT NULL,
            metadata_json TEXT
        );
        ",
    )?;

    // Index on request_logs.timestamp for stats query performance
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_request_logs_timestamp ON request_logs(timestamp);",
    )?;

    // Clean up orphan route_profile_providers referencing deleted providers
    conn.execute_batch(
        "DELETE FROM route_profile_providers WHERE provider_id NOT IN (SELECT id FROM providers);",
    )?;

    // Migration: add trace_json column if not present
    let has_trace: bool = conn
        .prepare("SELECT trace_json FROM request_logs LIMIT 0")
        .is_ok();
    if !has_trace {
        conn.execute_batch("ALTER TABLE request_logs ADD COLUMN trace_json TEXT;")?;
    }

    // Migration: provider_quirks (JSON) — per-provider known-bad request fields
    // and reasoning/thinking parameter bounds. Consumed by gateway refiners
    // (body_filter, thinking_rectifier) when their switches are on.
    let has_pq: bool = conn
        .prepare("SELECT provider_quirks FROM providers LIMIT 0")
        .is_ok();
    if !has_pq {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN provider_quirks TEXT;")?;
    }

    // Migration: per-provider refiner switches. NULL = inherit global toggle
    // from gateway_settings, 0 = force off, 1 = force on. Default NULL keeps
    // every existing provider in "transparent" mode until the user opts in.
    for col in [
        "body_filter_enabled",
        "thinking_rectifier_enabled",
        "error_mapper_enabled",
    ] {
        let has: bool = conn
            .prepare(&format!("SELECT {col} FROM providers LIMIT 0"))
            .is_ok();
        if !has {
            conn.execute_batch(&format!("ALTER TABLE providers ADD COLUMN {col} INTEGER;"))?;
        }
    }

    // Migration: model_degradation_chain (JSON map { requested_model → [fallback,...] }).
    // Used by provider_selector when the primary model on a provider fails;
    // the gateway walks the chain before moving to the next failover candidate.
    let has_mdc: bool = conn
        .prepare("SELECT model_degradation_chain FROM providers LIMIT 0")
        .is_ok();
    if !has_mdc {
        conn.execute_batch("ALTER TABLE providers ADD COLUMN model_degradation_chain TEXT;")?;
    }

    // Phase 8: client_apply_history — one row per `apply` / `disable` /
    // `toggle` call against any of the 5 clients (codex / claude_code /
    // opencode / gemini / atomcode). Stores a snapshot of the pre-write
    // on-disk config so the user can roll back from the UI. Retention:
    // first 'initial' row per client kept forever + most recent 10 others.
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS client_apply_history (
            id TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            action TEXT NOT NULL,
            snapshot_json TEXT NOT NULL,
            summary TEXT NOT NULL,
            is_initial INTEGER NOT NULL DEFAULT 0,
            agentgate_version TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_client_apply_history_client_created
          ON client_apply_history(client_id, created_at DESC);
        ",
    )?;

    // Migration: global refiner toggles on gateway_settings. Default 0 (off)
    // — per-provider switch can still force on if user opts in explicitly.
    for col in [
        "body_filter_global",
        "thinking_rectifier_global",
        "error_mapper_global",
    ] {
        let has: bool = conn
            .prepare(&format!("SELECT {col} FROM gateway_settings LIMIT 0"))
            .is_ok();
        if !has {
            conn.execute_batch(&format!(
                "ALTER TABLE gateway_settings ADD COLUMN {col} INTEGER NOT NULL DEFAULT 0;"
            ))?;
        }
    }

    // Migration: source / session_id / external_id 三列 —— request_logs 不再
    // 是 gateway 专属，要能同时容纳从客户端本地日志（Claude / Codex / Gemini）
    // 扫出的条目。
    //   - source：'gateway' / 'claude_session' / 'codex_session' / 'gemini_session'
    //     老数据 backfill 成 'gateway'。NOT NULL 但 default 走 SQLite ALTER 限制
    //     （不允许非常量 default），用 backfill UPDATE 补齐。
    //   - session_id：客户端的会话指纹 / session_id；按会话聚合视图用。
    //   - external_id：每条客户端日志的稳定唯一 ID（message_id / event_id），
    //     供 session 同步去重。gateway 路径填请求的 UUID。
    let has_source: bool = conn
        .prepare("SELECT source FROM request_logs LIMIT 0")
        .is_ok();
    if !has_source {
        conn.execute_batch(
            "ALTER TABLE request_logs ADD COLUMN source TEXT;
             UPDATE request_logs SET source = 'gateway' WHERE source IS NULL;",
        )?;
    }
    let has_sid: bool = conn
        .prepare("SELECT session_id FROM request_logs LIMIT 0")
        .is_ok();
    if !has_sid {
        conn.execute_batch("ALTER TABLE request_logs ADD COLUMN session_id TEXT;")?;
    }
    let has_ext: bool = conn
        .prepare("SELECT external_id FROM request_logs LIMIT 0")
        .is_ok();
    if !has_ext {
        conn.execute_batch(
            "ALTER TABLE request_logs ADD COLUMN external_id TEXT;
             CREATE INDEX IF NOT EXISTS idx_request_logs_source_external_id
                ON request_logs(source, external_id)
                WHERE external_id IS NOT NULL;",
        )?;
    }

    // Migration: split cache tokens into Write vs Read so the dashboard can
    // show the real value of session affinity / prompt caching. Upstream
    // formats:
    //   - Anthropic: usage.cache_creation_input_tokens (Write)
    //                usage.cache_read_input_tokens     (Read)
    //   - OpenAI Responses:    input_tokens_details.cached_tokens     (Read)
    //   - OpenAI Chat Completions: prompt_tokens_details.cached_tokens (Read)
    //   - Some Chinese providers: bare `cached_tokens` (Read)
    // OpenAI-family doesn't separately track cache writes; we leave Write as
    // NULL for those rows so the UI can render "—" rather than a misleading 0.
    let has_cwt: bool = conn
        .prepare("SELECT cache_write_tokens FROM request_logs LIMIT 0")
        .is_ok();
    if !has_cwt {
        conn.execute_batch("ALTER TABLE request_logs ADD COLUMN cache_write_tokens INTEGER;")?;
    }
    let has_crt: bool = conn
        .prepare("SELECT cache_read_tokens FROM request_logs LIMIT 0")
        .is_ok();
    if !has_crt {
        conn.execute_batch("ALTER TABLE request_logs ADD COLUMN cache_read_tokens INTEGER;")?;
    }

    // Ensure gateway_settings has exactly one row
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM gateway_settings", [], |row| {
        row.get(0)
    })?;
    if count == 0 {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO gateway_settings (id, host, port, input_protocol, output_protocol, auto_start, log_retention_days, updated_at)
             VALUES (1, '127.0.0.1', 9090, 'openai_responses', 'openai_chat_completions', 0, 14, ?1)",
            [&now],
        )?;
    }

    // Migration: convert protocol from single string to JSON array
    // e.g. "openai_chat_completions" → '["openai_chat_completions"]'
    conn.execute_batch(
        "UPDATE providers SET protocol = '[\"' || protocol || '\"]' WHERE protocol NOT LIKE '[%';",
    )?;

    // Migration: rename "OpenCode Default" route profile to "Chat Completions Default"
    conn.execute_batch(
        "UPDATE route_profiles SET name = 'Chat Completions Default' WHERE name = 'OpenCode Default' AND input_protocol = 'openai_chat_completions';",
    )?;

    // Seed default providers on first run
    seed_default_providers(conn)?;

    // Remove demo request logs seeded by older builds. Real gateway request IDs
    // are UUIDs, while these sample rows used a stable req-seed-* prefix.
    conn.execute(
        "DELETE FROM request_logs WHERE request_id LIKE 'req-seed-%'",
        [],
    )?;

    // Seed default route profile on first run
    seed_default_route_profile(conn)?;

    // Pet settings table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pet_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            pet_type TEXT NOT NULL DEFAULT 'robot',
            visible INTEGER NOT NULL DEFAULT 1,
            pos_x REAL NOT NULL DEFAULT 100.0,
            pos_y REAL NOT NULL DEFAULT 100.0
        );",
    )?;
    // Ensure pet_settings has exactly one row
    let pet_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM pet_settings", [], |row| row.get(0))?;
    if pet_count == 0 {
        conn.execute(
            "INSERT INTO pet_settings (id, pet_type, visible, pos_x, pos_y) VALUES (1, 'robot', 1, 100.0, 100.0)",
            [],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_migrations_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"providers".to_string()));
        assert!(tables.contains(&"gateway_settings".to_string()));
        assert!(tables.contains(&"request_logs".to_string()));
        assert!(tables.contains(&"app_settings".to_string()));
        assert!(tables.contains(&"route_profiles".to_string()));
        assert!(tables.contains(&"config_backups".to_string()));
        assert!(tables.contains(&"client_apply_history".to_string()));
        assert!(tables.contains(&"pet_settings".to_string()));
        assert!(tables.contains(&"provider_runtime_status".to_string()));
    }

    #[test]
    fn run_migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // should not panic or error
    }

    #[test]
    fn v5_stats_covering_index_serves_lifetime_aggregates() {
        // request_logs 行里 trace_json 动辄数 KB,lifetime SUM 全表扫等于把
        // 整个库文件读一遍(实测 306MB 库冷缓存秒级)。覆盖索引让聚合只扫
        // 数 MB 的索引。断言:索引存在 + 查询计划确实走 COVERING INDEX。
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_request_logs_stats'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "v5 应创建 idx_request_logs_stats");

        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT SUM(input_tokens), SUM(output_tokens), SUM(cost),
                    SUM(cache_read_tokens), SUM(cache_write_tokens) FROM request_logs",
                [],
                |r| r.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("COVERING INDEX idx_request_logs_stats"),
            "lifetime 聚合应走覆盖索引,实际计划: {plan}"
        );
    }

    #[test]
    fn db_newer_than_app_is_rejected() {
        // 模拟用户装过更新版(user_version 高于当前 app),回退旧版启动应明确报错。
        let conn = Connection::open_in_memory().unwrap();
        set_user_version(&conn, CURRENT_SCHEMA_VERSION + 1).unwrap();
        assert!(run_migrations(&conn).is_err(), "schema 高于 app 支持应报错");
    }

    #[test]
    fn run_migrations_sets_user_version() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(get_user_version(&conn).unwrap(), 0, "new DB starts at 0");
        run_migrations(&conn).unwrap();
        assert_eq!(
            get_user_version(&conn).unwrap(),
            CURRENT_SCHEMA_VERSION,
            "migrations should bump user_version to current"
        );
    }

    #[test]
    fn run_migrations_skips_baseline_when_already_v1() {
        // 模拟"已升级到 v1"的 DB:跑完 baseline + 手工 set user_version=1。
        // 再跑 run_migrations 应该跳过 baseline(不重建表),但跑 v2 ALTER 推到 v2。
        let conn = Connection::open_in_memory().unwrap();
        legacy_baseline_v1(&conn).unwrap();
        set_user_version(&conn, 1).unwrap();
        run_migrations(&conn).unwrap();
        // 推到当前版本
        assert_eq!(get_user_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
        // v2 加的新列存在
        let has_col: bool = conn
            .prepare("SELECT codex_compact_enabled FROM gateway_settings LIMIT 0")
            .is_ok();
        assert!(has_col, "v2 ALTER 应加上 codex_compact_enabled 列");
    }

    #[test]
    fn migration_v3_adds_model_context_windows() {
        // 复现 bug:user_version=2 的老库升级时必须加上 model_context_windows 列。
        // 之前误把该列加在 legacy_baseline_v1(老用户 user_version≥1 后不再执行),
        // 导致列缺失、providers 查询 "no such column" 报错、UI 供应商列表全空。
        let conn = Connection::open_in_memory().unwrap();
        legacy_baseline_v1(&conn).unwrap();
        set_user_version(&conn, 2).unwrap();
        assert!(
            conn.prepare("SELECT model_context_windows FROM providers LIMIT 0")
                .is_err(),
            "v2 库此时不该有该列"
        );
        run_migrations(&conn).unwrap();
        assert_eq!(get_user_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(
            conn.prepare("SELECT model_context_windows FROM providers LIMIT 0")
                .is_ok(),
            "v3 迁移应加上 model_context_windows 列"
        );
    }

    #[test]
    fn migration_v4_adds_source_timestamp_index() {
        // v4:(source, timestamp) 复合索引,修 Dashboard 大库冷缓存首屏慢
        // (gateway 过滤查询此前全表扫 / 单列索引回表过滤 source)。
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        assert_eq!(get_user_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
        let has_index: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='index' AND name='idx_request_logs_source_timestamp'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap();
        assert!(has_index, "v4 迁移应建 (source, timestamp) 复合索引");
    }

    #[test]
    fn legacy_db_without_user_version_gets_migrated() {
        // 已有 1.3.6 用户的真实路径:DB 已有完整 schema,但 user_version 还是 0。
        // 第一次跑新 run_migrations 应该:跑一遍 baseline(全 idempotent,无副作用)+ 推到当前 version。
        let conn = Connection::open_in_memory().unwrap();
        // 先跑一遍模拟已升级到 1.3.6 schema
        legacy_baseline_v1(&conn).unwrap();
        // user_version 仍是 0(1.3.6 没设)
        assert_eq!(get_user_version(&conn).unwrap(), 0);
        // 模拟用户升级到下一版后首次启动
        run_migrations(&conn).unwrap();
        assert_eq!(get_user_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
        // 数据仍在(没被破坏)
        let provider_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .unwrap();
        assert!(provider_count > 0, "seed providers 应仍存在");
    }

    #[test]
    fn existing_v5_db_gets_cost_alert_columns_on_upgrade() {
        // 复现 2026-06 实际事故:存量用户 DB 已 current=5,但缺 cost_alert 列
        // ——因为该列曾被错误塞进 legacy_baseline_v1(只对全新 DB 跑),current>=1
        // 的存量用户跑不到、永远漏列,升级后 gateway_settings::get()(查全部列)崩。
        // 修复后 cost_alert 改由 v6 versioned migration 补,此测试锁死这条升级路径。
        let conn = Connection::open_in_memory().unwrap();
        // 构造真实的 v5 DB:baseline + v2 的 codex_compact 列(versioned 列不在 baseline)。
        // 这样 gateway_settings 拥有除 v6 cost_alert 外的全部列,精确复现存量 v5 用户。
        legacy_baseline_v1(&conn).unwrap();
        conn.execute_batch(
            "ALTER TABLE gateway_settings ADD COLUMN codex_compact_enabled INTEGER NOT NULL DEFAULT 1;
             ALTER TABLE gateway_settings ADD COLUMN codex_compact_summary_max_tokens INTEGER NOT NULL DEFAULT 1500;",
        )
        .unwrap();
        set_user_version(&conn, 5).unwrap(); // 模拟已升级到 v5 的存量用户

        // 前提自检:此时确实缺 cost_alert 列,否则升级补列路径没被测到。
        assert!(
            conn.prepare("SELECT cost_alert_enabled FROM gateway_settings LIMIT 0")
                .is_err(),
            "v5 存量 DB 不应有 cost_alert 列,否则测不到 v6 补列"
        );

        run_migrations(&conn).unwrap();
        assert_eq!(get_user_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);

        // v6 补列后 get() 必须成功、不报 DATABASE_ERROR。
        let settings = crate::storage::gateway_settings::get(&conn)
            .expect("v5 存量 DB 升级后 gateway_settings::get() 必须成功");
        assert_eq!(settings.id, 1);
        assert!(!settings.cost_alert_enabled);
        assert!(settings.cost_alert_threshold.is_none());
    }

    #[test]
    fn v6_migration_is_idempotent_on_manually_patched_db() {
        // 边缘:有 DB 曾被手动补过 cost_alert 列(线上热修),current 仍 5。
        // v6 的幂等守卫必须跳过 ALTER,不能因列已存在而报错导致启动失败。
        let conn = Connection::open_in_memory().unwrap();
        legacy_baseline_v1(&conn).unwrap();
        // 真实 v5 DB(含 codex_compact)+ 线上热修手动补过的 cost_alert 列。
        conn.execute_batch(
            "ALTER TABLE gateway_settings ADD COLUMN codex_compact_enabled INTEGER NOT NULL DEFAULT 1;
             ALTER TABLE gateway_settings ADD COLUMN codex_compact_summary_max_tokens INTEGER NOT NULL DEFAULT 1500;
             ALTER TABLE gateway_settings ADD COLUMN cost_alert_enabled INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE gateway_settings ADD COLUMN cost_alert_threshold REAL;",
        )
        .unwrap();
        set_user_version(&conn, 5).unwrap();

        run_migrations(&conn).expect("已手动补列的 DB 跑 v6 不应报错");
        assert_eq!(get_user_version(&conn).unwrap(), CURRENT_SCHEMA_VERSION);
        assert!(crate::storage::gateway_settings::get(&conn).is_ok());
    }

    #[test]
    fn run_migrations_seeds_gateway_settings() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM gateway_settings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn run_migrations_seeds_pet_settings() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let pet_type: String = conn
            .query_row("SELECT pet_type FROM pet_settings WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(pet_type, "robot");
    }

    #[test]
    fn run_migrations_seeds_deepseek_v4_provider() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        let (default_model, reasoning_model, supported_models, anthropic_base_url, protocol): (
            String,
            String,
            String,
            String,
            String,
        ) = conn
            .query_row(
                "SELECT default_model, reasoning_model, supported_models, anthropic_base_url, protocol FROM providers WHERE provider_type='deepseek'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(default_model, "deepseek-v4-flash");
        assert_eq!(reasoning_model, "deepseek-v4-pro");
        assert_eq!(
            supported_models,
            r#"["deepseek-v4-flash","deepseek-v4-pro"]"#
        );
        assert_eq!(anthropic_base_url, "https://api.deepseek.com/anthropic");
        assert!(protocol.contains("anthropic_messages"));
    }
}

fn seed_default_providers(conn: &Connection) -> Result<(), AppError> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))?;
    if count > 0 {
        return Ok(());
    }

    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO providers (id, name, provider_type, base_url, default_model, reasoning_model, supported_models, anthropic_base_url, protocol, timeout_seconds, status, enabled, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, 1, ?12, ?12)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            "DeepSeek",
            "deepseek",
            "https://api.deepseek.com",
            "deepseek-v4-flash",
            "deepseek-v4-pro",
            r#"["deepseek-v4-flash","deepseek-v4-pro"]"#,
            "https://api.deepseek.com/anthropic",
            r#"["openai_chat_completions","anthropic_messages"]"#,
            120,
            "not_tested",
            &now,
        ],
    )?;

    conn.execute(
        "INSERT INTO providers (id, name, provider_type, base_url, default_model, protocol, timeout_seconds, status, enabled, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 0, ?9, ?9)",
        rusqlite::params![
            uuid::Uuid::new_v4().to_string(),
            "Custom OpenAI Compatible",
            "custom_openai_compatible",
            "http://localhost:8000",
            "custom-model",
            r#"["openai_chat_completions"]"#,
            120,
            "not_tested",
            &now,
        ],
    )?;

    // Set active_provider_id in gateway_settings
    let active_id: Option<String> = conn
        .query_row(
            "SELECT id FROM providers WHERE is_active = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Some(id) = active_id {
        conn.execute(
            "UPDATE gateway_settings SET active_provider_id = ?1, updated_at = ?2 WHERE id = 1",
            rusqlite::params![&id, &now],
        )?;
    }

    Ok(())
}

fn seed_default_route_profile(conn: &Connection) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();

    let active_provider_id: Option<String> = conn
        .query_row(
            "SELECT id FROM providers WHERE is_active = 1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    let mut stmt = conn.prepare(
        "SELECT id FROM providers WHERE enabled = 1 ORDER BY is_active DESC, created_at ASC",
    )?;
    let provider_ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    let default_codes = serde_json::json!([402, 429, 500, 502, 503, 504]).to_string();
    let default_kw = serde_json::json!([
        "quota",
        "insufficient balance",
        "rate limit",
        "too many requests",
        "timeout"
    ])
    .to_string();

    // Seed one default profile per protocol (skip if already exists for that protocol)
    let profiles = [
        ("Codex Default", "openai_responses"),
        ("Claude Code Default", "anthropic_messages"),
        ("Chat Completions Default", "openai_chat_completions"),
    ];

    for (name, protocol) in profiles {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM route_profiles WHERE input_protocol = ?1",
            [protocol],
            |row| row.get(0),
        )?;
        if exists > 0 {
            continue;
        }

        let profile_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO route_profiles (id, name, client_type, input_protocol, mode, active_provider_id, enabled, is_default, created_at, updated_at)
             VALUES (?1, ?2, '', ?3, 'manual', ?4, 1, 1, ?5, ?5)",
            rusqlite::params![&profile_id, name, protocol, &active_provider_id, &now],
        )?;

        for (i, pid) in provider_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO route_profile_providers (id, route_profile_id, provider_id, priority, enabled, max_retries, cooldown_seconds, failover_on_status_codes, failover_on_error_keywords, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, 0, 600, ?5, ?6, ?7, ?7)",
                rusqlite::params![uuid::Uuid::new_v4().to_string(), &profile_id, pid, (i + 1) as i64, &default_codes, &default_kw, &now],
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO provider_runtime_status (provider_id, available, consecutive_failures, quota_exhausted, updated_at) VALUES (?1, 1, 0, 0, ?2)",
                rusqlite::params![pid, &now],
            )?;
        }
    }

    Ok(())
}
