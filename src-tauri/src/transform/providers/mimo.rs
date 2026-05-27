use crate::errors::AppError;
use crate::protocol::chat_completions::{ChatCompletionsRequest, ChatMessage};
use crate::transform::{reasoning_store, tool_calls};
use serde_json::Value;

pub struct MimoProvider;

// Per https://platform.xiaomimimo.com/docs/zh-CN/api/chat/openai-api :
// "在思考模式下，mimo-v2.5-pro 与 mimo-v2.5 模型不支持自定义 temperature 参数。
//  即使传入该参数，实际生效值也会被模型强制采用其推荐默认值 1.0。"
const THINKING_STRIPS_TEMPERATURE: &[&str] = &["mimo-v2.5-pro", "mimo-v2.5"];

// Models that don't support MiMo's native web_search builtin. The translator
// emits `{"type": "web_search"}` for any MiMo target, but these models 400
// upstream with "webSearchEnabled is false" because the capability isn't
// available on that endpoint. mimo-v2-omni explicitly lacks web_search per
// the official model page (Omni row has multimodal + tools but no web_search).
const MIMO_NO_WEB_SEARCH: &[&str] = &["mimo-v2-omni"];

fn strip_qualifier(model: &str) -> &str {
    if let Some(stripped) = model.strip_suffix(']') {
        if let Some(open) = stripped.rfind('[') {
            return &stripped[..open];
        }
    }
    model
}

impl super::ProviderTransform for MimoProvider {
    fn process_messages(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<Vec<ChatMessage>, AppError> {
        let mut messages = tool_calls::fix_tool_message_order(messages)?;

        // MiMo thinking-mode multi-turn invariant: every assistant message
        // carrying tool_calls MUST also carry reasoning_content. Without this,
        // the model 400s ("The reasoning_content in the thinking mode must be
        // passed back") or silently degrades into narration without tool use.
        for msg in &mut messages {
            if msg.role == "assistant"
                && msg.tool_calls.is_some()
                && msg.reasoning_content.is_none()
            {
                let text = msg
                    .content
                    .as_ref()
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let stored = reasoning_store::lookup_by_content(text).or_else(|| {
                    msg.tool_calls.as_ref().and_then(|tcs| {
                        tcs.iter()
                            .find_map(|tc| reasoning_store::lookup_by_tool_call_id(&tc.id))
                    })
                });
                msg.reasoning_content = stored.or_else(|| Some(" ".to_string()));
            }
        }

        Ok(messages)
    }

    fn finalize_request(&self, req: &mut ChatCompletionsRequest, _tools: &Option<Vec<Value>>) {
        // Strip [1m]/[...] qualifier before comparing against the capability lists.
        let base_model = strip_qualifier(req.model.as_str()).to_string();
        let model = base_model.as_str();

        // Strip temperature in thinking mode for v2.5-pro / v2.5 — upstream
        // forces it to 1.0 anyway.
        if let Some(t) = &req.thinking {
            let enabled = t.get("type").and_then(|v| v.as_str()) == Some("enabled");
            if enabled && THINKING_STRIPS_TEMPERATURE.contains(&model) {
                req.temperature = None;
            }
        }

        // tool_choice non-"auto" values are dropped by MiMo's backend; strip
        // client-side to keep the request body honest.
        if let Some(tc) = &req.tool_choice {
            if tc.as_str() != Some("auto") {
                req.tool_choice = None;
            }
        }

        // MiMo's reasoning_effort schema only accepts low/medium/high; "none"
        // is a SenseNova extension and 400s here.
        if req.reasoning_effort.as_deref() == Some("none") {
            req.reasoning_effort = None;
        }

        // Strip MiMo's web_search builtin for models that don't support it.
        // Without this strip the upstream returns 400 "webSearchEnabled is false"
        // even when the account has the plugin activated, because the model
        // endpoint itself rejects the capability (e.g. mimo-v2-omni).
        if MIMO_NO_WEB_SEARCH.contains(&model) {
            if let Some(ref mut tools) = req.tools {
                tools.retain(|t| t.get("type").and_then(|x| x.as_str()) != Some("web_search"));
                if tools.is_empty() {
                    req.tools = None;
                }
            }
        }
    }

    fn provider_type(&self) -> &str {
        "mimo"
    }

    fn enhance_error(&self, status: u16, body: &str) -> Option<String> {
        // 400 "webSearchEnabled is false" — the account doesn't have the
        // Web Search Plugin activated. Surface a one-click activation hint
        // instead of letting the raw upstream string confuse the user.
        if status == 400 && body.contains("webSearchEnabled is false") {
            return Some(
                "MiMo 账号未开通 Web Search Plugin（联网搜索插件，按次计费）。\
                 请去 https://platform.xiaomimimo.com/#/console/plugin 开通后，\
                 重启客户端再试。如果不需要联网搜索，让模型主动避免调用该工具即可。"
                    .to_string(),
            );
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::{ToolCall, ToolCallFunction};
    use crate::transform::providers::ProviderTransform;
    use serde_json::json;

    fn req(model: &str) -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: model.to_string(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: false,
            temperature: None,
            top_p: None,
            max_tokens: None,
            thinking: None,
            stream_options: None,
            response_format: None,
            reasoning_effort: None,
            seed: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
        }
    }

    fn assistant_with_tool_call(content: Option<&str>, tc_id: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".into(),
            content: content.map(|c| json!(c)),
            reasoning_content: None,
            tool_calls: Some(vec![ToolCall {
                id: tc_id.into(),
                call_type: "function".into(),
                function: ToolCallFunction {
                    name: "f".into(),
                    arguments: "{}".into(),
                },
            }]),
            tool_call_id: None,
            name: None,
        }
    }

    #[test]
    fn temperature_stripped_in_thinking_mode_for_v25_pro() {
        let mut r = req("mimo-v2.5-pro");
        r.thinking = Some(json!({"type": "enabled"}));
        r.temperature = Some(0.5);
        MimoProvider.finalize_request(&mut r, &None);
        assert!(r.temperature.is_none(), "v2.5-pro thinking mode strips temperature");
    }

    #[test]
    fn temperature_stripped_in_thinking_mode_for_v25() {
        let mut r = req("mimo-v2.5");
        r.thinking = Some(json!({"type": "enabled"}));
        r.temperature = Some(0.7);
        MimoProvider.finalize_request(&mut r, &None);
        assert!(r.temperature.is_none());
    }

    #[test]
    fn temperature_kept_for_flash_in_thinking_mode() {
        let mut r = req("mimo-v2-flash");
        r.thinking = Some(json!({"type": "enabled"}));
        r.temperature = Some(0.3);
        MimoProvider.finalize_request(&mut r, &None);
        assert_eq!(r.temperature, Some(0.3), "flash is not in the strip list");
    }

    #[test]
    fn temperature_kept_when_thinking_disabled() {
        let mut r = req("mimo-v2.5-pro");
        r.thinking = Some(json!({"type": "disabled"}));
        r.temperature = Some(0.5);
        MimoProvider.finalize_request(&mut r, &None);
        assert_eq!(r.temperature, Some(0.5), "disabled thinking → temp passes through");
    }

    #[test]
    fn tool_choice_non_auto_stripped() {
        let mut r = req("mimo-v2.5-pro");
        r.tool_choice = Some(json!("required"));
        MimoProvider.finalize_request(&mut r, &None);
        assert!(r.tool_choice.is_none());
    }

    #[test]
    fn tool_choice_auto_kept() {
        let mut r = req("mimo-v2.5-pro");
        r.tool_choice = Some(json!("auto"));
        MimoProvider.finalize_request(&mut r, &None);
        assert_eq!(r.tool_choice, Some(json!("auto")));
    }

    #[test]
    fn reasoning_effort_none_stripped() {
        let mut r = req("mimo-v2.5-pro");
        r.reasoning_effort = Some("none".into());
        MimoProvider.finalize_request(&mut r, &None);
        assert!(r.reasoning_effort.is_none());
    }

    #[test]
    fn reasoning_effort_high_kept() {
        let mut r = req("mimo-v2.5-pro");
        r.reasoning_effort = Some("high".into());
        MimoProvider.finalize_request(&mut r, &None);
        assert_eq!(r.reasoning_effort, Some("high".into()));
    }

    #[test]
    fn reasoning_content_backfilled_for_assistant_with_tool_calls() {
        let msg = assistant_with_tool_call(Some("text"), "tc-1");
        let out = MimoProvider.process_messages(vec![msg]).unwrap();
        assert!(
            out[0].reasoning_content.is_some(),
            "missing reasoning_content must be backfilled to avoid 400 in MiMo thinking mode"
        );
    }

    #[test]
    fn reasoning_content_preserved_when_present() {
        let mut msg = assistant_with_tool_call(Some("text"), "tc-2");
        msg.reasoning_content = Some("original trace".into());
        let out = MimoProvider.process_messages(vec![msg]).unwrap();
        assert_eq!(out[0].reasoning_content.as_deref(), Some("original trace"));
    }

    #[test]
    fn enhance_error_maps_web_search_plugin_not_activated() {
        let body = r#"{"error":{"message":"web search tool found in the request body, but webSearchEnabled is false","type":"invalid_request_error"}}"#;
        let suggestion = MimoProvider.enhance_error(400, body)
            .expect("400 + webSearchEnabled marker should produce a suggestion");
        assert!(suggestion.contains("Web Search Plugin"), "suggestion should mention the plugin");
        assert!(suggestion.contains("xiaomimimo.com"), "suggestion should include activation URL");
    }

    #[test]
    fn enhance_error_ignores_unrelated_400s() {
        let body = r#"{"error":{"message":"Invalid model id","type":"invalid_request_error"}}"#;
        assert!(MimoProvider.enhance_error(400, body).is_none());
    }

    #[test]
    fn omni_strips_web_search_tool() {
        let mut r = req("mimo-v2-omni");
        r.tools = Some(vec![
            json!({"type": "web_search"}),
            json!({"type": "function", "function": {"name": "do_stuff"}}),
        ]);
        MimoProvider.finalize_request(&mut r, &None);
        let tools = r.tools.expect("function tool should remain");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
    }

    #[test]
    fn omni_clears_tools_array_when_only_web_search_present() {
        let mut r = req("mimo-v2-omni");
        r.tools = Some(vec![json!({"type": "web_search"})]);
        MimoProvider.finalize_request(&mut r, &None);
        assert!(r.tools.is_none(), "empty tools array → None (cleaner request body)");
    }

    #[test]
    fn omni_with_1m_qualifier_still_strips_web_search() {
        let mut r = req("mimo-v2-omni[1m]");
        r.tools = Some(vec![json!({"type": "web_search"})]);
        MimoProvider.finalize_request(&mut r, &None);
        assert!(r.tools.is_none());
    }

    #[test]
    fn non_omni_models_keep_web_search() {
        for model in ["mimo-v2.5-pro", "mimo-v2.5", "mimo-v2-pro", "mimo-v2-flash"] {
            let mut r = req(model);
            r.tools = Some(vec![json!({"type": "web_search"})]);
            MimoProvider.finalize_request(&mut r, &None);
            assert!(r.tools.is_some(), "{model} should keep web_search tool");
        }
    }

    #[test]
    fn enhance_error_ignores_non_400_status() {
        let body = r#"webSearchEnabled is false"#;
        // 500 with the same marker — not the plugin error, don't false-positive.
        assert!(MimoProvider.enhance_error(500, body).is_none());
    }
}
