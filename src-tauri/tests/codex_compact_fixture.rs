//! Codex remote compaction v2 端到端 fixture。
//!
//! 模拟 Codex CLI 触发 remote_compaction_v2 的真实路径:带 header 的 POST
//! /v1/responses,期望 SSE 流回三条事件(created + output_item.done + completed),
//! 其中 item 是 type=compaction + AgentGate 编码的 encrypted_content。
//!
//! 关键断言:
//! 1. 上游只被调用一次(summary 调用),路径是 /v1/chat/completions
//! 2. SSE 三个事件按顺序出现
//! 3. encrypted_content 含 magic prefix,base64 解码后是 mock 上游返回的 summary
//! 4. completed event 含 token usage 字段(没这个 Codex parser 卡住)

mod common;

use common::gateway_harness::{GatewayHarness, ProviderSpec};
use common::mock_upstream::MockUpstream;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::json;

/// codex_compact 处理器默认关。测试里要 env 开。
fn enable_codex_compact() {
    std::env::set_var("AGENTGATE_CODEX_COMPACT", "1");
}

#[tokio::test]
async fn codex_v2_compaction_via_header_returns_full_sse() {
    enable_codex_compact();
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("mock-default", "用户讨论了 FCP 调色,选 ProRes 4K 交付。")
        .await;
    let harness = GatewayHarness::start(
        ProviderSpec::chat_only("mimo", "mock-default"),
        &mock,
    )
    .await;

    let client = harness.client();
    let body = json!({
        "model": "gpt-5.5-openai-compact",
        "stream": true,
        "instructions": "你是 FCP 专家",
        "input": [
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "我想给 FCP 加专业默认值"}
            ]},
            {"type": "message", "role": "assistant", "content": [
                {"type": "output_text", "text": "建议从渲染质量开始,然后色彩管理。"}
            ]},
            {"type": "message", "role": "user", "content": [
                {"type": "input_text", "text": "继续讲色彩管理"}
            ]}
        ]
    });

    let resp = client
        .post(harness.url("/v1/responses"))
        .header("Authorization", format!("Bearer {}", harness.token))
        .header("x-codex-beta-features", "remote_compaction_v2")
        .header(
            "x-codex-turn-metadata",
            r#"{"request_kind":"compaction"}"#,
        )
        .json(&body)
        .send()
        .await
        .expect("send compact request");

    assert_eq!(resp.status(), 200, "网关应返回 200");
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.starts_with("text/event-stream"),
        "应是 SSE,而不是 {ct}"
    );

    let sse = resp.text().await.expect("read sse body");

    // 三个事件按顺序
    let i_created = sse
        .find("event: response.created")
        .expect("缺 response.created event,Codex stream state 不会 init");
    let i_item = sse
        .find("event: response.output_item.done")
        .expect("缺 output_item.done event");
    let i_done = sse
        .find("event: response.completed")
        .expect("缺 response.completed event");
    assert!(
        i_created < i_item && i_item < i_done,
        "事件顺序错:created={i_created} item={i_item} done={i_done}"
    );

    // item 必须是 compaction type
    assert!(
        sse.contains(r#""type":"compaction""#),
        "output item 必须 type=compaction"
    );
    assert!(sse.contains(r#""encrypted_content":""#));

    // usage 字段必填(Codex 解析 token_usage 用)
    assert!(
        sse.contains(r#""input_tokens_details":null"#),
        "completed event 必须含 usage 字段,否则 Codex parser 卡住"
    );

    // 反推 encrypted_content,确认能解出 mock 上游返回的 summary
    let enc_start = sse.find(r#""encrypted_content":""#).unwrap() + r#""encrypted_content":""#.len();
    let enc_end = sse[enc_start..].find('"').unwrap() + enc_start;
    let encrypted = &sse[enc_start..enc_end];
    let decoded = agentgate_lib::gateway::codex_compact::decode_summary(encrypted)
        .expect("encrypted_content 应能被 AgentGate 自己解码");
    assert!(
        decoded.contains("FCP") || decoded.contains("ProRes"),
        "解出的 summary 应是 mock 上游返回的内容,实际:{decoded}"
    );

    // 上游应被调用 1 次(summary call)
    let received = mock.received().await;
    assert_eq!(
        received.len(),
        1,
        "压缩流程应该正好调上游 1 次(summary 调用)"
    );
}

/// Wire-level 测试:用 Codex CLI 同款的 eventsource-stream 真实解析我们的
/// HTTP 响应,验证它能拿到 3 个 events(created / output_item.done / completed)
/// 且 event type + data 都正确。这是字符串断言抓不到的层。
#[tokio::test]
async fn sse_decodes_through_real_eventsource_stream() {
    enable_codex_compact();
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("mock-default", "MOCK_SUMMARY_PAYLOAD")
        .await;
    let harness = GatewayHarness::start(
        ProviderSpec::chat_only("mimo", "mock-default"),
        &mock,
    )
    .await;

    let resp = harness
        .client()
        .post(harness.url("/v1/responses"))
        .header("Authorization", format!("Bearer {}", harness.token))
        .header("x-codex-beta-features", "remote_compaction_v2")
        .json(&json!({
            "model": "gpt-5.5-openai-compact",
            "stream": true,
            "input": [
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "test content"}
                ]}
            ]
        }))
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status(), 200);

    // Codex 用的就是这个解析路径:bytes_stream → Eventsource
    let mut events = resp.bytes_stream().eventsource();

    let mut kinds = Vec::new();
    let mut item_payload: Option<String> = None;
    let mut completed_payload: Option<String> = None;
    while let Some(item) = events.next().await {
        let ev = item.expect("eventsource 解析失败");
        kinds.push(ev.event.clone());
        if ev.event == "response.output_item.done" {
            item_payload = Some(ev.data);
        } else if ev.event == "response.completed" {
            completed_payload = Some(ev.data);
        }
    }

    assert_eq!(
        kinds,
        vec![
            "response.created".to_string(),
            "response.output_item.done".to_string(),
            "response.completed".to_string(),
        ],
        "eventsource 应解出三个 event,实际:{kinds:?}"
    );

    // Codex 端 process_responses_event 路径:item 字段 deserialize 成 ResponseItem
    let item_data: serde_json::Value =
        serde_json::from_str(&item_payload.expect("item event missing")).unwrap();
    let item_inner = item_data.get("item").expect("item field missing");
    assert_eq!(
        item_inner.get("type").and_then(|v| v.as_str()),
        Some("compaction"),
        "item.type 必须是 compaction"
    );
    let enc = item_inner
        .get("encrypted_content")
        .and_then(|v| v.as_str())
        .expect("encrypted_content 必须是字符串");
    assert!(enc.starts_with("AGENTGATE_COMPACT_V1:"));

    // response.completed.usage 必须能让 Codex 的 ResponseCompletedUsage(i64/Option) parse
    let completed_data: serde_json::Value =
        serde_json::from_str(&completed_payload.expect("completed event missing")).unwrap();
    let usage = completed_data
        .get("response")
        .and_then(|r| r.get("usage"))
        .expect("usage 必填(Codex parser 用)");
    for required_i64 in &["input_tokens", "output_tokens", "total_tokens"] {
        assert!(
            usage.get(*required_i64).and_then(|v| v.as_i64()).is_some(),
            "usage.{required_i64} 必须是 i64,不能 missing/null"
        );
    }
}

#[tokio::test]
async fn non_codex_request_bypasses_compact_branch() {
    // 验证探嗅未命中时走原 chat 路径,不会触发 compact handler。
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("mock-default", "hello").await;
    let harness = GatewayHarness::start(
        ProviderSpec::chat_only("mimo", "mock-default"),
        &mock,
    )
    .await;

    let resp = harness
        .client()
        .post(harness.url("/v1/responses"))
        .header("Authorization", format!("Bearer {}", harness.token))
        // 没有 v2 探嗅信号,应该走原流程
        .json(&json!({
            "model": "gpt-4",
            "stream": false,
            "input": "say hi"
        }))
        .send()
        .await
        .expect("send normal request");

    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap_or_default();
    // 走原流程应该没有 compaction item
    assert!(
        !body.contains(r#""type":"compaction""#),
        "非 compact 请求不应触发 compact 分支"
    );
}

#[tokio::test]
async fn prior_compaction_item_is_restored_to_user_message() {
    // 验证下一轮:Codex 把上次的 compaction item 塞回 input,
    // 网关在 transform 层应该解出 summary 注入到 chat messages。
    // 这条路径不依赖 codex_compact handler,所以不用开 env。
    let mock = MockUpstream::start().await;
    mock.stub_chat_completions_ok("mock-default", "回复").await;
    let harness = GatewayHarness::start(
        ProviderSpec::chat_only("mimo", "mock-default"),
        &mock,
    )
    .await;

    let prior_summary = "上轮的 summary 内容";
    let encrypted = agentgate_lib::gateway::codex_compact::encode_summary(prior_summary);

    let resp = harness
        .client()
        .post(harness.url("/v1/responses"))
        .header("Authorization", format!("Bearer {}", harness.token))
        .json(&json!({
            "model": "gpt-4",
            "stream": false,
            "input": [
                {"type": "compaction", "encrypted_content": encrypted},
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "继续"}
                ]}
            ]
        }))
        .send()
        .await
        .expect("send");

    assert_eq!(resp.status(), 200);

    // 上游收到的 chat completion 请求里 messages 应含解出的 summary
    let received = mock.received().await;
    assert!(!received.is_empty(), "上游应被调用");
    let upstream_body = &received[0].body;
    let upstream_str = serde_json::to_string(upstream_body).unwrap_or_default();
    assert!(
        upstream_str.contains(prior_summary),
        "上游请求里应能看到解出的 prior summary,实际 body: {}",
        &upstream_str[..upstream_str.len().min(500)]
    );
    assert!(
        upstream_str.contains("Prior compacted history"),
        "应有注入注释"
    );
}
