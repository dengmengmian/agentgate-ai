use crate::protocol::chat_completions::ChatCompletionsRequest;
use serde_json::Value;

pub struct SenseNovaProvider;

impl super::ProviderTransform for SenseNovaProvider {
    fn finalize_request(&self, req: &mut ChatCompletionsRequest, _tools: &Option<Vec<Value>>) {
        // 针对 SenseNova 6.x 严格 OpenAI 子集的防御性清理（参考 mimo2codex sensenova preset）：
        // SenseNova 不支持 response_format
        req.response_format = None;

        // tool_choice="auto" 是默认值，SenseNova 拒显式传 → 省略
        if req.tool_choice.as_ref().and_then(Value::as_str) == Some("auto") {
            req.tool_choice = None;
        }

        // tools 处理：SenseNova schema 只接受 tools[].type ∈ {function, custom}，
        // Codex / Claude Code 常塞 web_search / file_search 等内置 tool，会被一刀切 400 → 删。
        // 同时 strict===null 也要删（SenseNova 要么不传要么 boolean）。
        if let Some(tools) = req.tools.as_mut() {
            tools.retain(|t| {
                matches!(
                    t.get("type").and_then(Value::as_str),
                    Some("function") | Some("custom")
                )
            });
            for t in tools.iter_mut() {
                if let Some(func) = t.get_mut("function").and_then(Value::as_object_mut) {
                    if func.get("strict").is_some_and(Value::is_null) {
                        func.remove("strict");
                    }
                }
            }
            if tools.is_empty() {
                req.tools = None;
            }
        }

        // assistant 消息 content===null → 删字段（SenseNova 拒 null content）
        for m in req.messages.iter_mut() {
            if m.role == "assistant" && matches!(m.content, Some(Value::Null)) {
                m.content = None;
            }
        }

        // 多条 system 合并为单条前置
        super::merge_system_messages(&mut req.messages);
    }

    fn provider_type(&self) -> &str {
        "sensenova"
    }

    fn enhance_error(&self, status: u16, body: &str) -> Option<String> {
        use crate::transform::providers as p;
        if p::detect_insufficient_balance(status, body) {
            return Some(
                "SenseNova 账户余额不足，请前往控制台充值：https://platform.sensenova.cn"
                    .to_string(),
            );
        }
        if p::detect_auth_error(status, body) {
            return Some(
                "SenseNova API key 无效 / 过期，请在控制台重建：https://platform.sensenova.cn"
                    .to_string(),
            );
        }
        if p::detect_rate_limit(status, body) {
            return Some(
                "SenseNova 触发限流。AgentGate 已冷却该 provider，路由会自动切换候选。".to_string(),
            );
        }
        p::detect_common_400(status, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::chat_completions::{ChatCompletionsRequest, ChatMessage};
    use crate::transform::providers::ProviderTransform;

    fn req() -> ChatCompletionsRequest {
        ChatCompletionsRequest {
            model: "sensenova-6.7-flash-lite".into(),
            messages: vec![],
            tools: None,
            tool_choice: None,
            stream: false,
            temperature: None,
            top_p: None,
            max_tokens: None,
            max_completion_tokens: None,
            thinking: None,
            stream_options: None,
            response_format: None,
            reasoning_effort: None,
            seed: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            parallel_tool_calls: None,
            diagnostic_events: Vec::new(),
        }
    }

    fn msg(role: &str, content: serde_json::Value) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: Some(content),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    #[test]
    fn sensenova_strips_response_format() {
        let mut r = req();
        r.response_format = Some(serde_json::json!({"type": "json_object"}));
        SenseNovaProvider.finalize_request(&mut r, &None);
        assert!(r.response_format.is_none());
    }

    #[test]
    fn sensenova_strips_tool_choice_auto() {
        let mut r = req();
        r.tool_choice = Some(serde_json::json!("auto"));
        SenseNovaProvider.finalize_request(&mut r, &None);
        assert!(r.tool_choice.is_none());
    }

    #[test]
    fn sensenova_keeps_explicit_tool_choice() {
        let mut r = req();
        r.tool_choice = Some(serde_json::json!("required"));
        SenseNovaProvider.finalize_request(&mut r, &None);
        assert_eq!(r.tool_choice, Some(serde_json::json!("required")));
    }

    #[test]
    fn sensenova_strips_null_strict() {
        let mut r = req();
        r.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "f", "strict": null}
        })]);
        SenseNovaProvider.finalize_request(&mut r, &None);
        assert!(r.tools.unwrap()[0]["function"].get("strict").is_none());
    }

    #[test]
    fn sensenova_drops_non_function_tools() {
        let mut r = req();
        r.tools = Some(vec![
            serde_json::json!({"type": "function", "function": {"name": "f"}}),
            serde_json::json!({"type": "web_search"}),
        ]);
        SenseNovaProvider.finalize_request(&mut r, &None);
        let tools = r.tools.unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"].as_str(), Some("function"));
    }

    #[test]
    fn sensenova_clears_tools_when_all_dropped() {
        let mut r = req();
        r.tools = Some(vec![serde_json::json!({"type": "web_search"})]);
        SenseNovaProvider.finalize_request(&mut r, &None);
        assert!(r.tools.is_none());
    }

    #[test]
    fn sensenova_drops_null_assistant_content() {
        let mut r = req();
        r.messages = vec![msg("assistant", serde_json::Value::Null)];
        SenseNovaProvider.finalize_request(&mut r, &None);
        assert!(r.messages[0].content.is_none());
    }

    #[test]
    fn sensenova_merges_system_messages() {
        let mut r = req();
        r.messages = vec![
            msg("system", serde_json::json!("a")),
            msg("user", serde_json::json!("hi")),
            msg("system", serde_json::json!("b")),
        ];
        SenseNovaProvider.finalize_request(&mut r, &None);
        let systems: Vec<_> = r.messages.iter().filter(|m| m.role == "system").collect();
        assert_eq!(systems.len(), 1);
        assert_eq!(systems[0].content.as_ref().unwrap().as_str().unwrap(), "a\n\nb");
        assert_eq!(r.messages[0].role, "system");
    }

    #[test]
    fn sensenova_provider_type() {
        assert_eq!(SenseNovaProvider.provider_type(), "sensenova");
    }
}
