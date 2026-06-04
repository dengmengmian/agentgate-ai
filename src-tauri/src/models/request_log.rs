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
    /// 'gateway' / 'claude_session' / 'codex_session' / 'gemini_session'
    pub source: Option<String>,
    /// 会话指纹：gateway 来源走 session_affinity；客户端日志来源是文件里的 session id
    pub session_id: Option<String>,
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
    pub source: Option<String>,
    pub session_id: Option<String>,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogFilter {
    pub client: Option<String>,
    pub provider: Option<String>,
    pub status: Option<String>,
    pub keyword: Option<String>,
    /// 'gateway' / 'claude_session' / 'codex_session' / 'gemini_session' /
    /// 'session_log'（聚合：所有非 gateway 来源）。
    pub source: Option<String>,
    /// 按指定 session_id 过滤——「按会话分组」视图点开某条 session 时用。
    pub session_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// 按 session 维度聚合的用量摘要。Logs 页「按会话分组」视图用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsageSummary {
    pub session_id: String,
    /// 该 session 多数请求的 source。混合时填 'mixed'。
    pub source: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost: f64,
}

/// 成本仪表盘用：按某维度（模型 / 客户端）聚合的成本与用量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    /// 维度值：模型名 或 客户端名。
    pub key: String,
    /// 该维度下出现过的一个 provider（按客户端聚合时仅供参考）。
    pub provider: Option<String>,
    pub request_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_log_list_item_serde() {
        let item = RequestLogListItem {
            id: "1".to_string(),
            request_id: "req_1".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            client: Some("codex".to_string()),
            provider: Some("openai".to_string()),
            model: Some("gpt-4".to_string()),
            route: Some("/v1/responses".to_string()),
            status_code: Some(200),
            latency_ms: Some(500),
            error_message: None,
            source: Some("gateway".to_string()),
            session_id: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("req_1"));
        let de: RequestLogListItem = serde_json::from_str(&json).unwrap();
        assert_eq!(de.id, "1");
        assert_eq!(de.status_code, Some(200));
    }

    #[test]
    fn test_request_log_filter_serde() {
        let filter = RequestLogFilter {
            client: Some("codex".to_string()),
            provider: None,
            status: Some("success".to_string()),
            keyword: Some("error".to_string()),
            source: None,
            session_id: None,
            limit: Some(50),
            offset: Some(0),
        };
        let json = serde_json::to_string(&filter).unwrap();
        let de: RequestLogFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(de.client, Some("codex".to_string()));
        assert_eq!(de.limit, Some(50));
    }

    #[test]
    fn test_request_log_detail_serde() {
        let detail = RequestLogDetail {
            id: "1".to_string(),
            request_id: "req_1".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            client: Some("codex".to_string()),
            provider: Some("openai".to_string()),
            model: Some("gpt-4".to_string()),
            route: Some("/v1/responses".to_string()),
            status_code: Some(200),
            latency_ms: Some(500),
            input_tokens: Some(100),
            output_tokens: Some(50),
            raw_request: Some(r#"{"input":"hello"}"#.to_string()),
            converted_request: None,
            raw_response: Some(r#"{"output":"hi"}"#.to_string()),
            converted_response: None,
            sse_events: None,
            tool_calls: None,
            error_message: None,
            trace_json: None,
            source: Some("gateway".to_string()),
            session_id: None,
            external_id: None,
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert!(json.contains("hello"));
        let de: RequestLogDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(de.input_tokens, Some(100));
    }
}
