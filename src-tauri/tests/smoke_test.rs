//! Release preflight smoke tests — run with real API keys against the local gateway.
//!
//! These tests connect to your REAL SQLite database and send REAL HTTP requests
//! to upstream providers. Each request costs a few tokens (max_tokens=5).
//!
//! Run only when explicitly enabled:
//!   AG_RUN_SMOKE_TESTS=1 cargo test --test smoke_test -- --nocapture
//!
//! Optional env vars:
//!   AG_SMOKE_DB_PATH    — override default DB path
//!   AG_SMOKE_HOST       — gateway bind host (default: 127.0.0.1)
//!   AG_SMOKE_PORT       — gateway bind port (default: 19090)
//!   AG_SMOKE_TIMEOUT    — per-request timeout seconds (default: 60)

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agentgate_lib::gateway::server;
use agentgate_lib::security::local_token;
use agentgate_lib::storage;

/// Try to locate the user's real AgentGate database.
fn default_db_path() -> Option<PathBuf> {
    // macOS
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        let p = PathBuf::from(home)
            .join("Library/Application Support/com.mengmian.agentgate/agentgate.db");
        if p.exists() {
            return Some(p);
        }
    }
    // Linux
    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").ok()?;
        let p = PathBuf::from(home).join(".local/share/agentgate/agentgate.db");
        if p.exists() {
            return Some(p);
        }
    }
    // Windows
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var("APPDATA").ok()?;
        let p = PathBuf::from(app_data).join("agentgate/agentgate.db");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn db_path() -> PathBuf {
    if let Ok(v) = std::env::var("AG_SMOKE_DB_PATH") {
        return PathBuf::from(v);
    }
    default_db_path().expect(
        "Could not find agentgate.db. Set AG_SMOKE_DB_PATH or ensure the app has run once."
    )
}

fn smoke_host() -> String {
    std::env::var("AG_SMOKE_HOST").unwrap_or_else(|_| "127.0.0.1".into())
}

fn smoke_port() -> u16 {
    std::env::var("AG_SMOKE_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(19090)
}

fn smoke_timeout() -> Duration {
    let secs = std::env::var("AG_SMOKE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60u64);
    Duration::from_secs(secs)
}

/// 读环境变量，空字符串视为未设。
fn env_opt(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// Redact 常见 provider key 前缀。**不是安全边界**——真正的防御靠不打印
/// raw upstream body，本函数只是 200 字符错误片段的二道防线（万一上游错
/// 误响应回显了 key）。
fn sanitize_for_log(text: &str) -> String {
    const PREFIXES: &[&str] = &["sk-", "tp-", "gsk_", "csk-", "ak-"];
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let mut hit: Option<&str> = None;
        for p in PREFIXES {
            let pb = p.as_bytes();
            if i + pb.len() <= bytes.len() && &bytes[i..i + pb.len()] == pb {
                hit = Some(*p);
                break;
            }
        }
        if let Some(prefix) = hit {
            let mut end = i + prefix.len();
            while end < bytes.len() {
                let b = bytes[end];
                let is_delim = matches!(
                    b,
                    b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\''
                    | b',' | b'}' | b']' | b')' | b'>' | b':'
                );
                if is_delim { break; }
                end += 1;
            }
            // 长度 > prefix + 4 才视为 key（避免误删 "sk-" 这种普通文本）
            if end - i > prefix.len() + 4 {
                out.extend_from_slice(prefix.as_bytes());
                out.extend_from_slice(b"***");
                i = end;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    // 仅插入 ASCII；非 ASCII 字节按原样保留——结果仍是合法 UTF-8。
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// 截 200 字符 + sanitize，专供 println! 错误日志使用。
fn snip(text: &str) -> String {
    let cut: String = text.chars().take(200).collect();
    sanitize_for_log(&cut)
}

/// 查给定 route 最近一条 request_logs 的 trace_json。smoke 测试串行跑、
/// 隔离在 19090 端口，不与用户 app 同台并发——LIMIT 1 ORDER BY id DESC
/// 拿到的就是当前测试自己的那条。
fn fetch_recent_trace(
    db: &Arc<Mutex<rusqlite::Connection>>,
    route: &str,
) -> Option<serde_json::Value> {
    let c = db.lock().ok()?;
    let tj: Option<String> = c.query_row(
        "SELECT trace_json FROM request_logs WHERE route = ?1 ORDER BY id DESC LIMIT 1",
        [route],
        |row| row.get(0),
    ).ok()?;
    serde_json::from_str(&tj?).ok()
}

/// 把目标 provider 临时设为 active，Drop 时恢复原 active。
/// 测试用：保证不论 panic / early return / 正常退出，都还原用户的 active 配置。
struct ActiveProviderGuard {
    db: Arc<Mutex<rusqlite::Connection>>,
    original_id: Option<String>,
}

impl ActiveProviderGuard {
    fn activate(
        db: &Arc<Mutex<rusqlite::Connection>>,
        target_id: &str,
    ) -> Result<Self, String> {
        let original = {
            let c = db.lock().map_err(|_| "db lock".to_string())?;
            agentgate_lib::storage::providers::list_all(&c)
                .map_err(|e| format!("list_all: {}", e.message))?
                .into_iter()
                .find(|p| p.is_active)
                .map(|p| p.id)
        };
        {
            let c = db.lock().map_err(|_| "db lock".to_string())?;
            agentgate_lib::storage::providers::set_active(&c, target_id)
                .map_err(|e| format!("set_active({target_id}): {}", e.message))?;
        }
        Ok(Self { db: db.clone(), original_id: original })
    }
}

impl Drop for ActiveProviderGuard {
    fn drop(&mut self) {
        if let Some(ref id) = self.original_id {
            if let Ok(c) = self.db.lock() {
                let _ = agentgate_lib::storage::providers::set_active(&c, id);
            }
        }
    }
}

#[tokio::test]
async fn release_preflight_smoke() {
    if std::env::var("AG_RUN_SMOKE_TESTS").is_err() {
        eprintln!("\n⚠️  Skipping smoke tests. Set AG_RUN_SMOKE_TESTS=1 to run.\n");
        return;
    }

    let db_file = db_path();
    println!("\n🔥 AgentGate Release Preflight Smoke Tests");
    println!("   Database: {}", db_file.display());

    // Open real DB (read-only not possible because WAL + migrations need write)
    let conn = rusqlite::Connection::open(&db_file)
        .expect("open database");
    let db = Arc::new(Mutex::new(conn));

    // Read settings for info
    let settings = {
        let c = db.lock().unwrap();
        storage::gateway_settings::get(&c).expect("read gateway settings")
    };
    println!("   Gateway config: {}:{}  auto_start={}", settings.host, settings.port, settings.auto_start);

    // Ensure local token exists
    let token = local_token::ensure_token().expect("ensure token");
    println!("   Token: {}...", &token[..12.min(token.len())]);

    // Start gateway on a test port
    let host = smoke_host();
    let port = smoke_port();
    println!("   Starting test gateway on {}:{} ...", host, port);

    let (shutdown_tx, server_handle, _active_requests, _port) = server::start(&host, port, db.clone())
        .await
        .expect("start gateway");

    // Wait for server to bind
    tokio::time::sleep(Duration::from_millis(300)).await;
    let base = format!("http://{}:{}", host, port);
    let client = reqwest::Client::builder()
        .timeout(smoke_timeout())
        .build()
        .expect("build http client");

    let mut results: Vec<(String, bool, Option<String>)> = Vec::new();

    // ── 1. Health check ──────────────────────────────────────────────
    {
        let url = format!("{}/health", base);
        let res = client.get(&url).send().await;
        match res {
            Ok(r) if r.status().is_success() => {
                println!("   ✅ Health check — 200");
                results.push(("health".to_string(), true, None));
            }
            Ok(r) => {
                let msg = format!("HTTP {}", r.status());
                println!("   ❌ Health check — {}", msg);
                results.push(("health".to_string(), false, Some(msg)));
            }
            Err(e) => {
                println!("   ❌ Health check — {}", e);
                results.push(("health".to_string(), false, Some(e.to_string())));
            }
        }
    }

    // ── 2. Auth rejection without token ──────────────────────────────
    {
        let url = format!("{}/v1/chat/completions", base);
        let body = serde_json::json!({
            "model": "test", "messages": [{"role":"user","content":"hi"}], "stream": false
        });
        let res = client.post(&url).json(&body).send().await;
        match res {
            Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED => {
                println!("   ✅ Auth rejection — 401 (expected)");
                results.push(("auth_reject".to_string(), true, None));
            }
            Ok(r) => {
                let msg = format!("HTTP {} (expected 401)", r.status());
                println!("   ❌ Auth rejection — {}", msg);
                results.push(("auth_reject".to_string(), false, Some(msg)));
            }
            Err(e) => {
                println!("   ❌ Auth rejection — {}", e);
                results.push(("auth_reject".to_string(), false, Some(e.to_string())));
            }
        }
    }

    // ── 3. Chat Completions (pass-through or transform) ──────────────
    {
        let url = format!("{}/v1/chat/completions", base);
        let res = test_chat_completions(&client, &token, &url).await;
        let ok = res.is_ok();
        match &res {
            Ok(model) => println!("   ✅ Chat Completions — model={}", model),
            Err(e) => println!("   ❌ Chat Completions — {}", e),
        }
        results.push(("chat_completions".to_string(), ok, res.err()));
    }

    // ── 4. Responses API (conversion to chat) ────────────────────────
    {
        let url = format!("{}/v1/responses", base);
        let res = test_responses_api(&client, &token, &url).await;
        let ok = res.is_ok();
        match &res {
            Ok(_) => println!("   ✅ Responses API — 200"),
            Err(e) => println!("   ❌ Responses API — {}", e),
        }
        results.push(("responses_api".to_string(), ok, res.err()));
    }

    // ── 5. Models list (needs valid provider) ────────────────────────
    {
        let url = format!("{}/v1/models", base);
        let res = client.get(&url).bearer_auth(&token).send().await;
        match res {
            Ok(r) if r.status().is_success() => {
                println!("   ✅ Models list — 200");
                results.push(("models_list".to_string(), true, None));
            }
            Ok(r) => {
                let msg = format!("HTTP {}", r.status());
                println!("   ⚠️  Models list — {} (may be ok if no active provider)", msg);
                results.push(("models_list".to_string(), true, Some(msg))); // non-fatal
            }
            Err(e) => {
                println!("   ❌ Models list — {}", e);
                results.push(("models_list".to_string(), false, Some(e.to_string())));
            }
        }
    }

    // ── 6a. Responses API — strict output shape ──────────────────────
    // Hits /v1/responses with a tiny prompt and walks the Responses-shape
    // contract. Verifies that whatever path was taken (translation or
    // native pass-through) produced a wire-correct response object.
    {
        let url = format!("{}/v1/responses", base);
        let res = test_responses_strict(&client, &token, &url).await;
        let ok = res.is_ok();
        match &res {
            Ok(model) => println!("   ✅ Responses strict — model={model}"),
            Err(e) => println!("   ❌ Responses strict — {e}"),
        }
        results.push(("responses_strict".to_string(), ok, res.err()));
    }

    // ── 6b. Anthropic Messages API ───────────────────────────────────
    // Tests /v1/messages directly. Exercises the responses_to_anthropic
    // path for providers without an Anthropic-native base_url, OR the
    // pass-through if the active provider has anthropic_base_url set.
    {
        let url = format!("{}/v1/messages", base);
        let res = test_anthropic_messages(&client, &token, &url).await;
        match &res {
            Ok(_) => println!("   ✅ Anthropic Messages — 200"),
            Err(e) => println!("   ⚠️  Anthropic Messages — {e} (non-fatal)"),
        }
        // Non-fatal: not every setup routes /v1/messages.
        results.push(("anthropic_messages".to_string(), true, res.err()));
    }

    // ── 6c. Session affinity multi-turn (cache_tokens recording) ─────
    // Two-turn conversation with the same opening message. If the upstream
    // surfaces cached_tokens > 0 on the second turn, session_affinity
    // should record the binding — verified indirectly via successful round-
    // trip; the affinity store itself is in-memory and not directly probe-
    // able from here.
    {
        let url = format!("{}/v1/responses", base);
        let res = test_responses_multi_turn(&client, &token, &url).await;
        let ok = res.is_ok();
        match &res {
            Ok(()) => println!("   ✅ Multi-turn responses — 200×2"),
            Err(e) => println!("   ❌ Multi-turn responses — {e}"),
        }
        results.push(("multi_turn".to_string(), ok, res.err()));
    }

    // ── 7. Per-provider connectivity (direct upstream ping) ──────────
    let providers = {
        let c = db.lock().unwrap();
        storage::providers::list_all(&c).unwrap_or_default()
    };
    let enabled: Vec<_> = providers.into_iter().filter(|p| p.enabled).collect();
    println!("\n   Enabled providers: {}", enabled.len());

    for provider in &enabled {
        let res = test_provider_direct(&client, provider).await;
        let ok = res.is_ok();
        match &res {
            Ok(_) => println!("   ✅ {} — direct ping ok", provider.name),
            Err(e) => println!("   ⚠️  {} — direct ping failed: {}", provider.name, e),
        }
        // Mark provider direct tests as non-fatal (network issues happen)
        let label = format!("provider_{}", provider.id);
        results.push((label, ok || true, res.err()));
    }

    // ── 8. 3-主路覆盖测试（按 env 开关，未配置则 skip） ──────────────
    //
    // 设计：每条测试用 env 指定 provider_id + model，guard 临时把该 provider
    // 设为 active，跑完恢复原 active；之后查 request_logs 验 trace 字段。
    //
    // env 矩阵：
    //   主路 ① 协议转换（5 路）：
    //     AG_SMOKE_ANTHROPIC_PROVIDER_ID + AG_SMOKE_ANTHROPIC_MODEL
    //       → Responses→Anthropic + Chat→Anthropic 非流 + 流（同 provider 复用）
    //     AG_SMOKE_GEMINI_PROVIDER_ID + AG_SMOKE_GEMINI_MODEL
    //       → Responses→Gemini
    //     AG_SMOKE_CHAT_ONLY_PROVIDER_ID + AG_SMOKE_CHAT_ONLY_MODEL
    //       → Messages→Chat fallback + Gemini→Chat
    //   主路 ② model_mapping 命中（3 路）：
    //     AG_SMOKE_MAPPING_RESPONSES_PROVIDER_ID + _CLIENT_MODEL
    //     AG_SMOKE_MAPPING_CHAT_PROVIDER_ID + _CLIENT_MODEL
    //     AG_SMOKE_MAPPING_MESSAGES_PROVIDER_ID + _CLIENT_MODEL
    println!();
    println!("   ── 3-主路覆盖测试 ──");

    // 8.1 协议转换：Responses → Anthropic
    {
        let label = "transform_responses_to_anthropic".to_string();
        match (env_opt("AG_SMOKE_ANTHROPIC_PROVIDER_ID"), env_opt("AG_SMOKE_ANTHROPIC_MODEL")) {
            (Some(pid), Some(model)) => {
                match ActiveProviderGuard::activate(&db, &pid) {
                    Ok(_g) => {
                        let res = test_responses_transform_anthropic(&client, &token, &base, &model, &db).await;
                        let ok = res.is_ok();
                        match &res {
                            Ok(()) => println!("   ✅ Responses → Anthropic"),
                            Err(e) => println!("   ❌ Responses → Anthropic — {e}"),
                        }
                        results.push((label, ok, res.err()));
                    }
                    Err(e) => {
                        println!("   ❌ Responses → Anthropic — activate failed: {e}");
                        results.push((label, false, Some(e)));
                    }
                }
            }
            _ => {
                println!("   ⏭️  Responses → Anthropic — skip (env AG_SMOKE_ANTHROPIC_PROVIDER_ID/MODEL 未设)");
                results.push((label, true, Some("SKIP".into())));
            }
        }
    }

    // 8.2 协议转换：Responses → Gemini
    {
        let label = "transform_responses_to_gemini".to_string();
        match (env_opt("AG_SMOKE_GEMINI_PROVIDER_ID"), env_opt("AG_SMOKE_GEMINI_MODEL")) {
            (Some(pid), Some(model)) => {
                match ActiveProviderGuard::activate(&db, &pid) {
                    Ok(_g) => {
                        let res = test_responses_transform_gemini(&client, &token, &base, &model, &db).await;
                        let ok = res.is_ok();
                        match &res {
                            Ok(()) => println!("   ✅ Responses → Gemini"),
                            Err(e) => println!("   ❌ Responses → Gemini — {e}"),
                        }
                        results.push((label, ok, res.err()));
                    }
                    Err(e) => {
                        println!("   ❌ Responses → Gemini — activate failed: {e}");
                        results.push((label, false, Some(e)));
                    }
                }
            }
            _ => {
                println!("   ⏭️  Responses → Gemini — skip");
                results.push((label, true, Some("SKIP".into())));
            }
        }
    }

    // 8.3 + 8.4 协议转换：Chat → Anthropic 非流 + 流（共用 ANTHROPIC env）
    {
        let label_ns = "transform_chat_to_anthropic_non_stream".to_string();
        let label_st = "transform_chat_to_anthropic_stream".to_string();
        match (env_opt("AG_SMOKE_ANTHROPIC_PROVIDER_ID"), env_opt("AG_SMOKE_ANTHROPIC_MODEL")) {
            (Some(pid), Some(model)) => {
                match ActiveProviderGuard::activate(&db, &pid) {
                    Ok(_g) => {
                        // 非流
                        let res_ns = test_chat_transform_anthropic_non_stream(&client, &token, &base, &model, &db).await;
                        let ok_ns = res_ns.is_ok();
                        match &res_ns {
                            Ok(()) => println!("   ✅ Chat → Anthropic 非流"),
                            Err(e) => println!("   ❌ Chat → Anthropic 非流 — {e}"),
                        }
                        results.push((label_ns, ok_ns, res_ns.err()));

                        // 流
                        let res_st = test_chat_transform_anthropic_stream(&client, &token, &base, &model, &db).await;
                        let ok_st = res_st.is_ok();
                        match &res_st {
                            Ok(()) => println!("   ✅ Chat → Anthropic 流"),
                            Err(e) => println!("   ❌ Chat → Anthropic 流 — {e}"),
                        }
                        results.push((label_st, ok_st, res_st.err()));
                    }
                    Err(e) => {
                        println!("   ❌ Chat → Anthropic 非流 — activate failed: {e}");
                        results.push((label_ns, false, Some(e.clone())));
                        results.push((label_st, false, Some(e)));
                    }
                }
            }
            _ => {
                println!("   ⏭️  Chat → Anthropic 非流/流 — skip");
                results.push((label_ns, true, Some("SKIP".into())));
                results.push((label_st, true, Some("SKIP".into())));
            }
        }
    }

    // 8.5 协议转换：Messages → Chat fallback
    {
        let label = "transform_messages_to_chat".to_string();
        match (env_opt("AG_SMOKE_CHAT_ONLY_PROVIDER_ID"), env_opt("AG_SMOKE_CHAT_ONLY_MODEL")) {
            (Some(pid), Some(model)) => {
                match ActiveProviderGuard::activate(&db, &pid) {
                    Ok(_g) => {
                        let res = test_messages_transform_chat_fallback(&client, &token, &base, &model, &db).await;
                        let ok = res.is_ok();
                        match &res {
                            Ok(()) => println!("   ✅ Messages → Chat fallback"),
                            Err(e) => println!("   ❌ Messages → Chat fallback — {e}"),
                        }
                        results.push((label, ok, res.err()));
                    }
                    Err(e) => {
                        println!("   ❌ Messages → Chat fallback — activate failed: {e}");
                        results.push((label, false, Some(e)));
                    }
                }
            }
            _ => {
                println!("   ⏭️  Messages → Chat fallback — skip");
                results.push((label, true, Some("SKIP".into())));
            }
        }
    }

    // 8.6 协议转换：Gemini → Chat
    {
        let label = "transform_gemini_to_chat".to_string();
        match (env_opt("AG_SMOKE_CHAT_ONLY_PROVIDER_ID"), env_opt("AG_SMOKE_CHAT_ONLY_MODEL")) {
            (Some(pid), Some(model)) => {
                match ActiveProviderGuard::activate(&db, &pid) {
                    Ok(_g) => {
                        let res = test_gemini_input_transform_chat(&client, &token, &base, &model, &db).await;
                        let ok = res.is_ok();
                        match &res {
                            Ok(()) => println!("   ✅ Gemini → Chat"),
                            Err(e) => println!("   ❌ Gemini → Chat — {e}"),
                        }
                        results.push((label, ok, res.err()));
                    }
                    Err(e) => {
                        println!("   ❌ Gemini → Chat — activate failed: {e}");
                        results.push((label, false, Some(e)));
                    }
                }
            }
            _ => {
                println!("   ⏭️  Gemini → Chat — skip");
                results.push((label, true, Some("SKIP".into())));
            }
        }
    }

    // 8.7 model_mapping：/v1/responses
    {
        let label = "mapping_responses".to_string();
        match (env_opt("AG_SMOKE_MAPPING_RESPONSES_PROVIDER_ID"), env_opt("AG_SMOKE_MAPPING_RESPONSES_CLIENT_MODEL")) {
            (Some(pid), Some(model)) => {
                match ActiveProviderGuard::activate(&db, &pid) {
                    Ok(_g) => {
                        let url = format!("{}/v1/responses", base);
                        let body = serde_json::json!({
                            "model": model,
                            "input": "Reply with 'ok'.",
                            "stream": false,
                            "max_output_tokens": 16,
                            "temperature": 0.0,
                        });
                        let res = test_passthrough_with_mapping(&client, &token, &url, body, &db, "/v1/responses").await;
                        let ok = res.is_ok();
                        match &res {
                            Ok(()) => println!("   ✅ /v1/responses + model_mapping"),
                            Err(e) => println!("   ❌ /v1/responses + model_mapping — {e}"),
                        }
                        results.push((label, ok, res.err()));
                    }
                    Err(e) => {
                        println!("   ❌ /v1/responses mapping — activate failed: {e}");
                        results.push((label, false, Some(e)));
                    }
                }
            }
            _ => {
                println!("   ⏭️  /v1/responses + model_mapping — skip");
                results.push((label, true, Some("SKIP".into())));
            }
        }
    }

    // 8.8 model_mapping：/v1/chat/completions
    {
        let label = "mapping_chat".to_string();
        match (env_opt("AG_SMOKE_MAPPING_CHAT_PROVIDER_ID"), env_opt("AG_SMOKE_MAPPING_CHAT_CLIENT_MODEL")) {
            (Some(pid), Some(model)) => {
                match ActiveProviderGuard::activate(&db, &pid) {
                    Ok(_g) => {
                        let url = format!("{}/v1/chat/completions", base);
                        let body = serde_json::json!({
                            "model": model,
                            "messages": [{"role": "user", "content": "Reply with 'ok'."}],
                            "stream": false,
                            "max_tokens": 16,
                            "temperature": 0.0,
                        });
                        let res = test_passthrough_with_mapping(&client, &token, &url, body, &db, "/v1/chat/completions").await;
                        let ok = res.is_ok();
                        match &res {
                            Ok(()) => println!("   ✅ /v1/chat/completions + model_mapping"),
                            Err(e) => println!("   ❌ /v1/chat/completions + model_mapping — {e}"),
                        }
                        results.push((label, ok, res.err()));
                    }
                    Err(e) => {
                        println!("   ❌ /v1/chat/completions mapping — activate failed: {e}");
                        results.push((label, false, Some(e)));
                    }
                }
            }
            _ => {
                println!("   ⏭️  /v1/chat/completions + model_mapping — skip");
                results.push((label, true, Some("SKIP".into())));
            }
        }
    }

    // 8.9 model_mapping：/v1/messages（Claude Code 打小米典型场景）
    {
        let label = "mapping_messages".to_string();
        match (env_opt("AG_SMOKE_MAPPING_MESSAGES_PROVIDER_ID"), env_opt("AG_SMOKE_MAPPING_MESSAGES_CLIENT_MODEL")) {
            (Some(pid), Some(model)) => {
                match ActiveProviderGuard::activate(&db, &pid) {
                    Ok(_g) => {
                        let url = format!("{}/v1/messages", base);
                        let body = serde_json::json!({
                            "model": model,
                            "max_tokens": 16,
                            "messages": [{"role": "user", "content": "Reply with 'ok'."}]
                        });
                        let res = test_passthrough_with_mapping(&client, &token, &url, body, &db, "/v1/messages").await;
                        let ok = res.is_ok();
                        match &res {
                            Ok(()) => println!("   ✅ /v1/messages + model_mapping"),
                            Err(e) => println!("   ❌ /v1/messages + model_mapping — {e}"),
                        }
                        results.push((label, ok, res.err()));
                    }
                    Err(e) => {
                        println!("   ❌ /v1/messages mapping — activate failed: {e}");
                        results.push((label, false, Some(e)));
                    }
                }
            }
            _ => {
                println!("   ⏭️  /v1/messages + model_mapping — skip");
                results.push((label, true, Some("SKIP".into())));
            }
        }
    }

    // Shutdown gateway
    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(5), server_handle).await;

    // Summary
    println!("\n📊 Smoke Test Summary");
    let passed = results.iter().filter(|(_, ok, _)| *ok).count();
    let failed = results.iter().filter(|(_, ok, _)| !*ok).count();
    println!("   Passed: {}  Failed: {}", passed, failed);
    for (name, ok, err) in &results {
        let icon = if *ok { "✅" } else { "❌" };
        if let Some(e) = err {
            println!("   {} {} — {}", icon, name, e);
        } else {
            println!("   {} {}", icon, name);
        }
    }

    assert_eq!(failed, 0, "{} smoke tests failed", failed);
    println!("\n🎉 All smoke tests passed. Ready to release.\n");
}

async fn test_chat_completions(
    client: &reqwest::Client,
    token: &str,
    url: &str,
) -> Result<String, String> {
    let body = serde_json::json!({
        "model": null,
        "messages": [{"role": "user", "content": "hi"}],
        "stream": false,
        "max_tokens": 5,
        "temperature": 0.0,
    });

    let resp = client
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read body: {e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;

    let model = json.get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let has_choices = json.get("choices").is_some();
    if !has_choices {
        return Err("missing choices in response".into());
    }

    Ok(model)
}

async fn test_responses_api(
    client: &reqwest::Client,
    token: &str,
    url: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "model": null,
        "input": "hi",
        "stream": false,
        "max_output_tokens": 5,
        "temperature": 0.0,
    });

    let resp = client
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read body: {e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;

    if json.get("id").is_none() {
        return Err("missing response id".into());
    }
    Ok(())
}

async fn test_responses_strict(
    client: &reqwest::Client,
    token: &str,
    url: &str,
) -> Result<String, String> {
    // Use array-form input — the realistic shape Codex sends. The string
    // shape is tested by the existing `test_responses_api`; this exercises
    // the array path + walks the output structure end-to-end.
    //
    // Shape contract (what the gateway controls) — strict:
    //   - status 2xx
    //   - response.id, object="response", status="completed", model present
    //   - output is an array
    // Content (what the model decides) — tolerant:
    //   - empty output / empty text is allowed; thinking-mode models can
    //     burn the entire token budget on reasoning and produce no text.
    //   - if output has a message item, its content must include an
    //     output_text block (validates our translation reshaping).
    let body = serde_json::json!({
        "model": null,
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": "Reply with the single word 'ok'."}]
        }],
        "stream": false,
        "max_output_tokens": 64,
        "temperature": 0.0,
    });

    let resp = client.post(url).bearer_auth(token).json(&body).send().await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read body: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;

    if json.get("id").is_none() {
        return Err("missing response id".into());
    }
    let object = json.get("object").and_then(|v| v.as_str()).unwrap_or("");
    if object != "response" {
        return Err(format!("expected object=response, got {object}"));
    }
    let status_field = json.get("status").and_then(|v| v.as_str()).unwrap_or("");
    if status_field != "completed" {
        return Err(format!("expected status=completed, got {status_field}"));
    }
    // Output array existence is required; emptiness is allowed.
    let output = json.get("output").and_then(|v| v.as_array())
        .ok_or_else(|| "missing output array".to_string())?;
    if let Some(msg) = output.iter().find(|o| o.get("type").and_then(|t| t.as_str()) == Some("message")) {
        let content = msg.get("content").and_then(|c| c.as_array())
            .ok_or_else(|| "message has no content array".to_string())?;
        // If a message item is present, our translator should have emitted
        // at least an empty output_text block (sometimes empty when the
        // upstream returned no choices).
        let _text_block = content.iter().find(|c| c.get("type").and_then(|t| t.as_str()) == Some("output_text"))
            .ok_or_else(|| "message present but no output_text content".to_string())?;
    }
    let model = json.get("model").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    Ok(model)
}

async fn test_anthropic_messages(
    client: &reqwest::Client,
    token: &str,
    url: &str,
) -> Result<(), String> {
    let body = serde_json::json!({
        "model": "claude-3-5-sonnet-latest",
        "max_tokens": 8,
        "messages": [
            {"role": "user", "content": "Reply with the single word 'ok'."}
        ]
    });
    let resp = client.post(url).bearer_auth(token).json(&body).send().await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read body: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;
    // Anthropic shape: {type: "message", content: [{type: "text", text: "..."}], ...}
    if json.get("type").and_then(|v| v.as_str()) != Some("message") {
        return Err(format!("expected type=message, got {:?}", json.get("type")));
    }
    let content = json.get("content").and_then(|v| v.as_array())
        .ok_or_else(|| "missing content array".to_string())?;
    if content.is_empty() {
        return Err("content array empty".into());
    }
    Ok(())
}

async fn test_responses_multi_turn(
    client: &reqwest::Client,
    token: &str,
    url: &str,
) -> Result<(), String> {
    // Long-enough opening prompt to clear the 64-char threshold in
    // session_affinity::derive_from_responses.
    let opening = "You are a helpful assistant in a smoke test harness. Reply concisely with the word 'one'.";

    // Turn 1
    let body1 = serde_json::json!({
        "model": null,
        "input": [{
            "type": "message", "role": "user",
            "content": [{"type": "input_text", "text": opening}],
        }],
        "stream": false,
        "max_output_tokens": 8,
        "temperature": 0.0,
    });
    let r1 = client.post(url).bearer_auth(token).json(&body1).send().await
        .map_err(|e| format!("turn1 send: {e}"))?;
    if !r1.status().is_success() {
        let t = r1.text().await.unwrap_or_default();
        return Err(format!("turn1 HTTP error: {}", snip(&t)));
    }

    // Turn 2 — replays the same opening to maximize prompt-cache hit
    // probability. Routing should prefer the same provider via affinity if
    // turn 1 surfaced cached_tokens > 0.
    let body2 = serde_json::json!({
        "model": null,
        "input": [
            {"type": "message", "role": "user",
             "content": [{"type": "input_text", "text": opening}]},
            {"type": "message", "role": "assistant",
             "content": [{"type": "output_text", "text": "one"}]},
            {"type": "message", "role": "user",
             "content": [{"type": "input_text", "text": "Now reply 'two'."}]},
        ],
        "stream": false,
        "max_output_tokens": 8,
        "temperature": 0.0,
    });
    let r2 = client.post(url).bearer_auth(token).json(&body2).send().await
        .map_err(|e| format!("turn2 send: {e}"))?;
    if !r2.status().is_success() {
        let t = r2.text().await.unwrap_or_default();
        return Err(format!("turn2 HTTP error: {}", snip(&t)));
    }
    Ok(())
}

async fn test_provider_direct(
    client: &reqwest::Client,
    provider: &agentgate_lib::models::provider::Provider,
) -> Result<(), String> {
    // Send a minimal chat completions request directly to the provider's base_url
    // using the provider's own API key and default model.
    let api_key = provider.api_key.as_ref()
        .filter(|k| !k.is_empty())
        .ok_or_else(|| "no api key".to_string())?;

    let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": provider.default_model,
        "messages": [{"role": "user", "content": "hi"}],
        "stream": false,
        "max_tokens": 5,
        "temperature": 0.0,
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read body: {e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;

    if json.get("choices").is_none() && json.get("candidates").is_none() {
        return Err("unexpected response shape".into());
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────
// 3-主路覆盖 helpers
// ─────────────────────────────────────────────────────────────────

/// POST json + 解码响应。错误片段过 sanitize_for_log。
async fn smoke_post_json(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    body: serde_json::Value,
) -> Result<(reqwest::StatusCode, String), String> {
    let resp = client.post(url).bearer_auth(token).json(&body).send().await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("read body: {e}"))?;
    Ok((status, text))
}

/// 主路 ① 协议转换：/v1/responses → Anthropic 上游。
/// 验 trace.protocol == "anthropic_messages"。
async fn test_responses_transform_anthropic(
    client: &reqwest::Client,
    token: &str,
    base: &str,
    model: &str,
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> Result<(), String> {
    let body = serde_json::json!({
        "model": model,
        "input": "Reply with 'ok'.",
        "stream": false,
        "max_output_tokens": 16,
        "temperature": 0.0,
    });
    let url = format!("{}/v1/responses", base);
    let (status, text) = smoke_post_json(client, token, &url, body).await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;
    if json.get("output").and_then(|v| v.as_array()).is_none() {
        return Err("missing output array".into());
    }
    let trace = fetch_recent_trace(db, "/v1/responses")
        .ok_or_else(|| "no recent trace".to_string())?;
    let protocol = trace.get("protocol").and_then(|p| p.as_str()).unwrap_or("");
    if protocol != "anthropic_messages" {
        return Err(format!("expected trace.protocol=anthropic_messages, got {protocol}"));
    }
    Ok(())
}

/// 主路 ① 协议转换：/v1/responses → Gemini 上游。
async fn test_responses_transform_gemini(
    client: &reqwest::Client,
    token: &str,
    base: &str,
    model: &str,
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> Result<(), String> {
    let body = serde_json::json!({
        "model": model,
        "input": "Reply with 'ok'.",
        "stream": false,
        "max_output_tokens": 16,
        "temperature": 0.0,
    });
    let url = format!("{}/v1/responses", base);
    let (status, text) = smoke_post_json(client, token, &url, body).await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }
    let trace = fetch_recent_trace(db, "/v1/responses")
        .ok_or_else(|| "no recent trace".to_string())?;
    let protocol = trace.get("protocol").and_then(|p| p.as_str()).unwrap_or("");
    if protocol != "gemini" {
        return Err(format!("expected trace.protocol=gemini, got {protocol}"));
    }
    Ok(())
}

/// 主路 ① 协议转换：/v1/chat/completions → Anthropic 上游（非流式）。
/// 验 trace.mode=transform + protocol=chat_to_anthropic。这是 v1.2.2 新加的路径。
async fn test_chat_transform_anthropic_non_stream(
    client: &reqwest::Client,
    token: &str,
    base: &str,
    model: &str,
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> Result<(), String> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Reply with 'ok'."}],
        "stream": false,
        "max_tokens": 16,
        "temperature": 0.0,
    });
    let url = format!("{}/v1/chat/completions", base);
    let (status, text) = smoke_post_json(client, token, &url, body).await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;
    if json.get("choices").and_then(|c| c.as_array()).is_none() {
        return Err("missing choices array".into());
    }
    let trace = fetch_recent_trace(db, "/v1/chat/completions")
        .ok_or_else(|| "no recent trace".to_string())?;
    let mode = trace.get("mode").and_then(|m| m.as_str()).unwrap_or("");
    let protocol = trace.get("protocol").and_then(|p| p.as_str()).unwrap_or("");
    if mode != "transform" || protocol != "chat_to_anthropic" {
        return Err(format!("expected transform/chat_to_anthropic, got {mode}/{protocol}"));
    }
    Ok(())
}

/// 主路 ① 协议转换：/v1/chat/completions → Anthropic 上游（流式）。
/// 校验 SSE 形态完整 + trace.stream=true。
async fn test_chat_transform_anthropic_stream(
    client: &reqwest::Client,
    token: &str,
    base: &str,
    model: &str,
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> Result<(), String> {
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": "Reply with 'ok'."}],
        "stream": true,
        "max_tokens": 16,
        "temperature": 0.0,
    });
    let url = format!("{}/v1/chat/completions", base);
    let resp = client.post(&url).bearer_auth(token).json(&body).send().await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {}", snip(&t)));
    }
    let text = resp.text().await.map_err(|e| format!("read sse: {e}"))?;
    if !text.contains("data: {") {
        return Err("no data chunk in SSE stream".into());
    }
    if !text.contains("data: [DONE]") {
        return Err("no [DONE] terminator".into());
    }
    let trace = fetch_recent_trace(db, "/v1/chat/completions")
        .ok_or_else(|| "no recent trace".to_string())?;
    let mode = trace.get("mode").and_then(|m| m.as_str()).unwrap_or("");
    let protocol = trace.get("protocol").and_then(|p| p.as_str()).unwrap_or("");
    let stream = trace.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);
    if mode != "transform" || protocol != "chat_to_anthropic" || !stream {
        return Err(format!("expected transform/chat_to_anthropic/stream=true, got {mode}/{protocol}/{stream}"));
    }
    Ok(())
}

/// 主路 ① 协议转换：/v1/messages → Chat 上游 fallback。
async fn test_messages_transform_chat_fallback(
    client: &reqwest::Client,
    token: &str,
    base: &str,
    model: &str,
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> Result<(), String> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 16,
        "messages": [{"role": "user", "content": "Reply with 'ok'."}]
    });
    let url = format!("{}/v1/messages", base);
    let (status, text) = smoke_post_json(client, token, &url, body).await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;
    if json.get("content").and_then(|c| c.as_array()).is_none() {
        return Err("missing content array".into());
    }
    let trace = fetch_recent_trace(db, "/v1/messages")
        .ok_or_else(|| "no recent trace".to_string())?;
    let mode = trace.get("mode").and_then(|m| m.as_str()).unwrap_or("");
    let protocol = trace.get("protocol").and_then(|p| p.as_str()).unwrap_or("");
    if mode != "transform" || protocol != "anthropic_messages" {
        return Err(format!("expected transform/anthropic_messages, got {mode}/{protocol}"));
    }
    Ok(())
}

/// 主路 ① 协议转换：/v1beta/models/<m>:generateContent → Chat 上游。
async fn test_gemini_input_transform_chat(
    client: &reqwest::Client,
    token: &str,
    base: &str,
    model: &str,
    db: &Arc<Mutex<rusqlite::Connection>>,
) -> Result<(), String> {
    let body = serde_json::json!({
        "contents": [{"role": "user", "parts": [{"text": "Reply with 'ok'."}]}],
        "generationConfig": {"maxOutputTokens": 16, "temperature": 0.0}
    });
    let url = format!("{}/v1beta/models/{}:generateContent", base, model);
    let (status, text) = smoke_post_json(client, token, &url, body).await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }
    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;
    if json.get("candidates").and_then(|c| c.as_array()).is_none() {
        return Err("missing candidates array".into());
    }
    let trace = fetch_recent_trace(db, "/v1beta/generateContent")
        .ok_or_else(|| "no recent trace".to_string())?;
    let protocol = trace.get("protocol").and_then(|p| p.as_str()).unwrap_or("");
    if protocol != "gemini_input" {
        return Err(format!("expected trace.protocol=gemini_input, got {protocol}"));
    }
    Ok(())
}

/// 主路 ② CC 转发直连 / model_mapping 命中：验 trace.mode == native_pass_through_model_mapping。
/// 通用 helper：3 个入口（responses / chat / messages）共用。
async fn test_passthrough_with_mapping(
    client: &reqwest::Client,
    token: &str,
    url: &str,
    body: serde_json::Value,
    db: &Arc<Mutex<rusqlite::Connection>>,
    route: &str,
) -> Result<(), String> {
    let (status, text) = smoke_post_json(client, token, url, body).await?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", snip(&text)));
    }
    let trace = fetch_recent_trace(db, route)
        .ok_or_else(|| "no recent trace".to_string())?;
    let mode = trace.get("mode").and_then(|m| m.as_str()).unwrap_or("");
    if mode != "native_pass_through_model_mapping" {
        return Err(format!("expected trace.mode=native_pass_through_model_mapping, got '{mode}'"));
    }
    Ok(())
}
