//! GitHub Copilot 上游接入的离线回归测试。
//!
//! 1. token 交换:wiremock 模拟 GitHub `/copilot_internal/v2/token`,
//!    验证请求头、成功解析、401/403/5xx 错误映射。
//! 2. 网关全链路:Claude Code → /v1/messages → copilot pass-through,
//!    验证 Bearer 替换、x-initiator 计费分类、Copilot 必带请求头、
//!    模型 dash→dot 归一化、token 进程内缓存;以及 Codex → /v1/responses
//!    → chat 转换路径的 Bearer 替换。

mod common;

use agentgate_lib::providers::copilot;
use common::gateway_harness::{GatewayHarness, ProviderSpec};
use common::mock_upstream::MockUpstream;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── token 交换 ──────────────────────────────────────────

#[tokio::test]
async fn copilot_token_exchange_ok_sends_required_headers() {
    let server = MockServer::start().await;
    // header matcher 兼断言:缺任何一个必带 header 都不会命中 stub → 404 → 测试失败
    Mock::given(method("GET"))
        .and(path("/copilot_internal/v2/token"))
        .and(header("authorization", "token gho_exchange_ok"))
        .and(header("copilot-integration-id", "vscode-chat"))
        .and(header("editor-version", "vscode/1.110.1"))
        .and(header("editor-plugin-version", "copilot-chat/0.38.2"))
        .and(header("user-agent", "GitHubCopilotChat/0.38.2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token": "tok_exchange;exp=1999999999",
            "expires_at": 1_999_999_999_i64,
            "refresh_in": 1500
        })))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let (token, expires_at) =
        copilot::exchange_copilot_token(&client, "gho_exchange_ok", &server.uri())
            .await
            .expect("exchange should succeed");
    assert_eq!(token, "tok_exchange;exp=1999999999");
    assert_eq!(expires_at, 1_999_999_999);
}

#[tokio::test]
async fn copilot_token_exchange_401_maps_to_actionable_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/copilot_internal/v2/token"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let err = copilot::exchange_copilot_token(&client, "gho_bad", &server.uri())
        .await
        .expect_err("401 must be an error");
    assert!(
        err.message.contains("GitHub token"),
        "message should point at the GitHub token: {}",
        err.message
    );
    let suggestion = err.suggestion.expect("401 must carry a suggestion");
    assert!(
        suggestion.contains("Copilot 订阅"),
        "suggestion should mention checking the Copilot subscription: {suggestion}"
    );
}

#[tokio::test]
async fn copilot_token_exchange_403_means_no_subscription() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/copilot_internal/v2/token"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"message": "forbidden"})))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let err = copilot::exchange_copilot_token(&client, "gho_no_sub", &server.uri())
        .await
        .expect_err("403 must be an error");
    assert!(
        err.message.contains("订阅"),
        "message should mention subscription: {}",
        err.message
    );
    assert!(err.suggestion.is_some());
}

#[tokio::test]
async fn copilot_token_exchange_5xx_keeps_body_detail() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/copilot_internal/v2/token"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal boom"))
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    let err = copilot::exchange_copilot_token(&client, "gho_5xx", &server.uri())
        .await
        .expect_err("500 must be an error");
    assert!(err.message.contains("500"), "message: {}", err.message);
    assert_eq!(err.detail.as_deref(), Some("internal boom"));
}

// ── 网关全链路 ──────────────────────────────────────────

/// 单个测试串行覆盖 messages + responses 两条链路:
/// `AGENTGATE_COPILOT_GITHUB_API_BASE` 是进程级环境变量,拆多个并行测试会互相覆盖。
#[tokio::test]
async fn copilot_gateway_end_to_end() {
    let mock = MockUpstream::start().await;
    let far_future = chrono::Utc::now().timestamp() + 3600;
    mock.stub_copilot_token_ok("tok_copilot_e2e", far_future).await;
    mock.stub_anthropic_messages_ok("claude-sonnet-4.6", "ok").await;

    // token 交换基址注入到同一个 mock(生产默认 api.github.com)
    std::env::set_var("AGENTGATE_COPILOT_GITHUB_API_BASE", mock.url());

    let spec = ProviderSpec::chat_only("copilot", "claude-sonnet-4.6")
        .with_anthropic(mock.url())
        .with_api_key("gho_gateway_e2e");
    let harness = GatewayHarness::start(spec, &mock).await;
    let client = harness.client();

    // 1) 工具续写请求(最后一条 user 全是 tool_result)→ x-initiator: agent
    let res = client
        .post(harness.url("/v1/messages"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 64,
            "messages": [
                {"role": "user", "content": "read the file"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "data"}
                ]}
            ]
        }))
        .send()
        .await
        .expect("send /v1/messages (agent)");
    assert!(res.status().is_success(), "gateway returned {}", res.status());

    // 2) 用户新输入 → x-initiator: user(同一 GitHub token,应命中 token 缓存)
    let res = client
        .post(harness.url("/v1/messages"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 64,
            "messages": [{"role": "user", "content": "hello"}]
        }))
        .send()
        .await
        .expect("send /v1/messages (user)");
    assert!(res.status().is_success(), "gateway returned {}", res.status());

    let received = mock.received().await;
    let token_calls: Vec<_> = received
        .iter()
        .filter(|r| r.path == "/copilot_internal/v2/token")
        .collect();
    let msg_calls: Vec<_> = received
        .iter()
        .filter(|r| r.path == "/v1/messages")
        .collect();

    assert_eq!(
        token_calls.len(),
        1,
        "token 交换应只发生一次(进程内缓存生效)"
    );
    assert_eq!(msg_calls.len(), 2);

    // 鉴权:Bearer Copilot token 替代 x-api-key
    for call in &msg_calls {
        assert_eq!(
            call.header("authorization"),
            Some("Bearer tok_copilot_e2e"),
            "must auth with the exchanged Copilot bearer token"
        );
        assert_eq!(call.header("x-api-key"), None, "x-api-key 不能发给 Copilot");
        assert_eq!(call.header("copilot-integration-id"), Some("vscode-chat"));
        assert_eq!(call.header("editor-version"), Some("vscode/1.110.1"));
        assert_eq!(call.header("x-github-api-version"), Some("2025-10-01"));
        // 模型归一化:dash → dot
        assert_eq!(
            call.body.get("model").and_then(|m| m.as_str()),
            Some("claude-sonnet-4.6"),
            "model must be normalized to Copilot dot form: {}",
            call.body_raw
        );
    }
    assert_eq!(msg_calls[0].header("x-initiator"), Some("agent"));
    assert_eq!(msg_calls[1].header("x-initiator"), Some("user"));

    harness.shutdown().await;

    // 3) Codex 链路:/v1/responses → chat 转换 → Copilot chat/completions
    let chat_mock = MockUpstream::start().await;
    chat_mock
        .stub_chat_completions_ok("claude-sonnet-4.6", "pong")
        .await;
    // 复用同一 GitHub token:exchange 已缓存,不需要 token 端点
    let spec = ProviderSpec::chat_only("copilot", "claude-sonnet-4.6")
        .with_api_key("gho_gateway_e2e");
    let harness = GatewayHarness::start(spec, &chat_mock).await;
    let client = harness.client();

    let res = client
        .post(harness.url("/v1/responses"))
        .bearer_auth(&harness.token)
        .json(&json!({
            "model": "claude-sonnet-4.6",
            "input": "ping",
            "stream": false,
            "max_output_tokens": 16
        }))
        .send()
        .await
        .expect("send /v1/responses");
    assert!(res.status().is_success(), "gateway returned {}", res.status());

    let received = chat_mock.received().await;
    let chat_call = received
        .iter()
        .find(|r| r.path == "/v1/chat/completions")
        .expect("upstream chat/completions should be hit");
    assert_eq!(
        chat_call.header("authorization"),
        Some("Bearer tok_copilot_e2e"),
        "chat path must also auth with the Copilot bearer token"
    );
    assert_eq!(chat_call.header("copilot-integration-id"), Some("vscode-chat"));
    assert_eq!(chat_call.header("editor-version"), Some("vscode/1.110.1"));

    harness.shutdown().await;
    std::env::remove_var("AGENTGATE_COPILOT_GITHUB_API_BASE");
}
