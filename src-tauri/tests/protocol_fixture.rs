//! L1 protocol conversion + L2 model-mapping regression tests.
//!
//! Mirrors the env-gated 8.1–8.9 block in `smoke_test.rs` but runs fully
//! offline against a wiremock upstream. The real smoke test stays as the
//! "true integration" verification — these are the CI-blockers that catch
//! regressions to the transform/mapping plumbing without needing keys.

mod common;

use common::gateway_harness::{GatewayHarness, ProviderSpec};
use common::mock_upstream::MockUpstream;
use serde_json::json;

// ── L1: protocol conversion ─────────────────────────────────────────────

#[tokio::test]
async fn l1_responses_to_anthropic_transform() {
    // Codex (/v1/responses) → Anthropic-typed provider with anthropic_base_url.
    // The gateway must convert to Anthropic Messages shape and hit /v1/messages.
    let mock = MockUpstream::start().await;
    mock.stub_anthropic_messages_ok("claude-sonnet-4-6", "ok")
        .await;

    let mut spec = ProviderSpec::chat_only("anthropic", "claude-sonnet-4-6");
    spec.protocol = r#"["anthropic_messages"]"#.to_string();
    spec.anthropic_base_url = Some(mock.url());

    let harness = GatewayHarness::start(spec, &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "claude-sonnet-4-6",
            "input": "ping",
            "stream": false,
            "max_output_tokens": 16,
        }))
        .send()
        .await
        .expect("send /v1/responses");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].path, "/v1/messages",
        "must hit Anthropic Messages path"
    );
    // Anthropic shape requires a `messages` array, not Responses-style `input`.
    assert!(
        received[0].body.get("messages").is_some(),
        "upstream should receive Anthropic-shaped messages array: {}",
        received[0].body_raw
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn l1_chat_to_anthropic_non_stream_transform() {
    // Generic Chat client → Anthropic-typed provider with anthropic_base_url.
    // Goes through client_chat_to_anthropic_handle.
    let mock = MockUpstream::start().await;
    mock.stub_anthropic_messages_ok("claude-sonnet-4-6", "ok")
        .await;

    let mut spec = ProviderSpec::chat_only("anthropic", "claude-sonnet-4-6");
    spec.protocol = r#"["anthropic_messages"]"#.to_string();
    spec.anthropic_base_url = Some(mock.url());

    let harness = GatewayHarness::start(spec, &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/chat/completions"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "claude-sonnet-4-6",
            "messages": [{ "role": "user", "content": "ping" }],
            "stream": false,
            "max_tokens": 16,
        }))
        .send()
        .await
        .expect("send /v1/chat/completions");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].path, "/v1/messages");
    assert!(
        received[0].body.get("messages").is_some(),
        "upstream should receive Anthropic-shaped body"
    );
    // Chat → Anthropic should also forward an explicit max_tokens.
    assert!(
        received[0].body.get("max_tokens").is_some(),
        "max_tokens should pass through to Anthropic upstream"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn l1_messages_to_chat_fallback_transform() {
    // Claude Code (/v1/messages) → Chat-only provider (no anthropic_base_url).
    // Gateway must fall back to the Messages → Chat translator and hit
    // /v1/chat/completions upstream with a Chat-shaped body.
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("custom-model", "ok").await;

    let harness =
        GatewayHarness::start(ProviderSpec::chat_only("custom", "custom-model"), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/messages"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "custom-model",
            "max_tokens": 16,
            "messages": [{ "role": "user", "content": "ping" }]
        }))
        .send()
        .await
        .expect("send /v1/messages");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].path, "/v1/chat/completions");
    let messages = received[0].body["messages"].as_array().expect("messages");
    assert!(!messages.is_empty());
    // Anthropic input uses `max_tokens` at the top level; Chat uses the same name
    // — the assertion that matters is the path + flat content shape.

    harness.shutdown().await;
}

// ── L3: vision-aware failover routing ───────────────────────────────────
//
// Regression for the asymmetry where /v1/chat/completions and /v1/messages
// did NOT skip vision-incapable providers for image requests (only
// /v1/responses did). An image request must route to the vision-capable
// candidate and never touch the non-vision primary.

#[tokio::test]
async fn messages_image_routes_to_vision_provider() {
    // Primary (non-vision) + vision candidate, failover mode. An image-bearing
    // /v1/messages request must land on the vision provider's upstream.
    let primary_mock = MockUpstream::start().await;
    primary_mock
        .stub_chat_completions_ok("novis-model", "ok")
        .await;
    let vision_mock = MockUpstream::start().await;
    vision_mock
        .stub_chat_completions_ok("vis-model", "ok")
        .await;

    let harness = GatewayHarness::start(
        ProviderSpec::chat_only("custom", "novis-model"),
        &primary_mock,
    )
    .await;
    harness.add_vision_failover_candidate(&vision_mock, "vis-model");
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/messages"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "novis-model",
            "max_tokens": 16,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "describe this" },
                    { "type": "image", "source": { "type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo=" } }
                ]
            }]
        }))
        .send()
        .await
        .expect("send /v1/messages");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let vision_hits = vision_mock.received().await;
    let primary_hits = primary_mock.received().await;
    assert_eq!(
        vision_hits.len(),
        1,
        "image request must reach the vision provider"
    );
    assert_eq!(
        primary_hits.len(),
        0,
        "non-vision primary must be skipped for image requests"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn chat_completions_image_routes_to_vision_provider() {
    // Same guarantee on the /v1/chat/completions entry.
    let primary_mock = MockUpstream::start().await;
    primary_mock
        .stub_chat_completions_ok("novis-model", "ok")
        .await;
    let vision_mock = MockUpstream::start().await;
    vision_mock
        .stub_chat_completions_ok("vis-model", "ok")
        .await;

    let harness = GatewayHarness::start(
        ProviderSpec::chat_only("custom", "novis-model"),
        &primary_mock,
    )
    .await;
    harness.add_vision_failover_candidate(&vision_mock, "vis-model");
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/chat/completions"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "novis-model",
            "max_tokens": 16,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "describe this" },
                    { "type": "image_url", "image_url": { "url": "data:image/png;base64,iVBORw0KGgo=" } }
                ]
            }]
        }))
        .send()
        .await
        .expect("send /v1/chat/completions");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let vision_hits = vision_mock.received().await;
    let primary_hits = primary_mock.received().await;
    assert_eq!(
        vision_hits.len(),
        1,
        "image request must reach the vision provider"
    );
    assert_eq!(
        primary_hits.len(),
        0,
        "non-vision primary must be skipped for image requests"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn messages_text_only_stays_on_primary() {
    // Control: a text-only /v1/messages request must NOT be rerouted — it stays
    // on the primary even though a vision candidate exists.
    let primary_mock = MockUpstream::start().await;
    primary_mock
        .stub_chat_completions_ok("novis-model", "ok")
        .await;
    let vision_mock = MockUpstream::start().await;
    vision_mock
        .stub_chat_completions_ok("vis-model", "ok")
        .await;

    let harness = GatewayHarness::start(
        ProviderSpec::chat_only("custom", "novis-model"),
        &primary_mock,
    )
    .await;
    harness.add_vision_failover_candidate(&vision_mock, "vis-model");
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/messages"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "novis-model",
            "max_tokens": 16,
            "messages": [{ "role": "user", "content": "just text" }]
        }))
        .send()
        .await
        .expect("send /v1/messages");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let primary_hits = primary_mock.received().await;
    assert_eq!(
        primary_hits.len(),
        1,
        "text-only request must stay on the primary provider"
    );

    harness.shutdown().await;
}

// ── L2: model mapping + agentgate virtual model ─────────────────────────

#[tokio::test]
async fn l2_responses_endpoint_applies_model_mapping() {
    // Codex sends model="gpt-5", provider mapping rewrites to "deepseek-v4-pro".
    // The mock upstream must observe the post-mapping name.
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("deepseek-v4-pro", "ok").await;

    let spec = ProviderSpec::chat_only("custom", "deepseek-v4-pro")
        .with_mapping(r#"{"gpt-5":"deepseek-v4-pro"}"#);

    let harness = GatewayHarness::start(spec, &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "gpt-5",
            "input": "ping",
            "stream": false,
            "max_output_tokens": 16,
        }))
        .send()
        .await
        .expect("send /v1/responses");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].body["model"], "deepseek-v4-pro");

    harness.shutdown().await;
}

#[tokio::test]
async fn l2_chat_endpoint_applies_model_mapping() {
    // Generic Chat client sends gpt-5, provider rewrites to deepseek-v4-pro.
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("deepseek-v4-pro", "ok").await;

    let spec = ProviderSpec::chat_only("custom", "deepseek-v4-pro")
        .with_mapping(r#"{"gpt-5":"deepseek-v4-pro"}"#);

    let harness = GatewayHarness::start(spec, &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/chat/completions"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "gpt-5",
            "messages": [{ "role": "user", "content": "ping" }],
            "stream": false,
            "max_tokens": 16,
        }))
        .send()
        .await
        .expect("send /v1/chat/completions");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].body["model"], "deepseek-v4-pro");

    harness.shutdown().await;
}

#[tokio::test]
async fn l2_messages_endpoint_applies_model_mapping() {
    // Claude Code sends claude-sonnet-4-6, provider rewrites to deepseek-v4-pro
    // on the Anthropic passthrough path. The mock /v1/messages must observe
    // the post-mapping name (and not a [1m] qualifier we didn't ask for).
    let mock = MockUpstream::start().await;
    mock.stub_anthropic_messages_ok("deepseek-v4-pro", "ok")
        .await;

    let spec = ProviderSpec::chat_only("deepseek", "deepseek-v4-pro")
        .with_anthropic(mock.url())
        .with_mapping(r#"{"claude-sonnet-4-6":"deepseek-v4-pro"}"#);

    let harness = GatewayHarness::start(spec, &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/messages"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 16,
            "messages": [{ "role": "user", "content": "ping" }]
        }))
        .send()
        .await
        .expect("send /v1/messages");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].path, "/v1/messages");
    assert_eq!(received[0].body["model"], "deepseek-v4-pro");

    harness.shutdown().await;
}

#[tokio::test]
async fn l2_agentgate_virtual_model_resolves_to_real_model() {
    // Per 1.2.4: AtomCode/OpenCode write `agentgate` as the client model so
    // the gateway can pick whichever real model the active route lands on.
    // On a Chat pass-through, the upstream must see the resolved real model,
    // not the literal "agentgate".
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("real-model-v1", "ok").await;

    let spec = ProviderSpec::chat_only("custom", "real-model-v1");
    let harness = GatewayHarness::start(spec, &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/chat/completions"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "agentgate",
            "messages": [{ "role": "user", "content": "ping" }],
            "stream": false,
            "max_tokens": 16,
        }))
        .send()
        .await
        .expect("send /v1/chat/completions");
    assert!(
        res.status().is_success(),
        "gateway returned {}",
        res.status()
    );

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].body["model"], "real-model-v1",
        "virtual `agentgate` should resolve to the provider's default model"
    );

    harness.shutdown().await;
}

// ── L4: error-triggered failover ────────────────────────────────────────
//
// 故障自愈的核心承诺:主 provider 返回 5xx 时自动切到下一个候选,客户端无感。
// 此前集成层从未验证过——只有能力路由(vision)的 e2e。这里 stub 主上游 500、
// 次上游 200,断言两个上游都被打到且客户端拿到 200。

#[tokio::test]
async fn responses_failover_on_upstream_500() {
    let primary = MockUpstream::start().await;
    primary
        .stub_chat_completions_err(500, json!({"error": {"message": "boom"}}))
        .await;
    let secondary = MockUpstream::start().await;
    secondary
        .stub_chat_completions_ok("backup-model", "recovered")
        .await;

    let harness =
        GatewayHarness::start(ProviderSpec::chat_only("custom", "primary-model"), &primary).await;
    harness.add_failover_candidate(&secondary, "backup-model");
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({ "model": "primary-model", "input": "hi" }))
        .send()
        .await
        .expect("send /v1/responses");

    assert!(
        res.status().is_success(),
        "client should get 200 after failover, got {}",
        res.status()
    );
    // primary 被打多次是 adapter 对 5xx 的内部重试(MAX_RETRIES),重试耗尽后才
    // failover——这里只断言"主被尝试过"+"failover 真的切到了次"。
    assert!(
        !primary.received().await.is_empty(),
        "primary must be tried first"
    );
    assert!(
        !secondary.received().await.is_empty(),
        "failover must reach the secondary after primary's 500"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn responses_all_providers_fail_returns_error() {
    // 全部候选都 500 → 候选耗尽 → 客户端拿到错误(非 2xx),不静默假成功。
    let primary = MockUpstream::start().await;
    primary
        .stub_chat_completions_err(500, json!({"error": {"message": "boom1"}}))
        .await;
    let secondary = MockUpstream::start().await;
    secondary
        .stub_chat_completions_err(500, json!({"error": {"message": "boom2"}}))
        .await;

    let harness =
        GatewayHarness::start(ProviderSpec::chat_only("custom", "primary-model"), &primary).await;
    harness.add_failover_candidate(&secondary, "backup-model");
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({ "model": "primary-model", "input": "hi" }))
        .send()
        .await
        .expect("send /v1/responses");

    assert!(
        !res.status().is_success(),
        "exhausted failover must surface an error, got {}",
        res.status()
    );
    assert!(!primary.received().await.is_empty());
    assert!(
        !secondary.received().await.is_empty(),
        "both candidates must be attempted before giving up"
    );

    harness.shutdown().await;
}

// ── L5: streaming conversion (chat SSE → Responses events) ───────────────
//
// 日常最高频的流式链路此前只有 codex_compact 一条专路有 wire 级 e2e。这里走
// 常规 /v1/responses 流式:上游吐 chat SSE(含 CJK)→ 网关转 Responses 事件 →
// 断言客户端拿到合法事件流且中文内容完整(不被转换破坏)。

#[tokio::test]
async fn responses_stream_converts_chat_sse_to_events() {
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_sse("stream-model").await;

    let harness =
        GatewayHarness::start(ProviderSpec::chat_only("custom", "stream-model"), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({ "model": "stream-model", "input": "hi", "stream": true }))
        .send()
        .await
        .expect("send streaming /v1/responses");

    assert!(res.status().is_success(), "stream status {}", res.status());
    let body = res.text().await.expect("read stream body");

    // 客户端应收到 Responses SSE 事件,且 CJK 内容完整拼回。
    assert!(
        body.contains("response.") || body.contains("output_text"),
        "expected Responses event frames, got: {}",
        &body[..body.len().min(300)]
    );
    assert!(
        body.contains("你好") && body.contains("世界"),
        "CJK content must survive SSE conversion intact"
    );

    harness.shutdown().await;
}

// ── L6: Gemini route (generateContent → chat conversion) ─────────────────

#[tokio::test]
async fn gemini_generate_converts_to_chat() {
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("gemini-backed", "hello from chat")
        .await;

    let harness =
        GatewayHarness::start(ProviderSpec::chat_only("custom", "gemini-backed"), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1beta/models/gemini-backed:generateContent"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "contents": [{ "role": "user", "parts": [{ "text": "hi" }] }]
        }))
        .send()
        .await
        .expect("send gemini generateContent");

    assert!(
        res.status().is_success(),
        "gemini route status {}",
        res.status()
    );
    let received = mock.received().await;
    assert_eq!(
        received.len(),
        1,
        "gemini request must reach the chat upstream"
    );
    assert!(
        received[0].body.get("messages").is_some(),
        "Gemini contents must be converted to chat messages"
    );

    harness.shutdown().await;
}
