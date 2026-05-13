use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogListItem {
    pub id: String,
    pub request_id: String,
    pub timestamp: String,
    pub client: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub route: Option<String>,
    pub status_code: Option<i64>,
    pub latency_ms: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogDetail {
    pub id: String,
    pub request_id: String,
    pub timestamp: String,
    pub client: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub route: Option<String>,
    pub status_code: Option<i64>,
    pub latency_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub raw_request: Option<String>,
    pub converted_request: Option<String>,
    pub raw_response: Option<String>,
    pub converted_response: Option<String>,
    pub sse_events: Option<String>,
    pub tool_calls: Option<String>,
    pub error_message: Option<String>,
    pub trace_json: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestLogFilter {
    pub client: Option<String>,
    pub provider: Option<String>,
    pub status: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}
