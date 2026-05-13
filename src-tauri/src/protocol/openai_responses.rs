use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Loosely-typed Responses API request — accepts unknown fields via flatten.
#[derive(Debug, Clone, Deserialize)]
pub struct ResponsesRequest {
    pub model: Option<String>,
    pub input: Value,
    pub instructions: Option<String>,
    pub system: Option<String>,
    pub previous_response_id: Option<String>,
    pub tools: Option<Vec<Value>>,
    pub tool_choice: Option<Value>,
    pub stream: Option<bool>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub max_output_tokens: Option<i64>,
    pub parallel_tool_calls: Option<bool>,
    pub reasoning: Option<Value>,
    pub text: Option<Value>,
    pub metadata: Option<Value>,
    pub seed: Option<Value>,
    pub stop: Option<Value>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}

/// Responses API SSE event types
#[derive(Debug, Clone, Serialize)]
pub struct ResponsesEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(flatten)]
    pub data: Value,
}

/// Non-stream response
#[derive(Debug, Clone, Serialize)]
pub struct ResponsesResponse {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: String,
    pub model: String,
    pub output: Vec<Value>,
}

impl ResponsesResponse {
    pub fn completed(id: String, model: String, output: Vec<Value>) -> Self {
        Self {
            id,
            object: "response".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            status: "completed".to_string(),
            model,
            output,
        }
    }

    pub fn failed(id: String, model: String) -> Self {
        Self {
            id,
            object: "response".to_string(),
            created_at: chrono::Utc::now().timestamp(),
            status: "failed".to_string(),
            model,
            output: vec![],
        }
    }
}
