//! L3 capability-layer tests for Kimi (Moonshot).
//!
//! Kimi's distinctive L3 quirks: `web_search` builtin maps to the
//! `$web_search` builtin_function (not Anthropic's or MiMo's shape) and
//! co-presence of `$web_search` forces `thinking: disabled` upstream.
//! Multi-turn tool roundtrips also exercise the generic Chat ↔ Responses
//! tool-call plumbing on the Kimi provider type.

mod common;

use common::gateway_harness::{GatewayHarness, ProviderSpec};
use common::mock_upstream::MockUpstream;
use serde_json::json;

fn kimi_provider() -> ProviderSpec {
    ProviderSpec::chat_only("kimi", "kimi-k2")
}

#[tokio::test]
async fn kimi_web_search_rewrites_to_builtin_and_disables_thinking() {
    // Codex emits `{"type": "web_search"}`. Kimi only accepts its own
    // `builtin_function/$web_search` shape, and crashes with $web_search
    // alongside `thinking: enabled`. The gateway must translate the tool
    // shape AND force-disable thinking in the same hop.
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("kimi-k2", "ok").await;

    let harness = GatewayHarness::start(kimi_provider(), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "kimi-k2",
            "input": "Look up the latest AgentGate release.",
            "stream": false,
            "max_output_tokens": 32,
            "tools": [{ "type": "web_search" }],
            "reasoning": { "effort": "high" }
        }))
        .send()
        .await
        .expect("send /v1/responses");
    assert!(res.status().is_success(), "gateway returned {}", res.status());

    let received = mock.received().await;
    assert_eq!(received.len(), 1);

    let tools = received[0].body["tools"]
        .as_array()
        .expect("tools should be present");
    let has_builtin = tools.iter().any(|t| {
        t.get("type").and_then(|x| x.as_str()) == Some("builtin_function")
            && t.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                == Some("$web_search")
    });
    assert!(
        has_builtin,
        "web_search should be rewritten to builtin_function/$web_search: {tools:?}"
    );

    // thinking should be forced off when $web_search is present.
    let thinking = received[0].body.get("thinking");
    let thinking_type = thinking
        .and_then(|t| t.get("type"))
        .and_then(|t| t.as_str());
    assert_eq!(
        thinking_type,
        Some("disabled"),
        "thinking must be disabled when $web_search present, got {thinking:?}"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn kimi_multi_turn_tool_call_roundtrip() {
    // Conversation history with a prior function_call + function_call_output
    // pair. Verifies Codex's Responses-shape history flattens to a clean
    // assistant.tool_calls → tool message pair on the upstream Chat side,
    // with matching ids. This is the path that turns into "model can see
    // its previous tool call" upstream — if the order or id linking breaks,
    // the model loses context.
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("kimi-k2", "Sunny, 22°C.").await;

    let harness = GatewayHarness::start(kimi_provider(), &mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "kimi-k2",
            "input": [
                { "type": "message", "role": "user",
                  "content": [{ "type": "input_text", "text": "weather in Beijing?" }] },
                { "type": "function_call",
                  "id": "fc_abc123",
                  "call_id": "call_abc123",
                  "name": "get_weather",
                  "arguments": "{\"city\":\"Beijing\"}" },
                { "type": "function_call_output",
                  "call_id": "call_abc123",
                  "output": "{\"temp\":22,\"sky\":\"sunny\"}" },
                { "type": "message", "role": "user",
                  "content": [{ "type": "input_text", "text": "summarize" }] }
            ],
            "stream": false,
            "max_output_tokens": 32,
            "tools": [{
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {
                    "type": "object",
                    "properties": { "city": { "type": "string" } },
                    "required": ["city"]
                }
            }]
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

    // Find the assistant message that carries tool_calls.
    let assistant_with_tools = messages.iter().find(|m| {
        m.get("role").and_then(|r| r.as_str()) == Some("assistant")
            && m.get("tool_calls").and_then(|t| t.as_array()).is_some()
    });
    let assistant = assistant_with_tools
        .expect("assistant message with tool_calls should be present in history");
    let tool_calls = assistant["tool_calls"].as_array().unwrap();
    assert_eq!(tool_calls.len(), 1);
    let tool_call = &tool_calls[0];
    let call_id = tool_call
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert_eq!(call_id, "call_abc123");
    assert_eq!(
        tool_call["function"]["name"].as_str(),
        Some("get_weather"),
        "function name should pass through unchanged"
    );

    // The tool response message must reference the same call id and follow
    // the assistant turn, so the model can match call → result.
    let tool_msg = messages
        .iter()
        .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("tool"))
        .expect("tool result message");
    assert_eq!(
        tool_msg.get("tool_call_id").and_then(|v| v.as_str()),
        Some("call_abc123"),
        "tool result must link to the original function call id"
    );

    harness.shutdown().await;
}
