use serde_json::{json, Value};

use crate::errors::AppError;
use crate::protocol::openai_responses::ResponsesRequest;

/// Convert a Responses API request into a Claude Messages API request body.
pub fn convert(req: &ResponsesRequest, model: &str) -> Result<Value, AppError> {
    // 1. System prompt (separate field in Claude, NOT in messages)
    let mut system_blocks: Vec<Value> = Vec::new();
    let system_text = req.instructions.as_ref().or(req.system.as_ref());
    if let Some(text) = system_text {
        if !text.is_empty() {
            system_blocks.push(json!({"type": "text", "text": text}));
        }
    }

    // 2. Replay history from previous_response_id
    let mut history_messages: Vec<Value> = Vec::new();
    if let Some(ref prev_id) = req.previous_response_id {
        if let Some(history) = crate::gateway::session_store::get_history(prev_id) {
            for msg in &history {
                let role = match msg.role.as_str() {
                    "system" | "developer" => {
                        // Fold system messages into system blocks
                        if let Some(text) = msg.content.as_ref().and_then(|c| c.as_str()) {
                            if !text.is_empty() {
                                system_blocks.push(json!({"type": "text", "text": text}));
                            }
                        }
                        continue;
                    }
                    "tool" => {
                        // Tool messages become user messages with tool_result
                        let content = msg.content.as_ref()
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let tool_use_id = msg.tool_call_id.as_deref().unwrap_or("");
                        history_messages.push(json!({
                            "role": "user",
                            "content": [{"type": "tool_result", "tool_use_id": tool_use_id, "content": content}]
                        }));
                        continue;
                    }
                    "assistant" => "assistant",
                    _ => "user",
                };

                let mut content_blocks: Vec<Value> = Vec::new();

                // Text content
                if let Some(ref c) = msg.content {
                    let text = c.as_str().unwrap_or("");
                    if !text.is_empty() {
                        content_blocks.push(json!({"type": "text", "text": text}));
                    }
                }

                // Tool calls → tool_use blocks
                if let Some(ref tcs) = msg.tool_calls {
                    for tc in tcs {
                        let input: Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(json!({}));
                        content_blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.function.name,
                            "input": input
                        }));
                    }
                }

                if !content_blocks.is_empty() {
                    history_messages.push(json!({"role": role, "content": content_blocks}));
                }
            }
        }
    }

    // 3. Convert input items to Claude messages
    let input_messages = convert_input(&req.input, &mut system_blocks)?;

    // 4. Combine history + input messages
    let mut all_messages = history_messages;
    all_messages.extend(input_messages);

    // Remove any existing system blocks from history that duplicate instructions
    // (instructions take priority, same as responses_to_chat)

    // 5. Enforce strict user/assistant alternation
    all_messages = merge_consecutive_role_messages(all_messages);

    // 6. Convert tools
    let tools = req.tools.as_ref().map(|t| convert_tools(t));

    // 7. Convert tool_choice
    let tool_choice = req.tool_choice.as_ref().map(convert_tool_choice);

    // 8. max_tokens (required by Claude)
    let max_tokens = req.max_output_tokens.unwrap_or(8192);

    // 9. Thinking configuration
    let thinking = convert_thinking(&req.reasoning);

    // 10. Build request body
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
    });

    if !system_blocks.is_empty() {
        body["system"] = json!(system_blocks);
    }
    body["messages"] = json!(all_messages);

    if let Some(tools) = tools {
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
    }
    if let Some(tc) = tool_choice {
        body["tool_choice"] = tc;
    }
    if let Some(thinking) = thinking {
        body["thinking"] = thinking;
    }
    if let Some(stream) = req.stream {
        body["stream"] = json!(stream);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = json!(temp);
    }
    if let Some(top_p) = req.top_p {
        body["top_p"] = json!(top_p);
    }
    if let Some(ref stop) = req.stop {
        body["stop_sequences"] = stop.clone();
    }

    Ok(body)
}

/// Convert the Responses API `input` field to Claude messages.
fn convert_input(input: &Value, system_blocks: &mut Vec<Value>) -> Result<Vec<Value>, AppError> {
    match input {
        Value::String(s) => {
            Ok(vec![json!({"role": "user", "content": [{"type": "text", "text": s}]})])
        }
        Value::Array(items) => convert_input_array(items, system_blocks),
        Value::Object(_) => {
            let blocks = extract_content_blocks(Some(input));
            if blocks.is_empty() {
                Ok(vec![json!({"role": "user", "content": [{"type": "text", "text": ""}]})])
            } else {
                Ok(vec![json!({"role": "user", "content": blocks})])
            }
        }
        _ => {
            Ok(vec![json!({"role": "user", "content": [{"type": "text", "text": input.to_string()}]})])
        }
    }
}

fn convert_input_array(items: &[Value], system_blocks: &mut Vec<Value>) -> Result<Vec<Value>, AppError> {
    let mut messages: Vec<Value> = Vec::new();
    let mut pending_tool_uses: Vec<Value> = Vec::new();
    let mut pending_text: Option<String> = None;

    for item in items {
        let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match item_type {
            "message" => {
                // Flush pending tool calls
                flush_tool_uses(&mut messages, &mut pending_tool_uses, &mut pending_text);

                let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");

                match role {
                    "system" | "developer" => {
                        let text = extract_text(item.get("content"));
                        if !text.is_empty() {
                            system_blocks.push(json!({"type": "text", "text": text}));
                        }
                    }
                    "assistant" => {
                        let mut content_blocks: Vec<Value> = Vec::new();
                        let text = extract_text(item.get("content"));
                        if !text.is_empty() {
                            content_blocks.push(json!({"type": "text", "text": text}));
                        }

                        // Check for embedded tool_calls in content array
                        if let Some(Value::Array(parts)) = item.get("content") {
                            for part in parts {
                                if part.get("type").and_then(|t| t.as_str()) == Some("tool_call") {
                                    let id = part.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                                    let name = part.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                    let args = part.get("arguments").map(|a| {
                                        if a.is_string() { a.as_str().unwrap().to_string() }
                                        else { a.to_string() }
                                    }).unwrap_or_default();
                                    let input: Value = serde_json::from_str(&args).unwrap_or(json!({}));
                                    content_blocks.push(json!({
                                        "type": "tool_use", "id": id, "name": name, "input": input
                                    }));
                                }
                            }
                        }

                        if !content_blocks.is_empty() {
                            messages.push(json!({"role": "assistant", "content": content_blocks}));
                        }
                    }
                    _ => {
                        // user — preserve images
                        let blocks = extract_content_blocks(item.get("content"));
                        if !blocks.is_empty() {
                            messages.push(json!({"role": "user", "content": blocks}));
                        }
                    }
                }
            }
            "function_call" => {
                let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("call_unknown");
                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let arguments = item.get("arguments").map(|a| {
                    if a.is_string() { a.as_str().unwrap().to_string() }
                    else { a.to_string() }
                }).unwrap_or_default();
                let input: Value = serde_json::from_str(&arguments).unwrap_or(json!({}));

                pending_tool_uses.push(json!({
                    "type": "tool_use",
                    "id": call_id,
                    "name": name,
                    "input": input
                }));
            }
            "function_call_output" => {
                // Flush pending tool calls first
                flush_tool_uses(&mut messages, &mut pending_tool_uses, &mut pending_text);

                let call_id = item.get("call_id").and_then(|c| c.as_str());
                if call_id.is_none() || call_id == Some("") {
                    return Err(AppError::new(
                        "FUNCTION_CALL_OUTPUT_ID_MISSING",
                        "function_call_output is missing call_id",
                    ).with_suggestion("Each function_call_output must have a call_id matching a previous function_call"));
                }

                let output = item.get("output").map(|o| {
                    if o.is_string() { o.as_str().unwrap().to_string() }
                    else if o.is_array() {
                        // Flatten ContentPart array
                        crate::transform::responses_to_chat::flatten_tool_output(o)
                    } else {
                        o.to_string()
                    }
                }).unwrap_or_default();

                messages.push(json!({
                    "role": "user",
                    "content": [{"type": "tool_result", "tool_use_id": call_id.unwrap(), "content": output}]
                }));
            }
            "reasoning" => {
                // Skip reasoning items — Claude handles thinking internally
            }
            "compaction" | "context_compaction" | "compaction_summary" => {
                flush_tool_uses(&mut messages, &mut pending_tool_uses, &mut pending_text);
                let summary = item.get("summary")
                    .or(item.get("content"))
                    .map(|v| extract_text(Some(v)))
                    .unwrap_or_else(|| "[context compacted]".to_string());
                messages.push(json!({"role": "user", "content": [{"type": "text", "text": summary}]}));
            }
            _ => {
                // Unknown item: try to extract as message if it has role/content
                if let Some(role) = item.get("role").and_then(|r| r.as_str()) {
                    flush_tool_uses(&mut messages, &mut pending_tool_uses, &mut pending_text);
                    let mapped_role = if role == "assistant" { "assistant" } else { "user" };
                    let text = extract_text(item.get("content"));
                    if !text.is_empty() {
                        messages.push(json!({"role": mapped_role, "content": [{"type": "text", "text": text}]}));
                    }
                }
            }
        }
    }

    flush_tool_uses(&mut messages, &mut pending_tool_uses, &mut pending_text);
    Ok(messages)
}

fn flush_tool_uses(messages: &mut Vec<Value>, pending: &mut Vec<Value>, _text: &mut Option<String>) {
    if pending.is_empty() {
        return;
    }
    messages.push(json!({"role": "assistant", "content": std::mem::take(pending)}));
}

fn extract_text(content: Option<&Value>) -> String {
    match content {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => {
            arr.iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()).map(String::from))
                .collect::<Vec<_>>()
                .join("")
        }
        Some(Value::Object(obj)) => {
            obj.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string()
        }
        Some(other) => other.to_string(),
    }
}

/// Extract content blocks for Claude Messages API, preserving images.
/// Returns Vec of content blocks (text + image blocks).
fn extract_content_blocks(content: Option<&Value>) -> Vec<Value> {
    match content {
        None => vec![],
        Some(Value::String(s)) => {
            if s.is_empty() { vec![] } else { vec![json!({"type": "text", "text": s})] }
        }
        Some(Value::Array(arr)) => {
            let mut blocks = Vec::new();
            for part in arr {
                let pt = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match pt {
                    "input_text" | "output_text" | "text" => {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                blocks.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                    "input_image" => {
                        // Convert to Claude image block format
                        if let Some(url) = part.get("image_url").and_then(|u| u.as_str())
                            .or_else(|| part.get("image_url").and_then(|u| u.get("url")).and_then(|u| u.as_str()))
                        {
                            if let Some(b64_data) = url.strip_prefix("data:") {
                                // Parse data URI: data:image/png;base64,<data>
                                if let Some((media_info, data)) = b64_data.split_once(',') {
                                    let media_type = media_info.replace(";base64", "");
                                    blocks.push(json!({
                                        "type": "image",
                                        "source": {
                                            "type": "base64",
                                            "media_type": media_type,
                                            "data": data
                                        }
                                    }));
                                }
                            } else {
                                // URL-based image
                                blocks.push(json!({
                                    "type": "image",
                                    "source": {"type": "url", "url": url}
                                }));
                            }
                        }
                    }
                    "image_url" => {
                        // OpenAI Chat Completions format image_url
                        if let Some(url) = part.get("image_url").and_then(|u| u.get("url")).and_then(|u| u.as_str()) {
                            if let Some(b64_data) = url.strip_prefix("data:") {
                                if let Some((media_info, data)) = b64_data.split_once(',') {
                                    let media_type = media_info.replace(";base64", "");
                                    blocks.push(json!({
                                        "type": "image",
                                        "source": {
                                            "type": "base64",
                                            "media_type": media_type,
                                            "data": data
                                        }
                                    }));
                                }
                            } else {
                                blocks.push(json!({
                                    "type": "image",
                                    "source": {"type": "url", "url": url}
                                }));
                            }
                        }
                    }
                    "tool_call" => {} // handled separately
                    _ => {
                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                blocks.push(json!({"type": "text", "text": text}));
                            }
                        }
                    }
                }
            }
            blocks
        }
        Some(Value::Object(obj)) => {
            if let Some(text) = obj.get("text").and_then(|t| t.as_str()) {
                vec![json!({"type": "text", "text": text})]
            } else {
                vec![]
            }
        }
        Some(_) => vec![],
    }
}

/// Merge consecutive messages with the same role.
/// Claude requires strict user/assistant alternation.
fn merge_consecutive_role_messages(messages: Vec<Value>) -> Vec<Value> {
    let mut result: Vec<Value> = Vec::new();

    for msg in messages {
        if let Some(last) = result.last_mut() {
            let last_role = last.get("role").and_then(|r| r.as_str()).unwrap_or("");
            let msg_role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

            if last_role == msg_role {
                // Merge content arrays
                if let (Some(last_content), Some(msg_content)) = (
                    last.get_mut("content").and_then(|c| c.as_array_mut()),
                    msg.get("content").and_then(|c| c.as_array()),
                ) {
                    last_content.extend(msg_content.iter().cloned());
                    continue;
                }
            }
        }
        result.push(msg);
    }

    result
}

/// Convert Responses API tools to Claude tool definitions.
fn convert_tools(tools: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();

    for tool in tools {
        let tool_type = tool.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match tool_type {
            "function" => {
                if let Some(func) = tool.get("function") {
                    // Structure B: {type: "function", function: {name, description, parameters}}
                    let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let desc = func.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    let params = func.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}}));
                    result.push(json!({
                        "name": name,
                        "description": desc,
                        "input_schema": params
                    }));
                } else {
                    // Structure A: flat {type: "function", name, description, parameters}
                    let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let desc = tool.get("description").and_then(|d| d.as_str()).unwrap_or("");
                    let params = tool.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}}));
                    result.push(json!({
                        "name": name,
                        "description": desc,
                        "input_schema": params
                    }));
                }
            }
            "namespace" => {
                let ns_name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if let Some(Value::Array(sub_tools)) = tool.get("tools") {
                    for sub in sub_tools {
                        if sub.get("type").and_then(|t| t.as_str()) == Some("function") {
                            let name = sub.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let desc = sub.get("description").and_then(|d| d.as_str()).unwrap_or("");
                            let params = sub.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}}));
                            let prefixed = if ns_name.is_empty() { name.to_string() } else { format!("{ns_name}__{name}") };
                            result.push(json!({
                                "name": prefixed,
                                "description": desc,
                                "input_schema": params
                            }));
                        }
                    }
                }
            }
            "custom" => {
                let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("custom_tool");
                let desc = tool.get("description").and_then(|d| d.as_str()).unwrap_or("");
                result.push(json!({
                    "name": name,
                    "description": desc,
                    "input_schema": {"type": "object", "properties": {"input": {"type": "string"}}, "required": ["input"]}
                }));
            }
            "local_shell" => {
                result.push(json!({
                    "name": "shell",
                    "description": "Execute a shell command on the local machine. Returns stdout, stderr and exit code.",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "command": {"type": "array", "items": {"type": "string"}, "description": "Argv array"},
                            "workdir": {"type": "string", "description": "Working directory (optional)."},
                            "timeout_ms": {"type": "number", "description": "Timeout in milliseconds (optional)."}
                        },
                        "required": ["command"]
                    }
                }));
            }
            _ => {
                // Skip web_search, code_interpreter, file_search, etc.
            }
        }
    }

    result
}

/// Convert Responses API tool_choice to Claude format.
fn convert_tool_choice(tc: &Value) -> Value {
    match tc {
        Value::String(s) => match s.as_str() {
            "auto" | "none" => json!({"type": "auto"}),
            "required" | "any" => json!({"type": "any"}),
            _ => json!({"type": "auto"}),
        },
        Value::Object(obj) => {
            // {name: "X"} or {function: {name: "X"}}
            let name = obj.get("name").and_then(|n| n.as_str())
                .or_else(|| obj.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()));
            if let Some(name) = name {
                json!({"type": "tool", "name": name})
            } else {
                json!({"type": "auto"})
            }
        }
        _ => json!({"type": "auto"}),
    }
}

/// Convert reasoning effort to Claude thinking configuration.
fn convert_thinking(reasoning: &Option<Value>) -> Option<Value> {
    let effort = reasoning.as_ref()
        .and_then(|r| r.get("effort"))
        .and_then(|e| e.as_str())?;

    let budget = match effort.trim().to_ascii_lowercase().as_str() {
        "low" | "minimal" => 4096,
        "medium" => 8192,
        "high" => 16384,
        "xhigh" | "max" | "highest" => 32768,
        "none" | "off" | "auto" | "" => return None,
        _ => return None,
    };

    Some(json!({"type": "enabled", "budget_tokens": budget}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_req(input: Value) -> ResponsesRequest {
        ResponsesRequest {
            model: Some("claude-3-5-sonnet".to_string()),
            input,
            instructions: None,
            system: None,
            previous_response_id: None,
            tools: None,
            tool_choice: None,
            stream: None,
            temperature: None,
            top_p: None,
            max_output_tokens: None,
            ..Default::default()
        }
    }

    #[test]
    fn test_convert_simple_string_input() {
        let req = make_req(json!("hello"));
        let result = convert(&req, "claude-3-5-sonnet").unwrap();
        assert_eq!(result["model"], "claude-3-5-sonnet");
        assert_eq!(result["max_tokens"], 8192);
        let msgs = result["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"][0]["type"], "text");
        assert_eq!(msgs[0]["content"][0]["text"], "hello");
    }

    #[test]
    fn test_convert_with_instructions() {
        let mut req = make_req(json!("hello"));
        req.instructions = Some("Be helpful".to_string());
        let result = convert(&req, "claude-3-5-sonnet").unwrap();
        let sys = result["system"].as_array().unwrap();
        assert_eq!(sys.len(), 1);
        assert_eq!(sys[0]["text"], "Be helpful");
    }

    #[test]
    fn test_convert_function_call_to_tool_use() {
        let req = make_req(json!([
            {"type": "function_call", "call_id": "call_1", "name": "search", "arguments": "{\"q\":\"hi\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "result"}
        ]));
        let result = convert(&req, "claude-3-5-sonnet").unwrap();
        let msgs = result["messages"].as_array().unwrap();
        // assistant with tool_use, then user with tool_result
        assert_eq!(msgs[0]["role"], "assistant");
        assert_eq!(msgs[0]["content"][0]["type"], "tool_use");
        assert_eq!(msgs[0]["content"][0]["id"], "call_1");
        assert_eq!(msgs[0]["content"][0]["name"], "search");
        assert_eq!(msgs[0]["content"][0]["input"]["q"], "hi");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(msgs[1]["content"][0]["type"], "tool_result");
        assert_eq!(msgs[1]["content"][0]["tool_use_id"], "call_1");
    }

    #[test]
    fn test_convert_tool_choice() {
        assert_eq!(convert_tool_choice(&json!("auto")), json!({"type": "auto"}));
        assert_eq!(convert_tool_choice(&json!("required")), json!({"type": "any"}));
        assert_eq!(convert_tool_choice(&json!({"name": "search"})), json!({"type": "tool", "name": "search"}));
    }

    #[test]
    fn test_convert_tools_parameters_to_input_schema() {
        let tools = vec![json!({
            "type": "function",
            "function": {
                "name": "search",
                "description": "Search the web",
                "parameters": {"type": "object", "properties": {"q": {"type": "string"}}}
            }
        })];
        let result = convert_tools(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "search");
        assert!(result[0]["input_schema"].is_object());
        assert!(result[0].get("parameters").is_none());
    }

    #[test]
    fn test_convert_max_tokens_default() {
        let req = make_req(json!("hi"));
        let result = convert(&req, "claude-3-5-sonnet").unwrap();
        assert_eq!(result["max_tokens"], 8192);
    }

    #[test]
    fn test_convert_max_tokens_custom() {
        let mut req = make_req(json!("hi"));
        req.max_output_tokens = Some(4096);
        let result = convert(&req, "claude-3-5-sonnet").unwrap();
        assert_eq!(result["max_tokens"], 4096);
    }

    #[test]
    fn test_convert_thinking() {
        assert_eq!(convert_thinking(&Some(json!({"effort": "low"}))), Some(json!({"type": "enabled", "budget_tokens": 4096})));
        assert_eq!(convert_thinking(&Some(json!({"effort": "medium"}))), Some(json!({"type": "enabled", "budget_tokens": 8192})));
        assert_eq!(convert_thinking(&Some(json!({"effort": "high"}))), Some(json!({"type": "enabled", "budget_tokens": 16384})));
        assert_eq!(convert_thinking(&Some(json!({"effort": "auto"}))), None);
        assert_eq!(convert_thinking(&None), None);
    }

    #[test]
    fn test_merge_consecutive_user_messages() {
        let messages = vec![
            json!({"role": "user", "content": [{"type": "text", "text": "a"}]}),
            json!({"role": "user", "content": [{"type": "text", "text": "b"}]}),
        ];
        let merged = merge_consecutive_role_messages(messages);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_system_role_message_folded_into_system() {
        let req = make_req(json!([
            {"type": "message", "role": "system", "content": "sys prompt"},
            {"type": "message", "role": "user", "content": "hello"}
        ]));
        let result = convert(&req, "claude-3-5-sonnet").unwrap();
        let sys = result["system"].as_array().unwrap();
        assert_eq!(sys[0]["text"], "sys prompt");
        // Messages should only have user, no system
        let msgs = result["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn test_missing_call_id_errors() {
        let req = make_req(json!([
            {"type": "function_call_output", "call_id": "", "output": "result"}
        ]));
        assert!(convert(&req, "claude-3-5-sonnet").is_err());
    }

    // ── extract_content_blocks image tests ──

    #[test]
    fn test_extract_content_blocks_text_only() {
        let content = json!([{"type": "input_text", "text": "hello"}]);
        let blocks = extract_content_blocks(Some(&content));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "hello");
    }

    #[test]
    fn test_extract_content_blocks_with_base64_image() {
        let content = json!([
            {"type": "input_text", "text": "describe"},
            {"type": "input_image", "image_url": "data:image/png;base64,abc123"}
        ]);
        let blocks = extract_content_blocks(Some(&content));
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["type"], "base64");
        assert_eq!(blocks[1]["source"]["media_type"], "image/png");
        assert_eq!(blocks[1]["source"]["data"], "abc123");
    }

    #[test]
    fn test_extract_content_blocks_with_url_image() {
        let content = json!([
            {"type": "input_image", "image_url": "https://example.com/photo.jpg"}
        ]);
        let blocks = extract_content_blocks(Some(&content));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["type"], "url");
        assert_eq!(blocks[0]["source"]["url"], "https://example.com/photo.jpg");
    }

    #[test]
    fn test_extract_content_blocks_image_url_type() {
        let content = json!([
            {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64,xyz"}}
        ]);
        let blocks = extract_content_blocks(Some(&content));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["type"], "image");
        assert_eq!(blocks[0]["source"]["media_type"], "image/jpeg");
        assert_eq!(blocks[0]["source"]["data"], "xyz");
    }

    #[test]
    fn test_extract_content_blocks_string_input() {
        let blocks = extract_content_blocks(Some(&json!("hello")));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["text"], "hello");
    }

    #[test]
    fn test_extract_content_blocks_none() {
        let blocks = extract_content_blocks(None);
        assert!(blocks.is_empty());
    }
}
