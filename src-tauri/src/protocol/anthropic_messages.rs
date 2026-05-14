use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Anthropic Messages API request (loosely typed for compatibility).
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MessagesRequest {
    pub model: Option<String>,
    pub messages: Vec<AnthropicMessage>,
    pub system: Option<Value>,
    pub max_tokens: Option<i64>,
    pub stream: Option<bool>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub tools: Option<Vec<Value>>,
    pub tool_choice: Option<Value>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: Value,
}

/// Convert Anthropic Messages -> Chat Completions messages.
pub fn to_chat_messages(
    req: &MessagesRequest,
) -> Vec<crate::protocol::chat_completions::ChatMessage> {
    let mut messages = Vec::new();

    // System message
    if let Some(ref sys) = req.system {
        let text = match sys {
            Value::String(s) => s.clone(),
            Value::Array(arr) => {
                arr.iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            _ => sys.to_string(),
        };
        if !text.is_empty() {
            messages.push(crate::protocol::chat_completions::ChatMessage {
                role: "system".to_string(),
                content: Some(Value::String(text)),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
    }

    // Convert each message
    for msg in &req.messages {
        match msg.role.as_str() {
            "user" => {
                let text = extract_text_content(&msg.content);
                messages.push(crate::protocol::chat_completions::ChatMessage {
                    role: "user".to_string(),
                    content: Some(Value::String(text)),
                    reasoning_content: None,
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
            "assistant" => {
                let (text, tool_calls) = extract_assistant_content(&msg.content);
                messages.push(crate::protocol::chat_completions::ChatMessage {
                    role: "assistant".to_string(),
                    content: if text.is_empty() { None } else { Some(Value::String(text)) },
                    reasoning_content: None,
                    tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                    tool_call_id: None,
                    name: None,
                });
            }
            _ => {
                // tool_result -> tool message
                if let Some(tool_use_id) = msg.content.get("tool_use_id").and_then(|v| v.as_str()) {
                    let output = extract_text_content(&msg.content);
                    messages.push(crate::protocol::chat_completions::ChatMessage {
                        role: "tool".to_string(),
                        content: Some(Value::String(output)),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(tool_use_id.to_string()),
                        name: None,
                    });
                } else if msg.content.is_array() {
                    // Array of tool_result blocks
                    for block in msg.content.as_array().unwrap_or(&vec![]) {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                            let tid = block.get("tool_use_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let output = block.get("content").map(|c| extract_text_content(c)).unwrap_or_default();
                            messages.push(crate::protocol::chat_completions::ChatMessage {
                                role: "tool".to_string(),
                                content: Some(Value::String(output)),
                                reasoning_content: None,
                                tool_calls: None,
                                tool_call_id: Some(tid),
                                name: None,
                            });
                        }
                    }
                }
            }
        }
    }

    messages
}

fn extract_text_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            arr.iter()
                .filter_map(|p| {
                    let t = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if t == "text" || t == "input_text" {
                        p.get("text").and_then(|t| t.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        }
        _ => content.to_string(),
    }
}

fn extract_assistant_content(content: &Value) -> (String, Vec<crate::protocol::chat_completions::ToolCall>) {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    match content {
        Value::String(s) => text = s.clone(),
        Value::Array(arr) => {
            for block in arr {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            text.push_str(t);
                        }
                    }
                    "tool_use" => {
                        let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let input = block.get("input").map(|v| v.to_string()).unwrap_or("{}".to_string());
                        tool_calls.push(crate::protocol::chat_completions::ToolCall {
                            id,
                            call_type: "function".to_string(),
                            function: crate::protocol::chat_completions::ToolCallFunction { name, arguments: input },
                        });
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    (text, tool_calls)
}

/// Convert Chat Completions response back to Anthropic Messages format.
pub fn from_chat_response(upstream: &Value, model: &str) -> Value {
    let mut content = Vec::<Value>::new();

    if let Some(choices) = upstream.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            if let Some(msg) = choice.get("message") {
                if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        content.push(serde_json::json!({"type": "text", "text": text}));
                    }
                }
                if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let id = tc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        let name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
                        let args_str = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()).unwrap_or("{}");
                        let input: Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
                        content.push(serde_json::json!({
                            "type": "tool_use", "id": id, "name": name, "input": input
                        }));
                    }
                }
            }
        }
    }

    let stop_reason = if content.iter().any(|c| c.get("type").and_then(|t| t.as_str()) == Some("tool_use")) {
        "tool_use"
    } else {
        "end_turn"
    };

    serde_json::json!({
        "id": format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string()),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": stop_reason,
        "usage": upstream.get("usage").cloned().unwrap_or(serde_json::json!({})),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;


    #[test]
    fn test_to_chat_messages_with_string_system() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![],
            system: Some(json!("You are helpful")),
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, Some(json!("You are helpful")));
    }

    #[test]
    fn test_to_chat_messages_with_array_system() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![],
            system: Some(json!([
                {"type": "text", "text": "Part 1"},
                {"type": "text", "text": "Part 2"}
            ])),
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages[0].content, Some(json!("Part 1\nPart 2")));
    }

    #[test]
    fn test_to_chat_messages_user_message() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: json!("hello"),
            }],
            system: None,
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, Some(json!("hello")));
    }

    #[test]
    fn test_to_chat_messages_assistant_with_tool_use() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![AnthropicMessage {
                role: "assistant".to_string(),
                content: json!([
                    {"type": "text", "text": "Let me check"},
                    {"type": "tool_use", "id": "tu1", "name": "search", "input": {"q": "test"}}
                ]),
            }],
            system: None,
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "assistant");
        assert_eq!(messages[0].content, Some(json!("Let me check")));
        assert!(messages[0].tool_calls.is_some());
        let tcs = messages[0].tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "tu1");
        assert_eq!(tcs[0].function.name, "search");
    }

    #[test]
    fn test_to_chat_messages_tool_result_non_user_role() {
        // Non-user/non-assistant role with array of tool_result blocks
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![AnthropicMessage {
                role: "tool".to_string(),
                content: json!([{"type": "tool_result", "tool_use_id": "tu1", "content": "result data"}]),
            }],
            system: None,
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "tool");
        assert_eq!(messages[0].tool_call_id, Some("tu1".to_string()));
        assert_eq!(messages[0].content, Some(json!("result data")));
    }

    #[test]
    fn test_to_chat_messages_tool_result_array_non_user_role() {
        let req = MessagesRequest {
            model: Some("claude-3".to_string()),
            messages: vec![AnthropicMessage {
                role: "tool".to_string(),
                content: json!([
                    {"type": "tool_result", "tool_use_id": "tu1", "content": "r1"},
                    {"type": "tool_result", "tool_use_id": "tu2", "content": "r2"}
                ]),
            }],
            system: None,
            max_tokens: None,
            stream: None,
            temperature: None,
            top_p: None,
            tools: None,
            tool_choice: None,
            extra: std::collections::HashMap::new(),
        };
        let messages = to_chat_messages(&req);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].tool_call_id, Some("tu1".to_string()));
        assert_eq!(messages[1].tool_call_id, Some("tu2".to_string()));
    }

    #[test]
    fn test_from_chat_response_text_only() {
        let upstream = json!({
            "choices": [{"message": {"content": "Hello!"}}],
            "usage": {"input_tokens": 10}
        });
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["role"], "assistant");
        assert_eq!(resp["model"], "claude-3");
        assert_eq!(resp["stop_reason"], "end_turn");
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Hello!");
    }

    #[test]
    fn test_from_chat_response_with_tool_calls() {
        let upstream = json!({
            "choices": [{"message": {
                "content": "",
                "tool_calls": [
                    {"id": "tc1", "function": {"name": "search", "arguments": "{\"q\":\"hi\"}"}}
                ]
            }}]
        });
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["stop_reason"], "tool_use");
        let content = resp["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["name"], "search");
    }

    #[test]
    fn test_from_chat_response_empty_choices() {
        let upstream = json!({"choices": []});
        let resp = from_chat_response(&upstream, "claude-3");
        assert_eq!(resp["role"], "assistant");
        let content = resp["content"].as_array().unwrap();
        assert!(content.is_empty());
        assert_eq!(resp["stop_reason"], "end_turn");
    }
}
