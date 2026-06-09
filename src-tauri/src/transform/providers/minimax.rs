use crate::protocol::chat_completions::ChatCompletionsRequest;
use serde_json::Value;

pub struct MiniMaxProvider;

impl super::ProviderTransform for MiniMaxProvider {
    fn finalize_request(&self, req: &mut ChatCompletionsRequest, _tools: &Option<Vec<Value>>) {
        // MiniMax doesn't support reasoning_effort
        req.reasoning_effort = None;
        // MiniMax doesn't support response_format
        req.response_format = None;

        // 以下针对 MiniMax 严格 API 的兼容：
        // 1. tool_choice="auto" 是默认值，MiniMax 拒显式传 → 省略
        if req.tool_choice.as_ref().and_then(Value::as_str) == Some("auto") {
            req.tool_choice = None;
        }
        // 2. tools[].function.strict === null → 删（MiniMax 要么不传要么 boolean）
        if let Some(tools) = req.tools.as_mut() {
            for t in tools.iter_mut() {
                if let Some(func) = t.get_mut("function").and_then(Value::as_object_mut) {
                    if func.get("strict").is_some_and(Value::is_null) {
                        func.remove("strict");
                    }
                }
            }
        }
        // 3. assistant 消息 content===null → 删字段（MiniMax 拒 null content）
        for m in req.messages.iter_mut() {
            if m.role == "assistant" && matches!(m.content, Some(Value::Null)) {
                m.content = None;
            }
        }
        // 4. 多条 system 合并为单条前置（MiniMax 只接受 1 条且须最前）
        super::merge_system_messages(&mut req.messages);
    }

    fn provider_type(&self) -> &str {
        "minimax"
    }

    fn enhance_error(&self, status: u16, body: &str) -> Option<String> {
        use crate::transform::providers as p;
        if p::detect_insufficient_balance(status, body) {
            return Some(
                "MiniMax 账户余额不足。\n\
                 • 充值入口：https://platform.minimaxi.com/user-center/finance/balance\n\
                 • 用量查询：https://platform.minimaxi.com/user-center/finance/usage"
                    .to_string(),
            );
        }
        if p::detect_auth_error(status, body) {
            return Some(
                "MiniMax API key 无效 / 过期。\n\
                 • 重建 key：https://platform.minimaxi.com/user-center/basic-information/interface-key"
                    .to_string(),
            );
        }
        if p::detect_rate_limit(status, body) {
            return Some(
                "MiniMax 触发限流。AgentGate 已冷却该 provider，路由会自动切换候选。".to_string(),
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
            model: "minimax-pro".into(),
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

    #[test]
    fn minimax_strips_reasoning_effort() {
        let mut r = req();
        r.reasoning_effort = Some("high".into());
        MiniMaxProvider.finalize_request(&mut r, &None);
        assert!(r.reasoning_effort.is_none());
    }

    #[test]
    fn minimax_strips_response_format() {
        let mut r = req();
        r.response_format = Some(serde_json::json!({"type": "json_object"}));
        MiniMaxProvider.finalize_request(&mut r, &None);
        assert!(r.response_format.is_none());
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
    fn minimax_strips_tool_choice_auto() {
        let mut r = req();
        r.tool_choice = Some(serde_json::json!("auto"));
        MiniMaxProvider.finalize_request(&mut r, &None);
        assert!(r.tool_choice.is_none());
    }

    #[test]
    fn minimax_keeps_explicit_tool_choice() {
        let mut r = req();
        r.tool_choice = Some(serde_json::json!("required"));
        MiniMaxProvider.finalize_request(&mut r, &None);
        assert_eq!(r.tool_choice, Some(serde_json::json!("required")));
    }

    #[test]
    fn minimax_strips_null_strict() {
        let mut r = req();
        r.tools = Some(vec![serde_json::json!({
            "type": "function",
            "function": {"name": "f", "strict": null}
        })]);
        MiniMaxProvider.finalize_request(&mut r, &None);
        assert!(r.tools.unwrap()[0]["function"].get("strict").is_none());
    }

    #[test]
    fn minimax_drops_null_assistant_content() {
        let mut r = req();
        r.messages = vec![msg("assistant", serde_json::Value::Null)];
        MiniMaxProvider.finalize_request(&mut r, &None);
        assert!(r.messages[0].content.is_none());
    }

    #[test]
    fn minimax_merges_system_messages() {
        let mut r = req();
        r.messages = vec![
            msg("system", serde_json::json!("a")),
            msg("user", serde_json::json!("hi")),
            msg("system", serde_json::json!("b")),
        ];
        MiniMaxProvider.finalize_request(&mut r, &None);
        let systems: Vec<_> = r.messages.iter().filter(|m| m.role == "system").collect();
        assert_eq!(systems.len(), 1);
        assert_eq!(
            systems[0].content.as_ref().unwrap().as_str().unwrap(),
            "a\n\nb"
        );
        assert_eq!(r.messages[0].role, "system");
    }

    #[test]
    fn minimax_provider_type() {
        assert_eq!(MiniMaxProvider.provider_type(), "minimax");
    }
}
