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
    convert_tools_with_matrix(tools, clean_for_deepseek, provider_type, "", &Default::default())
}

/// Same as convert_tools_for_provider but consults the per-model capability
/// matrix to decide whether to emit MiMo's `web_search` builtin. If the matrix
/// has an entry for the target model AND that entry doesn't include
/// "web_search", we skip emission — lets users disable forwarding per model
/// (e.g. when the MiMo account has no Web Search Plugin activated).
///
/// Matrix-empty behavior is unchanged for back-compat.
pub fn convert_tools_with_matrix(
    tools: &[Value],
    clean_for_deepseek: bool,
    provider_type: &str,
    model: &str,
    matrix: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<Value> {
    let is_kimi = provider_type == "kimi" || provider_type.contains("moonshot");
    let is_mimo = provider_type == "mimo" || provider_type == "xiaomi" || provider_type.contains("mimo");

    // Strip [1m]/[...] qualifier before looking up in matrix (matrix keys use base id).
    let model_base = {
        let m = model;
        if let Some(stripped) = m.strip_suffix(']') {
            if let Some(open) = stripped.rfind('[') { &stripped[..open] } else { m }
        } else { m }
    };
    // Whether to emit web_search for MiMo: only suppress when matrix has an
    // explicit entry that excludes web_search. Empty matrix → emit (back-compat).
    let mimo_emit_web_search = matrix.get(model_base)
        .map(|caps| caps.iter().any(|c| c == "web_search"))
        .unwrap_or(true);

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
            "local_shell" => {
                // Codex's builtin local_shell → standard function tool named "shell".
                // Codex accepts tool_calls with either name, so emitting "shell" works.
                result.push(json!({
                    "type": "function",
                    "function": {
                        "name": "shell",
                        "description": "Execute a shell command on the local machine. Returns stdout, stderr and exit code.",
                        "parameters": {
                            "type": "object",
                            "properties": {
                                "command": {
                                    "type": "array",
                                    "items": { "type": "string" },
                                    "description": "Argv array, e.g. [\"ls\", \"-la\"]. The first element is the program; remaining elements are arguments."
                                },
                                "workdir": {
                                    "type": "string",
                                    "description": "Working directory to run the command in (optional)."
                                },
                                "timeout_ms": {
                                    "type": "number",
                                    "description": "Timeout in milliseconds (optional, default 30000)."
                                }
                            },
                            "required": ["command"]
                        }
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
                } else if is_mimo && mimo_emit_web_search {
                    // MiMo's native web_search builtin. Preserve Codex's optional
                    // config fields; MiMo's account-level "Web Search Plugin" must
                    // be activated, otherwise upstream 400s with
                    // "webSearchEnabled is false". Users opt out per-model by
                    // unchecking `web_search` in the capability matrix.
                    let mut ws = json!({ "type": "web_search" });
                    for k in &["user_location", "max_keyword", "force_search", "limit"] {
                        if let Some(v) = tool.get(*k) {
                            ws[*k] = v.clone();
                        }
                    }
                    result.push(ws);
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
/// Rules (matching codex_proxy.py):
/// 1. After an assistant message with tool_calls, corresponding tool messages must follow immediately.
/// 2. System/developer messages injected between assistant and tool messages are moved before the assistant.
///    (Codex sometimes injects approval notifications as system messages.)
/// 3. Multiple tool_calls in one assistant message: tool outputs follow in matching order.
/// 4. The LAST assistant with tool_calls may have missing tool outputs (model pending request).
pub fn fix_tool_message_order(messages: Vec<ChatMessage>) -> Result<Vec<ChatMessage>, AppError> {
    let mut reordered: Vec<ChatMessage> = Vec::new();
    let len = messages.len();
    let mut i = 0;

    while i < len {
        let msg = &messages[i];

        if msg.role == "assistant" && msg.tool_calls.is_some() {
            let tcs = msg.tool_calls.as_ref().unwrap();
            let mut expected_ids: Vec<String> = tcs.iter().map(|tc| tc.id.clone()).collect();
            let mut tool_msgs: Vec<ChatMessage> = Vec::new();
            let mut non_tool_msgs: Vec<ChatMessage> = Vec::new();

            let mut j = i + 1;
            while j < len && !expected_ids.is_empty() {
                let nxt = &messages[j];
                if nxt.role == "tool" {
                    if let Some(ref tcid) = nxt.tool_call_id {
                        if let Some(pos) = expected_ids.iter().position(|id| id == tcid) {
                            expected_ids.remove(pos);
                            tool_msgs.push(nxt.clone());
                            j += 1;
                            continue;
                        }
                    }
                }

                if nxt.role == "system" || nxt.role == "developer" {
                    // Move injected system messages before the assistant
                    non_tool_msgs.push(nxt.clone());
                    j += 1;
                    continue;
                }

                // Stop at user/assistant boundary
                break;
            }

            // Error if not last and some expected tool outputs are missing
            let is_last = j >= len;
            if !is_last && !expected_ids.is_empty() {
                let tc = tcs.iter().find(|tc| expected_ids.contains(&tc.id)).unwrap();
                return Err(AppError::new(
                    "TOOL_OUTPUT_NOT_FOUND",
                    format!("Missing tool output for tool call '{}' (function: {})", tc.id, tc.function.name),
                ).with_suggestion("Check that all function_call items have matching function_call_output items in the input"));
            }

            // Put system/developer messages before assistant, then assistant, then tool messages
            reordered.extend(non_tool_msgs);
            reordered.push(msg.clone());
            reordered.extend(tool_msgs);
            i = j;
        } else {
            reordered.push(msg.clone());
            i += 1;
        }
    }

    Ok(reordered)
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
    fn test_convert_tools_local_shell() {
        let tools = vec![json!({"type": "local_shell"})];
        let result = convert_tools(&tools, false);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "function");
        assert_eq!(result[0]["function"]["name"], "shell");
        assert!(result[0]["function"]["parameters"]["properties"]["command"].is_object());
        assert_eq!(result[0]["function"]["parameters"]["required"][0], "command");
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
    fn test_convert_tools_web_search_mimo_minimal() {
        let tools = vec![json!({"type": "web_search_preview"})];
        let result = convert_tools_for_provider(&tools, false, "mimo");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "web_search");
    }

    #[test]
    fn test_convert_tools_mimo_matrix_suppresses_web_search() {
        // Matrix entry exists for the target model but doesn't include web_search
        // → user has opted out, translator must skip emitting web_search.
        let tools = vec![json!({"type": "web_search"})];
        let mut matrix: std::collections::HashMap<String, Vec<String>> = Default::default();
        matrix.insert("mimo-v2.5".to_string(), vec!["text".into(), "vision".into()]);
        let result = convert_tools_with_matrix(&tools, false, "mimo", "mimo-v2.5", &matrix);
        assert!(result.is_empty(), "web_search should be suppressed when matrix says model doesn't have it");
    }

    #[test]
    fn test_convert_tools_mimo_matrix_qualifier_stripped() {
        // model = "mimo-v2.5-pro[1m]" — should look up "mimo-v2.5-pro" in matrix.
        let tools = vec![json!({"type": "web_search"})];
        let mut matrix: std::collections::HashMap<String, Vec<String>> = Default::default();
        matrix.insert("mimo-v2.5-pro".to_string(), vec!["text".into()]);
        let result = convert_tools_with_matrix(&tools, false, "mimo", "mimo-v2.5-pro[1m]", &matrix);
        assert!(result.is_empty(), "[1m] qualifier should be stripped before matrix lookup");
    }

    #[test]
    fn test_convert_tools_mimo_matrix_allows_web_search_when_listed() {
        let tools = vec![json!({"type": "web_search"})];
        let mut matrix: std::collections::HashMap<String, Vec<String>> = Default::default();
        matrix.insert("mimo-v2.5-pro".to_string(), vec!["text".into(), "web_search".into()]);
        let result = convert_tools_with_matrix(&tools, false, "mimo", "mimo-v2.5-pro", &matrix);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "web_search");
    }

    #[test]
    fn test_convert_tools_mimo_empty_matrix_keeps_back_compat() {
        // No matrix configured → keep emitting (existing behavior).
        let tools = vec![json!({"type": "web_search"})];
        let result = convert_tools_with_matrix(&tools, false, "mimo", "mimo-v2.5-pro", &Default::default());
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_convert_tools_mimo_matrix_unknown_model_keeps_emit() {
        // model not in matrix at all → emit (matrix only restricts when entry exists).
        let tools = vec![json!({"type": "web_search"})];
        let mut matrix: std::collections::HashMap<String, Vec<String>> = Default::default();
        matrix.insert("other-model".to_string(), vec!["text".into()]);
        let result = convert_tools_with_matrix(&tools, false, "mimo", "mimo-v2.5-pro", &matrix);
        assert_eq!(result.len(), 1, "unknown model → assume web_search supported");
    }

    #[test]
    fn test_convert_tools_web_search_mimo_preserves_optional_fields() {
        let tools = vec![json!({
            "type": "web_search",
            "max_keyword": 3,
            "force_search": true,
            "limit": 10,
            "user_location": {"country": "CN"}
        })];
        let result = convert_tools_for_provider(&tools, false, "mimo");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["type"], "web_search");
        assert_eq!(result[0]["max_keyword"], 3);
        assert_eq!(result[0]["force_search"], true);
        assert_eq!(result[0]["limit"], 10);
        assert_eq!(result[0]["user_location"]["country"], "CN");
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
        // Tool messages preserved in input order (matching Python codex_proxy behavior)
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
        // Tool messages keep input order (call_2 first, then call_1)
        assert_eq!(result[1].tool_call_id, Some("call_2".to_string()));
        assert_eq!(result[2].tool_call_id, Some("call_1".to_string()));
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

    #[test]
    fn test_fix_tool_message_order_moves_system_before_assistant() {
        // Codex injects system messages (e.g. approval notifications) between
        // assistant(tool_calls) and tool messages. These must be moved before
        // the assistant for DeepSeek compatibility.
        let messages = vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: Some(json!("calling tool")),
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
                role: "system".to_string(),
                content: Some(json!("approval notification")),
                reasoning_content: None,
                tool_calls: None,
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
        assert_eq!(result.len(), 3);
        // System message moved before assistant
        assert_eq!(result[0].role, "system");
        assert_eq!(result[1].role, "assistant");
        assert_eq!(result[2].role, "tool");
    }
}
