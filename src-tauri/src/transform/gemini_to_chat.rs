use serde_json::{json, Value};

use crate::errors::AppError;
use crate::protocol::chat_completions::{ChatCompletionsRequest, ChatMessage, ToolCall, ToolCallFunction};

/// Convert a Gemini API request body to Chat Completions format.
pub fn convert(gemini_body: &Value, model: &str) -> Result<ChatCompletionsRequest, AppError> {
    let mut messages: Vec<ChatMessage> = Vec::new();

    // 1. System instruction → system message
    if let Some(si) = gemini_body.get("systemInstruction") {
        let text = extract_parts_text(si.get("parts"));
        if !text.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: Some(Value::String(text)),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
    }

    // 2. Contents → messages
    if let Some(contents) = gemini_body.get("contents").and_then(|c| c.as_array()) {
        for content in contents {
            let role = content.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let chat_role = if role == "model" { "assistant" } else { "user" };

            let parts = content.get("parts").and_then(|p| p.as_array());
            if parts.is_none() { continue; }
            let parts = parts.unwrap();

            let mut text_parts: Vec<String> = Vec::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut function_responses: Vec<(String, String)> = Vec::new();

            for part in parts {
                // Text part
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(text.to_string());
                }

                // Function call (model calling a tool)
                if let Some(fc) = part.get("functionCall") {
                    let name = fc.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let args = fc.get("args").map(|a| a.to_string()).unwrap_or("{}".to_string());
                    let id = fc.get("id").and_then(|i| i.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| format!("call_{}", tool_calls.len()));
                    tool_calls.push(ToolCall {
                        id,
                        call_type: "function".to_string(),
                        function: ToolCallFunction { name, arguments: args },
                    });
                }

                // Function response (tool result)
                if let Some(fr) = part.get("functionResponse") {
                    let name = fr.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let response = fr.get("response")
                        .map(|r| if r.is_string() { r.as_str().unwrap().to_string() } else { r.to_string() })
                        .unwrap_or_default();
                    function_responses.push((name, response));
                }
            }

            // Emit function responses as tool messages
            if !function_responses.is_empty() {
                for (name, response) in function_responses {
                    messages.push(ChatMessage {
                        role: "tool".to_string(),
                        content: Some(Value::String(response)),
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(format!("call_{name}")),
                        name: Some(name),
                    });
                }
                continue;
            }

            // Emit as regular message
            let text = text_parts.join("");
            messages.push(ChatMessage {
                role: chat_role.to_string(),
                content: if text.is_empty() { None } else { Some(Value::String(text)) },
                reasoning_content: None,
                tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
                tool_call_id: None,
                name: None,
            });
        }
    }

    // 3. Tools → Chat Completions tools
    let tools = gemini_body.get("tools")
        .and_then(|t| t.as_array())
        .map(|tools_arr| {
            let mut result = Vec::new();
            for tool in tools_arr {
                if let Some(decls) = tool.get("functionDeclarations").and_then(|d| d.as_array()) {
                    for decl in decls {
                        let name = decl.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let desc = decl.get("description").and_then(|d| d.as_str()).unwrap_or("");
                        let params = decl.get("parameters").cloned().unwrap_or(json!({"type": "object"}));
                        result.push(json!({
                            "type": "function",
                            "function": {"name": name, "description": desc, "parameters": params}
                        }));
                    }
                }
            }
            result
        })
        .filter(|t| !t.is_empty());

    // 4. Generation config
    let gen = gemini_body.get("generationConfig");
    let temperature = gen.and_then(|g| g.get("temperature")).and_then(|v| v.as_f64());
    let top_p = gen.and_then(|g| g.get("topP")).and_then(|v| v.as_f64());
    let max_tokens = gen.and_then(|g| g.get("maxOutputTokens")).and_then(|v| v.as_i64());
    let stop = gen.and_then(|g| g.get("stopSequences")).cloned();

    // 5. Stream options
    let stream_options = Some(json!({"include_usage": true}));

    Ok(ChatCompletionsRequest {
        model: model.to_string(),
        messages,
        tools,
        tool_choice: None,
        stream: true, // Gemini CLI typically streams
        temperature,
        top_p,
        max_tokens,
        thinking: None,
        stream_options,
        response_format: None,
        reasoning_effort: None,
        seed: None,
        stop,
        frequency_penalty: None,
        presence_penalty: None,
        parallel_tool_calls: None,
    })
}

fn extract_parts_text(parts: Option<&Value>) -> String {
    parts.and_then(|p| p.as_array())
        .map(|arr| arr.iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(""))
        .unwrap_or_default()
}

/// Convert a Chat Completions response to Gemini response format.
pub fn response_to_gemini(chat_resp: &Value, model: &str) -> Value {
    let mut parts: Vec<Value> = Vec::new();

    if let Some(choices) = chat_resp.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            if let Some(msg) = choice.get("message").or(choice.get("delta")) {
                if let Some(text) = msg.get("content").and_then(|c| c.as_str()) {
                    if !text.is_empty() {
                        parts.push(json!({"text": text}));
                    }
                }
                if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tcs {
                        let name = tc.get("function").and_then(|f| f.get("name")).and_then(|n| n.as_str()).unwrap_or("");
                        let args_str = tc.get("function").and_then(|f| f.get("arguments")).and_then(|a| a.as_str()).unwrap_or("{}");
                        let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                        parts.push(json!({"functionCall": {"name": name, "args": args}}));
                    }
                }
            }
        }
    }

    let finish_reason = chat_resp.get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        .map(|r| match r {
            "stop" => "STOP",
            "length" => "MAX_TOKENS",
            "tool_calls" => "STOP",
            _ => "STOP",
        })
        .unwrap_or("STOP");

    let usage = chat_resp.get("usage");

    let mut resp = json!({
        "candidates": [{
            "content": {"role": "model", "parts": parts},
            "finishReason": finish_reason
        }],
        "modelVersion": model
    });

    if let Some(u) = usage {
        resp["usageMetadata"] = json!({
            "promptTokenCount": u.get("prompt_tokens").or(u.get("input_tokens")).and_then(|v| v.as_i64()).unwrap_or(0),
            "candidatesTokenCount": u.get("completion_tokens").or(u.get("output_tokens")).and_then(|v| v.as_i64()).unwrap_or(0),
            "totalTokenCount": u.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0)
        });
    }

    resp
}

/// Convert a Chat Completions SSE chunk to Gemini SSE format.
pub fn chunk_to_gemini(chunk: &Value) -> Option<String> {
    let choices = chunk.get("choices")?.as_array()?;
    let choice = choices.first()?;
    let delta = choice.get("delta")?;

    let mut parts: Vec<Value> = Vec::new();

    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
        if !text.is_empty() {
            parts.push(json!({"text": text}));
        }
    }

    if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
        for tc in tcs {
            if let Some(func) = tc.get("function") {
                let name = func.get("name").and_then(|n| n.as_str());
                let args = func.get("arguments").and_then(|a| a.as_str());
                if let Some(name) = name {
                    let args_val: Value = args.and_then(|a| serde_json::from_str(a).ok()).unwrap_or(json!({}));
                    parts.push(json!({"functionCall": {"name": name, "args": args_val}}));
                }
            }
        }
    }

    if parts.is_empty() {
        // Check for finish_reason or usage
        let finish = choice.get("finish_reason").and_then(|f| f.as_str());
        let usage = chunk.get("usage");
        if finish.is_none() && usage.is_none() {
            return None;
        }

        let mut resp = json!({"candidates": [{}]});
        if let Some(f) = finish {
            let gemini_reason = match f {
                "stop" => "STOP",
                "length" => "MAX_TOKENS",
                "tool_calls" => "STOP",
                _ => "STOP",
            };
            resp["candidates"][0]["finishReason"] = json!(gemini_reason);
        }
        if let Some(u) = usage {
            resp["usageMetadata"] = json!({
                "promptTokenCount": u.get("prompt_tokens").or(u.get("input_tokens")).and_then(|v| v.as_i64()).unwrap_or(0),
                "candidatesTokenCount": u.get("completion_tokens").or(u.get("output_tokens")).and_then(|v| v.as_i64()).unwrap_or(0),
                "totalTokenCount": u.get("total_tokens").and_then(|v| v.as_i64()).unwrap_or(0)
            });
        }
        return Some(format!("data: {}\n\n", resp));
    }

    let resp = json!({"candidates": [{"content": {"role": "model", "parts": parts}}]});
    Some(format!("data: {}\n\n", resp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_simple() {
        let body = json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}]
        });
        let result = convert(&body, "gemini-2.5-flash").unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].role, "user");
        assert_eq!(result.messages[0].content, Some(Value::String("hello".to_string())));
    }

    #[test]
    fn test_convert_system_instruction() {
        let body = json!({
            "systemInstruction": {"parts": [{"text": "Be helpful"}]},
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}]
        });
        let result = convert(&body, "test").unwrap();
        assert_eq!(result.messages[0].role, "system");
        assert_eq!(result.messages[0].content, Some(Value::String("Be helpful".to_string())));
        assert_eq!(result.messages[1].role, "user");
    }

    #[test]
    fn test_convert_model_to_assistant() {
        let body = json!({
            "contents": [
                {"role": "user", "parts": [{"text": "q"}]},
                {"role": "model", "parts": [{"text": "a"}]}
            ]
        });
        let result = convert(&body, "test").unwrap();
        assert_eq!(result.messages[1].role, "assistant");
    }

    #[test]
    fn test_convert_function_call() {
        let body = json!({
            "contents": [
                {"role": "model", "parts": [{"functionCall": {"name": "search", "args": {"q": "hi"}}}]},
                {"role": "user", "parts": [{"functionResponse": {"name": "search", "response": {"result": "found"}}}]}
            ]
        });
        let result = convert(&body, "test").unwrap();
        assert_eq!(result.messages[0].role, "assistant");
        assert!(result.messages[0].tool_calls.is_some());
        assert_eq!(result.messages[1].role, "tool");
    }

    #[test]
    fn test_convert_tools() {
        let body = json!({
            "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
            "tools": [{"functionDeclarations": [{"name": "search", "description": "Search", "parameters": {"type": "object"}}]}]
        });
        let result = convert(&body, "test").unwrap();
        assert!(result.tools.is_some());
        assert_eq!(result.tools.unwrap()[0]["function"]["name"], "search");
    }

    #[test]
    fn test_response_to_gemini() {
        let chat_resp = json!({
            "choices": [{"message": {"content": "hello"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15}
        });
        let result = response_to_gemini(&chat_resp, "gemini-2.5-flash");
        assert_eq!(result["candidates"][0]["content"]["parts"][0]["text"], "hello");
        assert_eq!(result["candidates"][0]["finishReason"], "STOP");
        assert_eq!(result["usageMetadata"]["promptTokenCount"], 10);
    }
}
