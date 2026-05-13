use serde_json::{json, Value};

use crate::errors::AppError;
use crate::protocol::chat_completions::ChatMessage;
use crate::transform::schema_cleaner::clean_schema_for_deepseek;

/// Convert Responses API tools to Chat Completions tools format.
/// Handles structure A (flat), structure B (nested function), namespace tools, and custom tools.
/// Returns (converted_tools, namespace_map) where namespace_map maps function name -> namespace.
pub fn convert_tools(tools: &[Value], clean_for_deepseek: bool) -> Vec<Value> {
    convert_tools_for_provider(tools, clean_for_deepseek, "")
}

/// Convert tools with provider-type awareness (e.g. Kimi web_search).
pub fn convert_tools_for_provider(tools: &[Value], clean_for_deepseek: bool, provider_type: &str) -> Vec<Value> {
    let is_kimi = provider_type == "kimi" || provider_type.contains("moonshot");
    let mut result = Vec::new();
    for tool in tools {
        let tool_type = tool.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match tool_type {
            "function" => {
                if let Some(converted) = convert_function_tool(tool, clean_for_deepseek) {
                    result.push(converted);
                }
            }
            "namespace" => {
                // Flatten namespace tools: recursively extract function tools
                let ns_name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if let Some(Value::Array(sub_tools)) = tool.get("tools") {
                    for sub in sub_tools {
                        let sub_type = sub.get("type").and_then(|t| t.as_str()).unwrap_or("");
                        if sub_type == "function" {
                            if let Some(mut converted) = convert_function_tool(sub, clean_for_deepseek) {
                                // Prefix function name with namespace for uniqueness
                                if !ns_name.is_empty() {
                                    if let Some(func) = converted.get_mut("function") {
                                        if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                            let prefixed = format!("{ns_name}__{name}");
                                            func["name"] = json!(prefixed);
                                        }
                                    }
                                }
                                result.push(converted);
                            }
                        }
                    }
                }
            }
            "custom" => {
                // Downgrade custom tool to function with single string input
                let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("custom_tool");
                let desc = tool.get("description").and_then(|d| d.as_str()).unwrap_or("");
                result.push(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": desc,
                        "parameters": { "type": "object", "properties": { "input": { "type": "string" } }, "required": ["input"] }
                    }
                }));
            }
            "web_search" | "web_search_preview" => {
                if is_kimi {
                    // Kimi uses builtin_function/$web_search
                    result.push(json!({
                        "type": "builtin_function",
                        "function": { "name": "$web_search" }
                    }));
                }
                // Other providers: skip (not supported by Chat Completions)
            }
            _ => {
                // Skip code_interpreter, file_search, etc.
            }
        }
    }
    result
}

/// Check if converted tools contain Kimi's $web_search builtin.
pub fn contains_kimi_web_search(tools: &[Value]) -> bool {
    tools.iter().any(|t| {
        t.get("type").and_then(|ty| ty.as_str()) == Some("builtin_function")
            && t.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()) == Some("$web_search")
    })
}

fn convert_function_tool(tool: &Value, clean_for_deepseek: bool) -> Option<Value> {
    // Structure B: already has "function" wrapper
    if let Some(func) = tool.get("function") {
        let mut result = json!({ "type": "function", "function": func.clone() });
        if clean_for_deepseek {
            if let Some(params) = result.get_mut("function").and_then(|f| f.get_mut("parameters")) {
                clean_schema_for_deepseek(params);
            }
        }
        return Some(result);
    }

    // Structure A: flat name/description/parameters
    let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let desc = tool.get("description").and_then(|d| d.as_str()).unwrap_or("");
    let mut params = tool.get("parameters").cloned().unwrap_or(json!({}));

    if clean_for_deepseek {
        clean_schema_for_deepseek(&mut params);
    }

    Some(json!({
        "type": "function",
        "function": { "name": name, "description": desc, "parameters": params }
    }))
}

/// Convert tool_choice from Responses API to Chat Completions format.
pub fn convert_tool_choice(tc: &Value) -> Value {
    match tc {
        Value::String(s) => Value::String(s.clone()),
        Value::Object(obj) => {
            if obj.get("function").is_some() {
                return tc.clone();
            }
            if let Some(name) = obj.get("name").and_then(|n| n.as_str()) {
                return json!({
                    "type": "function",
                    "function": { "name": name }
                });
            }
            tc.clone()
        }
        _ => tc.clone(),
    }
}

/// Fix tool message ordering for DeepSeek.
///
/// Rules:
/// 1. After an assistant message with tool_calls, corresponding tool messages must follow immediately.
/// 2. Multiple tool_calls in one assistant message: tool outputs follow in matching order.
/// 3. The LAST assistant message with tool_calls in the conversation may not have tool outputs yet
///    (this is the current turn where the model previously requested tools and Codex hasn't filled them yet,
///     or this is new context). We are lenient about this.
/// 4. Unrelated user/system messages must not appear between assistant tool_calls and tool outputs.
pub fn fix_tool_message_order(messages: Vec<ChatMessage>) -> Result<Vec<ChatMessage>, AppError> {
    // Collect all tool outputs indexed by tool_call_id
    let mut tool_output_pool: Vec<ChatMessage> = Vec::new();
    let mut other_messages: Vec<ChatMessage> = Vec::new();

    for msg in messages {
        if msg.role == "tool" {
            tool_output_pool.push(msg);
        } else {
            other_messages.push(msg);
        }
    }

    let mut result: Vec<ChatMessage> = Vec::new();
    let total = other_messages.len();

    for (i, msg) in other_messages.into_iter().enumerate() {
        let is_last = i == total - 1;

        if msg.role == "assistant" {
            result.push(msg.clone());

            if let Some(ref tcs) = msg.tool_calls {
                for tc in tcs {
                    let idx = tool_output_pool.iter().position(|t| {
                        t.tool_call_id.as_deref() == Some(&tc.id)
                    });
                    match idx {
                        Some(i) => {
                            result.push(tool_output_pool.remove(i));
                        }
                        None => {
                            // Be lenient for the last assistant message — Codex may not have
                            // filled the tool outputs yet (this is the model's pending request).
                            if !is_last {
                                return Err(AppError::new(
                                    "TOOL_OUTPUT_NOT_FOUND",
                                    format!("Missing tool output for tool call '{}' (function: {})", tc.id, tc.function.name),
                                ).with_suggestion("Check that all function_call items have matching function_call_output items in the input"));
                            }
                            // For the last assistant, skip silently — the model is about to respond
                        }
                    }
                }
            }
        } else {
            result.push(msg);
        }
    }

    // Append any remaining unmatched tool outputs at the end (lenient)
    for t in tool_output_pool {
        result.push(t);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::protocol::chat_completions::{ChatMessage, ToolCall, ToolCallFunction};

    #[test]
    fn test_convert_function_tool_structure_b() {
        let tool = json!({
            "type": "function",
            "function": {
                "name": "search",
                "description": "Search the web",
                "parameters": {"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]}
            }
        });
        let result = convert_function_tool(&tool, false);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r["type"], "function");
        assert_eq!(r["function"]["name"], "search");
    }

    #[test]
    fn test_convert_function_tool_structure_a() {
        let tool = json!({
            "type": "function",
            "name": "search",
            "description": "Search the web",
            "parameters": {"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]}
        });
        let result = convert_function_tool(&tool, false);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r["type"], "function");
        assert_eq!(r["function"]["name"], "search");
    }

    #[test]
    fn test_convert_tools_namespace() {
        let tools = vec![json!({
            "type": "namespace",
            "name": "math",
            "tools": [
                {"type": "function", "name": "add", "description": "Add numbers", "parameters": {"type": "object"}}
            ]
        })];
        let result = convert_tools(&tools, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["function"]["name"], "math__add");
    }

    #[test]
    fn test_convert_tools_custom() {
        let tools = vec![json!({
            "type": "custom",
            "name": "my_tool",
            "description": "Does stuff"
        })];
        let result = convert_tools(&tools, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["function"]["name"], "my_tool");
        assert_eq!(result[0]["function"]["parameters"]["properties"]["input"]["type"], "string");
    }

    #[test]
    fn test_convert_tools_web_search_kimi() {
        let tools = vec![json!({"type": "web_search"})];
        let result = convert_tools_for_provider(&tools, false, "kimi");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "builtin_function");
        assert_eq!(result[0]["function"]["name"], "$web_search");
    }

    #[test]
    fn test_convert_tools_web_search_non_kimi() {
        let tools = vec![json!({"type": "web_search"})];
        let result = convert_tools_for_provider(&tools, false, "openai");
        assert!(result.is_empty());
    }

    #[test]
    fn test_contains_kimi_web_search() {
        let tools = vec![json!({"type": "builtin_function", "function": {"name": "$web_search"}})];
        assert!(contains_kimi_web_search(&tools));

        let other = vec![json!({"type": "function", "function": {"name": "search"}})];
        assert!(!contains_kimi_web_search(&other));
    }

    #[test]
    fn test_convert_tool_choice_string() {
        assert_eq!(convert_tool_choice(&json!("auto")), json!("auto"));
        assert_eq!(convert_tool_choice(&json!("none")), json!("none"));
    }

    #[test]
    fn test_convert_tool_choice_function() {
        let tc = json!({"function": {"name": "search"}});
        assert_eq!(convert_tool_choice(&tc), tc);
    }

    #[test]
    fn test_convert_tool_choice_name() {
        let tc = json!({"name": "search"});
        let result = convert_tool_choice(&tc);
        assert_eq!(result["type"], "function");
        assert_eq!(result["function"]["name"], "search");
    }

    #[test]
    fn test_fix_tool_message_order_basic() {
        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(json!("call tool")),
                reasoning_content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction { name: "search".to_string(), arguments: "{}".to_string() },
                }]),
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "tool".to_string(),
                content: Some(json!("result")),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
                name: None,
            },
        ];
        let result = fix_tool_message_order(messages).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "assistant");
        assert_eq!(result[1].role, "tool");
    }

    #[test]
    fn test_fix_tool_message_order_missing_non_last() {
        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(json!("call tool")),
                reasoning_content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction { name: "search".to_string(), arguments: "{}".to_string() },
                }]),
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(json!("hello")),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];
        let result = fix_tool_message_order(messages);
        assert!(result.is_err());
    }

    #[test]
    fn test_fix_tool_message_order_lenient_last() {
        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(json!("call tool")),
                reasoning_content: None,
                tool_calls: Some(vec![ToolCall {
                    id: "call_1".to_string(),
                    call_type: "function".to_string(),
                    function: ToolCallFunction { name: "search".to_string(), arguments: "{}".to_string() },
                }]),
                tool_call_id: None,
                name: None,
            },
        ];
        let result = fix_tool_message_order(messages).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_fix_tool_message_order_multiple_tools() {
        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(json!("call tools")),
                reasoning_content: None,
                tool_calls: Some(vec![
                    ToolCall {
                        id: "call_1".to_string(),
                        call_type: "function".to_string(),
                        function: ToolCallFunction { name: "a".to_string(), arguments: "{}".to_string() },
                    },
                    ToolCall {
                        id: "call_2".to_string(),
                        call_type: "function".to_string(),
                        function: ToolCallFunction { name: "b".to_string(), arguments: "{}".to_string() },
                    },
                ]),
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "tool".to_string(),
                content: Some(json!("r2")),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: Some("call_2".to_string()),
                name: None,
            },
            ChatMessage {
                role: "tool".to_string(),
                content: Some(json!("r1")),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: Some("call_1".to_string()),
                name: None,
            },
        ];
        let result = fix_tool_message_order(messages).unwrap();
        assert_eq!(result[1].tool_call_id, Some("call_1".to_string()));
        assert_eq!(result[2].tool_call_id, Some("call_2".to_string()));
    }

    #[test]
    fn test_fix_tool_message_order_unmatched_at_end() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: Some(json!("hi")),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "tool".to_string(),
                content: Some(json!("orphan")),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: Some("call_x".to_string()),
                name: None,
            },
        ];
        let result = fix_tool_message_order(messages).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].role, "tool");
    }
}
