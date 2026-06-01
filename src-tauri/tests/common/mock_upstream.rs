//! Mock upstream AI provider for offline capability tests.
//!
//! Wraps `wiremock` with helpers that speak the three protocols AgentGate
//! transforms between: OpenAI Chat Completions, OpenAI Responses, and
//! Anthropic Messages. Each stub method records the canned reply; callers
//! later inspect `received_requests()` to assert what AgentGate actually
//! sent upstream after L1 / L2 / L3 transforms.

use serde_json::{json, Value};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub struct MockUpstream {
    server: MockServer,
}

impl MockUpstream {
    pub async fn start() -> Self {
        Self { server: MockServer::start().await }
    }

    /// Base URL the gateway should use as the provider's `base_url` /
    /// `anthropic_base_url`. Already includes scheme + host + port, no path.
    pub fn url(&self) -> String {
        self.server.uri()
    }

    /// All requests the mock received since startup, in order. JSON body
    /// is best-effort parsed; non-JSON bodies surface as `Value::Null`.
    pub async fn received(&self) -> Vec<ReceivedRequest> {
        self.server
            .received_requests()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                let body_json = serde_json::from_slice::<Value>(&r.body).unwrap_or(Value::Null);
                ReceivedRequest {
                    method: r.method.to_string(),
                    path: r.url.path().to_string(),
                    headers: r
                        .headers
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or_default().to_string()))
                        .collect(),
                    body_raw: String::from_utf8_lossy(&r.body).to_string(),
                    body: body_json,
                }
            })
            .collect()
    }

    /// Stub `POST /v1/chat/completions` returning a minimal OpenAI-shaped
    /// chat completion. `model` and `content` are echoed in the response.
    pub async fn stub_chat_completions_ok(&self, model: &str, content: &str) {
        let body = chat_completion_body(model, content);
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// Stub `POST /v1/chat/completions` returning a chat completion that
    /// carries DeepSeek-style `reasoning_content`. Useful for verifying that
    /// the gateway preserves (does not strip) reasoning fields when the
    /// upstream model supports them.
    pub async fn stub_chat_completions_with_reasoning(
        &self,
        model: &str,
        reasoning: &str,
        content: &str,
    ) {
        let body = chat_completion_body_with_reasoning(model, reasoning, content);
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// Stub `POST /v1/chat/completions` returning the given status + JSON body.
    pub async fn stub_chat_completions_err(&self, status: u16, body: Value) {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(status).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// Stub `POST /v1/messages` returning a minimal Anthropic message.
    pub async fn stub_anthropic_messages_ok(&self, model: &str, content: &str) {
        let body = anthropic_message_body(model, content);
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&self.server)
            .await;
    }

    /// Two-stage stub for MiMo's "web_search plugin not enabled" path:
    /// first call returns 400 with the upstream's plugin error marker
    /// (`webSearchEnabled is false`), every subsequent call returns 200.
    /// Lets tests verify the gateway strips `web_search` and retries.
    pub async fn stub_mimo_web_search_unavailable_then_ok(&self, model: &str, content: &str) {
        let err = json!({
            "error": {
                "message": "web_search tool found in the request body, but webSearchEnabled is false",
                "type": "invalid_request_error"
            }
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_json(err))
            .up_to_n_times(1)
            .mount(&self.server)
            .await;
        self.stub_chat_completions_ok(model, content).await;
    }
}

#[derive(Debug, Clone)]
pub struct ReceivedRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body_raw: String,
    pub body: Value,
}

impl ReceivedRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

fn chat_completion_body(model: &str, content: &str) -> Value {
    json!({
        "id": "chatcmpl-mock-001",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": model,
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 4, "completion_tokens": 2, "total_tokens": 6 }
    })
}

fn chat_completion_body_with_reasoning(model: &str, reasoning: &str, content: &str) -> Value {
    json!({
        "id": "chatcmpl-mock-002",
        "object": "chat.completion",
        "created": 1_700_000_000,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": content,
                "reasoning_content": reasoning
            },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 4, "completion_tokens": 6, "total_tokens": 10 }
    })
}

fn anthropic_message_body(model: &str, content: &str) -> Value {
    json!({
        "id": "msg_mock_001",
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{ "type": "text", "text": content }],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": { "input_tokens": 4, "output_tokens": 2 }
    })
}
