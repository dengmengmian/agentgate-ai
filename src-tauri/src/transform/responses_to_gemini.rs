use serde_json::{json, Value};

use crate::errors::AppError;
use crate::protocol::openai_responses::ResponsesRequest;

/// Convert a Responses API request into a Gemini API request body.
pub fn convert(req: &ResponsesRequest, _model: &str) -> Result<Value, AppError> {
    // 1. System instruction (separate field in Gemini, text only)
    let system_instruction = req
        .instructions
        .as_ref()
        .or(req.system.as_ref())
        .filter(|s| !s.is_empty())
        .map(|text| json!({"parts": [{"text": text}]}));

    // 2. Replay history from previous_response_id
    let mut contents: Vec<Value> = Vec::new();
    if let Some(ref prev_id) = req.previous_response_id {
        if let Some(history) = crate::gateway::session_store::get_history(prev_id) {
            for msg in &history {
                let role = match msg.role.as_str() {
                    "system" | "developer" => continue, // System goes to system_instruction
                    "assistant" => "model",
                    "tool" => {
                        // Tool results → user message with functionResponse part
                        let content = msg.content.as_ref().and_then(|c| c.as_str()).unwrap_or("");
                        let name = msg.name.as_deref().unwrap_or("unknown");
                        contents.push(json!({
                            "role": "user",
                            "parts": [{"functionResponse": {"name": name, "response": {"result": content}}}]
                        }));
                        continue;
                    }
                    _ => "user",
                };

                let mut parts: Vec<Value> = Vec::new();

                // Text content
                if let Some(ref c) = msg.content {
                    let text = c.as_str().unwrap_or("");
                    if !text.is_empty() {
                        parts.push(json!({"text": text}));
                    }
                }

                // Tool calls → functionCall parts
                if let Some(ref tcs) = msg.tool_calls {
                    for tc in tcs {
                        let args: Value =
                            serde_json::from_str(&tc.function.arguments).unwrap_or(json!({}));
                        parts.push(json!({
                            "functionCall": {
                                "name": tc.function.name,
                                "args": args
                            }
                        }));
                    }
                }

                if !parts.is_empty() {
                    contents.push(json!({"role": role, "parts": parts}));
                }
            }
        }
    }

    // 3. Convert input items to Gemini contents
    let input_contents = convert_input(&req.input)?;
    contents.extend(input_contents);

    // 4. Merge consecutive same-role contents (Gemini requires alternation)
    contents = merge_consecutive_roles(contents);

    // 5. Convert tools
    let tools = req
        .tools
        .as_ref()
        .map(|t| convert_tools(t))
        .filter(|t| !t.is_empty());

    // 6. Generation config
    let mut gen_config = json!({});
    if let Some(temp) = req.temperature {
        gen_config["temperature"] = json!(temp);
    }
    if let Some(top_p) = req.top_p {
        gen_config["topP"] = json!(top_p);
    }
    if let Some(max_tokens) = req.max_output_tokens {
        gen_config["maxOutputTokens"] = json!(max_tokens);
    }
    if let Some(ref stop) = req.stop {
        if let Some(arr) = stop.as_array() {
            let seqs: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if !seqs.is_empty() {
                gen_config["stopSequences"] = json!(seqs);
            }
        }
    }

    // Thinking config from reasoning.effort
    if let Some(ref reasoning) = req.reasoning {
        if let Some(effort) = reasoning.get("effort").and_then(|e| e.as_str()) {
            let level = match effort.trim().to_ascii_lowercase().as_str() {
                "low" | "minimal" => "low",
                "medium" => "medium",
                "high" | "xhigh" | "max" => "high",
                _ => "",
            };
            if !level.is_empty() {
                gen_config["thinkingConfig"] = json!({"thinkingLevel": level});
            }
        }
    }

    // 7. Build request body
    let mut body = json!({"contents": contents});

    if let Some(si) = system_instruction {
        body["systemInstruction"] = si;
    }
    if let Some(tools) = tools {
        body["tools"] = json!([{"functionDeclarations": tools}]);
    }
    if gen_config.as_object().map_or(false, |o| !o.is_empty()) {
        body["generationConfig"] = gen_config;
    }

    Ok(body)
}

fn convert_input(input: &Value) -> Result<Vec<Value>, AppError> {
    match input {
        Value::String(s) => Ok(vec![json!({"role": "user", "parts": [{"text": s}]})]),
        Value::Array(items) => convert_input_array(items),
        Value::Object(_) => {
            let text = extract_text(Some(input));
            Ok(vec![json!({"role": "user", "parts": [{"text": text}]})])
        }
        _ => Ok(vec![
            json!({"role": "user", "parts": [{"text": input.to_string()}]}),
        ]),
    }
}

fn convert_input_array(items: &[Value]) -> Result<Vec<Value>, AppError> {
    let mut contents: Vec<Value> = Vec::new();
    let mut pending_function_calls: Vec<Value> = Vec::new();

    for item in items {
        let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match item_type {
            "message" => {
                flush_function_calls(&mut contents, &mut pending_function_calls);

                let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                match role {
                    "system" | "developer" => continue, // Handled via system_instruction
                    _ => {
                        let gemini_role = if role == "assistant" { "model" } else { "user" };
                        let text = extract_text(item.get("content"));
                        if !text.is_empty() {
                            contents.push(json!({"role": gemini_role, "parts": [{"text": text}]}));
                        }
                    }
                }
            }
            "function_call" => {
                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let arguments = item
                    .get("arguments")
                    .map(|a| {
                        if a.is_string() {
                            a.as_str().unwrap().to_string()
                        } else {
                            a.to_string()
                        }
                    })
                    .unwrap_or_default();
                let args: Value = serde_json::from_str(&arguments).unwrap_or(json!({}));

                pending_function_calls.push(json!({
                    "functionCall": {"name": name, "args": args}
                }));
            }
            "function_call_output" => {
                flush_function_calls(&mut contents, &mut pending_function_calls);

                let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("");
                if call_id.is_empty() {
                    return Err(AppError::new(
                        crate::errors::codes::FUNCTION_CALL_OUTPUT_ID_MISSING,
                        "function_call_output is missing call_id",
                    ));
                }

                let name = item.get("name").and_then(|n| n.as_str()).unwrap_or(call_id);
                let output = item
                    .get("output")
                    .map(|o| {
                        if o.is_string() {
                            o.as_str().unwrap().to_string()
                        } else {
                            o.to_string()
                        }
                    })
                    .unwrap_or_default();

                contents.push(json!({
                    "role": "user",
                    "parts": [{"functionResponse": {"name": name, "response": {"result": output}}}]
                }));
            }
            "reasoning" | "compaction" | "context_compaction" | "compaction_summary" => {
                // Skip reasoning items; handle compaction as user message
                if item_type.contains("compaction") {
                    flush_function_calls(&mut contents, &mut pending_function_calls);
                    let summary = item
                        .get("summary")
                        .or(item.get("content"))
                        .map(|v| extract_text(Some(v)))
                        .unwrap_or_else(|| "[context compacted]".to_string());
                    contents.push(json!({"role": "user", "parts": [{"text": summary}]}));
                }
            }
            _ => {
                if let Some(role) = item.get("role").and_then(|r| r.as_str()) {
                    flush_function_calls(&mut contents, &mut pending_function_calls);
                    let gemini_role = if role == "assistant" { "model" } else { "user" };
                    let text = extract_text(item.get("content"));
                    if !text.is_empty() {
                        contents.push(json!({"role": gemini_role, "parts": [{"text": text}]}));
                    }
                }
            }
        }
    }

    flush_function_calls(&mut contents, &mut pending_function_calls);
    Ok(contents)
}

fn flush_function_calls(contents: &mut Vec<Value>, pending: &mut Vec<Value>) {
    if pending.is_empty() {
        return;
    }
    contents.push(json!({"role": "model", "parts": std::mem::take(pending)}));
}

fn extract_text(content: Option<&Value>) -> String {
    match content {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()).map(String::from))
            .collect::<Vec<_>>()
            .join(""),
        Some(Value::Object(obj)) => obj
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        Some(other) => other.to_string(),
    }
}

/// Merge consecutive same-role contents (Gemini requires user/model alternation).
fn merge_consecutive_roles(contents: Vec<Value>) -> Vec<Value> {
    let mut result: Vec<Value> = Vec::new();

    for msg in contents {
        if let Some(last) = result.last_mut() {
            let last_role = last.get("role").and_then(|r| r.as_str()).unwrap_or("");
            let msg_role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

            if last_role == msg_role {
                if let (Some(last_parts), Some(msg_parts)) = (
                    last.get_mut("parts").and_then(|p| p.as_array_mut()),
                    msg.get("parts").and_then(|p| p.as_array()),
                ) {
                    last_parts.extend(msg_parts.iter().cloned());
                    continue;
                }
            }
        }
        result.push(msg);
    }

    result
}

/// Convert Responses API tools to Gemini functionDeclarations.
fn convert_tools(tools: &[Value]) -> Vec<Value> {
    let mut declarations = Vec::new();

    for tool in tools {
        let tool_type = tool.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match tool_type {
            "function" => {
                if let Some(func) = tool.get("function") {
                    let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let desc = func
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");
                    let params = func
                        .get("parameters")
                        .cloned()
                        .unwrap_or(json!({"type": "object", "properties": {}}));
                    declarations.push(json!({
                        "name": name,
                        "description": desc,
                        "parameters": params
                    }));
                } else {
                    // Flat structure
                    let name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let desc = tool
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or("");
                    let params = tool
                        .get("parameters")
                        .cloned()
                        .unwrap_or(json!({"type": "object", "properties": {}}));
                    declarations.push(json!({
                        "name": name,
                        "description": desc,
                        "parameters": params
                    }));
                }
            }
            "local_shell" => {
                declarations.push(json!({
                    "name": "shell",
                    "description": "Execute a shell command on the local machine.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": {"type": "array", "items": {"type": "string"}, "description": "Argv array"},
                            "workdir": {"type": "string", "description": "Working directory (optional)"}
                        },
                        "required": ["command"]
                    }
                }));
            }
            "namespace" => {
                let ns_name = tool.get("name").and_then(|n| n.as_str()).unwrap_or("");
                if let Some(Value::Array(sub_tools)) = tool.get("tools") {
                    for sub in sub_tools {
                        if sub.get("type").and_then(|t| t.as_str()) == Some("function") {
                            let name = sub.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            let desc = sub
                                .get("description")
                                .and_then(|d| d.as_str())
                                .unwrap_or("");
                            let params = sub
                                .get("parameters")
                                .cloned()
                                .unwrap_or(json!({"type": "object", "properties": {}}));
                            let prefixed = if ns_name.is_empty() {
                                name.to_string()
                            } else {
                                format!("{ns_name}__{name}")
                            };
                            declarations.push(json!({"name": prefixed, "description": desc, "parameters": params}));
                        }
                    }
                }
            }
            _ => {} // Skip web_search, custom, etc.
        }
    }

    crate::transform::tool_calls::dedupe_tools_by_name(declarations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_req(input: Value) -> ResponsesRequest {
        ResponsesRequest {
            model: Some("gemini-2.5-flash".to_string()),
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
    fn test_convert_simple_string() {
        let req = make_req(json!("hello"));
        let result = convert(&req, "gemini-2.5-flash").unwrap();
        let contents = result["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "hello");
    }

    #[test]
    fn test_convert_with_system() {
        let mut req = make_req(json!("hello"));
        req.instructions = Some("Be helpful".to_string());
        let result = convert(&req, "gemini-2.5-flash").unwrap();
        assert_eq!(
            result["systemInstruction"]["parts"][0]["text"],
            "Be helpful"
        );
    }

    #[test]
    fn test_convert_role_mapping() {
        let req = make_req(json!([
            {"type": "message", "role": "user", "content": "q"},
            {"type": "message", "role": "assistant", "content": "a"}
        ]));
        let result = convert(&req, "gemini-2.5-flash").unwrap();
        let contents = result["contents"].as_array().unwrap();
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[1]["role"], "model"); // assistant → model
    }

    #[test]
    fn test_convert_function_call() {
        let req = make_req(json!([
            {"type": "function_call", "call_id": "c1", "name": "search", "arguments": "{\"q\":\"hi\"}"},
            {"type": "function_call_output", "call_id": "c1", "name": "search", "output": "result"}
        ]));
        let result = convert(&req, "gemini-2.5-flash").unwrap();
        let contents = result["contents"].as_array().unwrap();
        // model message with functionCall
        assert_eq!(contents[0]["role"], "model");
        assert!(contents[0]["parts"][0].get("functionCall").is_some());
        // user message with functionResponse
        assert_eq!(contents[1]["role"], "user");
        assert!(contents[1]["parts"][0].get("functionResponse").is_some());
    }

    #[test]
    fn test_convert_tools() {
        let tools = vec![json!({
            "type": "function",
            "function": {"name": "search", "description": "Search", "parameters": {"type": "object"}}
        })];
        let result = convert_tools(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "search");
        assert!(result[0].get("parameters").is_some());
    }

    #[test]
    fn test_convert_tools_dedupes_duplicate_names() {
        let tools = vec![
            json!({"type": "function", "function": {"name": "search", "description": "A", "parameters": {"type": "object"}}}),
            json!({"type": "function", "function": {"name": "search", "description": "B", "parameters": {"type": "object"}}}),
        ];
        let result = convert_tools(&tools);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["description"], "A");
    }

    #[test]
    fn test_convert_generation_config() {
        let mut req = make_req(json!("hi"));
        req.temperature = Some(0.7);
        req.top_p = Some(0.9);
        req.max_output_tokens = Some(4096);
        let result = convert(&req, "gemini-2.5-flash").unwrap();
        assert_eq!(result["generationConfig"]["temperature"], 0.7);
        assert_eq!(result["generationConfig"]["topP"], 0.9);
        assert_eq!(result["generationConfig"]["maxOutputTokens"], 4096);
    }

    #[test]
    fn test_merge_consecutive_roles() {
        let contents = vec![
            json!({"role": "user", "parts": [{"text": "a"}]}),
            json!({"role": "user", "parts": [{"text": "b"}]}),
        ];
        let merged = merge_consecutive_roles(contents);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["parts"].as_array().unwrap().len(), 2);
    }
}
