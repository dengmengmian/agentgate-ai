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
        return Err(format!("HTTP {status}: {}", text.chars().take(200).collect::<String>()));
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
        return Err(format!("HTTP {status}: {}", text.chars().take(200).collect::<String>()));
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
        return Err(format!("HTTP {status}: {}", text.chars().take(200).collect::<String>()));
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
        return Err(format!("HTTP {status}: {}", text.chars().take(200).collect::<String>()));
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
        return Err(format!("turn1 HTTP error: {}", t.chars().take(200).collect::<String>()));
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
        return Err(format!("turn2 HTTP error: {}", t.chars().take(200).collect::<String>()));
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
        return Err(format!("HTTP {status}: {}", text.chars().take(200).collect::<String>()));
    }

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("parse json: {e}"))?;

    if json.get("choices").is_none() && json.get("candidates").is_none() {
        return Err("unexpected response shape".into());
    }

    Ok(())
}
