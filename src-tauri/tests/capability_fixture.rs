//! Offline capability-layer regression tests.
//!
//! Each test wires up a [`MockUpstream`] + [`GatewayHarness`], drives the
//! gateway via real HTTP, and asserts on what the mock upstream actually
//! received. No real provider keys or network involved — these run in CI
//! on every PR.
//!
//! L3 provider-specific cases (MiMo vision swap, DeepSeek image strip,
//! Kimi tools, …) land here in later tasks. This file currently carries
//! the harness sanity test only.

mod common;

use common::gateway_harness::{GatewayHarness, ProviderSpec};
use common::mock_upstream::MockUpstream;
use serde_json::json;

#[tokio::test]
async fn harness_chat_completions_passthrough_roundtrip() {
    // Sanity check: prove the mock + gateway wiring works end-to-end on a
    // boring chat-completions pass-through. Provider-specific L3 cases
    // build on this same scaffold.
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("test-model", "ok").await;

    let harness =
        GatewayHarness::start(ProviderSpec::chat_only("custom", "test-model"), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/chat/completions"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "test-model",
            "messages": [{ "role": "user", "content": "ping" }],
            "stream": false,
            "max_tokens": 8,
        }))
        .send()
        .await
        .expect("send request");
    let status = res.status();
    let body_text = res.text().await.unwrap_or_default();
    assert!(
        status.is_success(),
        "gateway returned {status} body={body_text}"
    );
    let body: serde_json::Value = serde_json::from_str(&body_text).expect("parse response");
    assert_eq!(body["choices"][0]["message"]["content"], "ok");

    let received = mock.received().await;
    assert_eq!(
        received.len(),
        1,
        "mock should have seen exactly one request"
    );
    assert_eq!(received[0].path, "/v1/chat/completions");
    assert_eq!(received[0].body["model"], "test-model");
    assert_eq!(received[0].body["messages"][0]["content"], "ping");

    harness.shutdown().await;
}
