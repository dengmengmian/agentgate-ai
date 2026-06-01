//! L3 capability-layer tests for MiMo.
//!
//! Each test drives `/v1/responses` (Codex's protocol) through a mock MiMo
//! upstream. That endpoint takes the full Responses → Chat transform path,
//! which is the only path where MiMo-specific transforms fire: vision-aware
//! model promotion, image stripping with notice, web_search auto-degrade,
//! and reasoning_content placeholder injection.

mod common;

use common::gateway_harness::{GatewayHarness, ProviderSpec};
use common::mock_upstream::MockUpstream;
use serde_json::json;

/// Capability matrix that mirrors a realistic MiMo deployment: pro has
/// reasoning + tools but no vision; non-pro has vision; omni has vision too.
fn mimo_matrix() -> &'static str {
    r#"{
        "mimo-v2.5-pro": ["text","tools","reasoning","web_search"],
        "mimo-v2.5": ["text","vision","tools","reasoning","web_search"],
        "mimo-v2-omni": ["text","vision","tools"]
    }"#
}

fn mimo_provider() -> ProviderSpec {
    ProviderSpec::chat_only("mimo", "mimo-v2.5-pro")
        .with_capabilities(mimo_matrix())
        .with_api_key("sk-test-payg-key")
}

#[tokio::test]
async fn mimo_vision_promotion_swaps_to_capable_sibling() {
    // Codex sends an image to mimo-v2.5-pro (no vision). The provider matrix
    // declares mimo-v2.5 as vision-capable. The gateway must swap the model
    // before forwarding, so the upstream sees `mimo-v2.5`, not `mimo-v2.5-pro`.
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("mimo-v2.5", "ok").await;

    let harness = GatewayHarness::start(mimo_provider(), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "mimo-v2.5-pro",
            "input": [{
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "describe this image" },
                    { "type": "input_image", "image_url": "data:image/png;base64,iVBORw0KGgo=" }
                ]
            }],
            "stream": false,
            "max_output_tokens": 16,
        }))
        .send()
        .await
        .expect("send /v1/responses");
    assert!(res.status().is_success(), "gateway returned {}", res.status());

    let received = mock.received().await;
    assert_eq!(received.len(), 1, "expected exactly one upstream call");
    assert_eq!(
        received[0].body["model"], "mimo-v2.5",
        "gateway should have promoted mimo-v2.5-pro → mimo-v2.5 for vision request"
    );
    // The image content must survive the swap.
    let user_content = &received[0].body["messages"][0]["content"];
    let has_image = user_content
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url"))
        })
        .unwrap_or(false);
    assert!(has_image, "image content should survive promotion");

    harness.shutdown().await;
}

#[tokio::test]
async fn mimo_strips_image_with_notice_when_no_vision_sibling() {
    // Same vision-on-non-vision request, but the matrix now lists ONLY pro
    // (no sibling that has vision). The gateway can't promote, so it must
    // fall back to stripping the image and appending a recovery notice
    // so the model knows why the picture is gone.
    let matrix = r#"{ "mimo-v2.5-pro": ["text","tools","reasoning","web_search"] }"#;
    let spec = ProviderSpec::chat_only("mimo", "mimo-v2.5-pro").with_capabilities(matrix);

    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("mimo-v2.5-pro", "ok").await;

    let harness = GatewayHarness::start(spec, &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "mimo-v2.5-pro",
            "input": [{
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "what is this" },
                    { "type": "input_image", "image_url": "data:image/png;base64,iVBORw0KGgo=" }
                ]
            }],
            "stream": false,
            "max_output_tokens": 16,
        }))
        .send()
        .await
        .expect("send /v1/responses");
    assert!(res.status().is_success(), "gateway returned {}", res.status());

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].body["model"], "mimo-v2.5-pro");
    let parts = received[0].body["messages"][0]["content"]
        .as_array()
        .expect("content array");
    assert!(
        parts
            .iter()
            .all(|p| p.get("type").and_then(|t| t.as_str()) != Some("image_url")),
        "image_url part should have been stripped: {parts:?}"
    );
    let text_blob = parts
        .iter()
        .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text_blob.contains("image stripped") && text_blob.contains("mimo-v2.5-pro"),
        "stripped-image notice should mention MiMo model: {text_blob:?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn mimo_web_search_auto_degrades_on_plugin_unavailable() {
    // PAYG (`sk-*`) MiMo account hits the web_search builtin while the
    // Web Search Plugin isn't activated. Upstream 400s with the canonical
    // marker. Gateway must strip web_search from the request and retry
    // once; the second attempt succeeds.
    let mock = MockUpstream::start().await;
    mock.stub_mimo_web_search_unavailable_then_ok("mimo-v2.5", "ok")
        .await;

    let harness = GatewayHarness::start(mimo_provider(), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "mimo-v2.5",
            "input": "Search the web for AgentGate.",
            "stream": false,
            "max_output_tokens": 16,
            "tools": [{ "type": "web_search" }],
        }))
        .send()
        .await
        .expect("send /v1/responses");
    assert!(res.status().is_success(), "gateway returned {}", res.status());

    let received = mock.received().await;
    assert_eq!(
        received.len(),
        2,
        "expected 2 upstream calls (1st fails, 2nd retries without web_search)"
    );

    // First attempt should carry the web_search builtin.
    let first_tools = &received[0].body["tools"];
    let first_has_ws = first_tools
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|t| t.get("type").and_then(|x| x.as_str()) == Some("web_search"))
        })
        .unwrap_or(false);
    assert!(first_has_ws, "first attempt must include web_search builtin");

    // Second attempt should have stripped web_search.
    let second_tools = &received[1].body["tools"];
    let second_has_ws = second_tools
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|t| t.get("type").and_then(|x| x.as_str()) == Some("web_search"))
        })
        .unwrap_or(false);
    assert!(
        !second_has_ws,
        "retry must have stripped web_search builtin: {second_tools:?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn mimo_thinking_mode_injects_reasoning_placeholder_for_history() {
    // Multi-turn conversation where a prior assistant turn ran without
    // thinking mode (no reasoning_content). MiMo thinking-mode requires
    // EVERY history assistant to carry reasoning_content or upstream 400s.
    // The gateway must inject the canonical placeholder string into any
    // assistant message that lacks it.
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("mimo-v2.5", "ok").await;

    let harness = GatewayHarness::start(mimo_provider(), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "mimo-v2.5",
            "input": [
                { "type": "message", "role": "user",
                  "content": [{ "type": "input_text", "text": "first question" }] },
                { "type": "message", "role": "assistant",
                  "content": [{ "type": "output_text", "text": "first answer" }] },
                { "type": "message", "role": "user",
                  "content": [{ "type": "input_text", "text": "second question" }] }
            ],
            "stream": false,
            "max_output_tokens": 16,
            "reasoning": { "effort": "medium" }
        }))
        .send()
        .await
        .expect("send /v1/responses");
    assert!(res.status().is_success(), "gateway returned {}", res.status());

    let received = mock.received().await;
    assert_eq!(received.len(), 1);
    let messages = received[0].body["messages"]
        .as_array()
        .expect("messages array");
    let assistant = messages
        .iter()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
        .expect("assistant message in history");
    let reasoning = assistant
        .get("reasoning_content")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        !reasoning.is_empty(),
        "history assistant should have a reasoning_content placeholder, got: {assistant:?}"
    );

    harness.shutdown().await;
}
