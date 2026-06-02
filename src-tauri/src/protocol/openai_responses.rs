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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn responses_request_deserialization() {
        let body = json!({
            "model": "gpt-4",
            "input": "hello",
            "stream": true,
            "temperature": 0.5,
            "extra_field": 123
        });
        let req: ResponsesRequest = serde_json::from_value(body).unwrap();
        assert_eq!(req.model, Some("gpt-4".into()));
        assert_eq!(req.stream, Some(true));
        assert_eq!(req.temperature, Some(0.5));
        assert!(req.extra.contains_key("extra_field"));
    }

    #[test]
    fn responses_request_default() {
        let req = ResponsesRequest::default();
        assert!(req.model.is_none());
        assert!(req.input.is_null());
    }

    #[test]
    fn responses_request_unknown_fields_via_flatten() {
        let body = json!({
            "input": [],
            "custom_key": "custom_value"
        });
        let req: ResponsesRequest = serde_json::from_value(body).unwrap();
        assert_eq!(req.extra.get("custom_key").unwrap(), "custom_value");
    }
}
