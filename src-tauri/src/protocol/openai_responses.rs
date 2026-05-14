use serde::Deserialize;
use serde_json::Value;

/// Loosely-typed Responses API request — accepts unknown fields via flatten.
#[derive(Debug, Clone, Default, Deserialize)]
#[allow(dead_code)]
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

