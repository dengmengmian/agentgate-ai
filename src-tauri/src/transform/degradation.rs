use crate::protocol::chat_completions::{CapabilityDegradationEvent, ChatMessage};
use serde_json::{json, Value};

pub fn event(
    capability: &str,
    source: &str,
    provider: Option<&str>,
    model: Option<&str>,
    message: impl Into<String>,
    count: Option<usize>,
    reason: Option<&str>,
) -> CapabilityDegradationEvent {
    CapabilityDegradationEvent {
        kind: "capability_degradation".to_string(),
        capability: capability.to_string(),
        source: source.to_string(),
        provider: provider.map(str::to_string),
        model: model.map(str::to_string),
        message: message.into(),
        count,
        reason: reason.map(str::to_string),
    }
}

pub fn image_stripped_notice(
    provider_label: &str,
    model: &str,
    stripped_count: usize,
    recovery_hint: &str,
) -> String {
    let plural = if stripped_count == 1 { "" } else { "s" };
    format!(
        "[Note: {stripped_count} image{plural} stripped - the current {provider_label} model \
         ({model}) does not support image input. {recovery_hint}]"
    )
}

pub fn strip_image_parts_with_notice(
    messages: &mut [ChatMessage],
    provider_type: &str,
    provider_label: &str,
    model: &str,
    recovery_hint: &str,
) -> Vec<CapabilityDegradationEvent> {
    let mut events = Vec::new();
    for msg in messages {
        let Some(Value::Array(parts)) = &msg.content else {
            continue;
        };

        let stripped_count = parts
            .iter()
            .filter(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url"))
            .count();
        if stripped_count == 0 {
            continue;
        }

        let mut text_only: Vec<Value> = parts
            .iter()
            .filter(|p| p.get("type").and_then(|t| t.as_str()) != Some("image_url"))
            .cloned()
            .collect();
        let notice = image_stripped_notice(provider_label, model, stripped_count, recovery_hint);
        append_text_notice(&mut text_only, &notice);
        msg.content = Some(Value::Array(text_only));
        events.push(event(
            "vision",
            "provider_transform",
            Some(provider_type),
            Some(model),
            notice,
            Some(stripped_count),
            Some("model_does_not_support_image_input"),
        ));
    }
    events
}

pub fn append_text_notice(parts: &mut Vec<Value>, notice: &str) {
    let notice = format!("\n\n{notice}");
    let last_text_pos = parts
        .iter()
        .rposition(|p| p.get("type").and_then(|t| t.as_str()) == Some("text"));
    if let Some(pos) = last_text_pos {
        if let Some(existing) = parts[pos].get("text").and_then(|t| t.as_str()) {
            parts[pos]["text"] = Value::String(format!("{existing}{notice}"));
            return;
        }
    }

    parts.push(json!({
        "type": "text",
        "text": notice.trim_start_matches('\n'),
    }));
}

pub fn tool_output_image_omitted_notice(dropped_images: usize) -> String {
    let suffix = if dropped_images > 1 { "s" } else { "" };
    format!("[{dropped_images} image attachment{suffix} omitted from tool output]")
}

pub fn tool_output_image_omitted_event(dropped_images: usize) -> CapabilityDegradationEvent {
    event(
        "vision",
        "tool_output_transform",
        None,
        None,
        tool_output_image_omitted_notice(dropped_images),
        Some(dropped_images),
        Some("chat_completions_tool_messages_do_not_support_images"),
    )
}

pub fn mcp_advisory_event(labels: &[String]) -> CapabilityDegradationEvent {
    let list = labels.join(", ");
    event(
        "mcp",
        "tool_transform",
        None,
        None,
        format!(
            "OpenAI Responses MCP connector tools are not callable through this Chat Completions upstream: {list}"
        ),
        Some(labels.len()),
        Some("upstream_does_not_implement_openai_mcp_runtime"),
    )
}

pub fn web_search_degraded_event(
    provider: &str,
    model: Option<&str>,
    reason: &str,
) -> CapabilityDegradationEvent {
    event(
        "web_search",
        "provider_adapter",
        Some(provider),
        model,
        "Native web_search was stripped and the request was retried without it",
        Some(1),
        Some(reason),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_stripped_notice_uses_singular_for_one() {
        let notice = image_stripped_notice("DeepSeek", "deepseek-chat", 1, "Use a vision model");
        assert!(notice.contains("1 image stripped"));
        assert!(notice.contains("DeepSeek"));
        assert!(notice.contains("deepseek-chat"));
        assert!(notice.contains("Use a vision model"));
    }

    #[test]
    fn image_stripped_notice_uses_plural_for_many() {
        let notice = image_stripped_notice("Kimi", "kimi-vl", 2, "Vision needed");
        assert!(notice.contains("2 images stripped"));
        let out = strip_image_parts_with_notice(
            &mut [ChatMessage {
                role: "user".to_string(),
                content: Some(json!([
                    {"type": "text", "text": "look"},
                    {"type": "image_url", "image_url": {"url": "http://a"}},
                    {"type": "image_url", "image_url": {"url": "http://b"}}
                ])),
                reasoning_content: None,
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            "kimi",
            "Kimi",
            "kimi-vl",
            "Vision needed",
        );
        assert!(out[0].message.contains("2 images stripped"));
    }

    #[test]
    fn strip_image_parts_with_notice_skips_non_array_content() {
        let mut msgs = [
            ChatMessage {
                role: "user".to_string(),
                content: Some(json!("plain string")),
                reasoning_content: None,
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: None,
                reasoning_content: None,
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];
        let events = strip_image_parts_with_notice(&mut msgs, "x", "X", "m", "hint");
        assert!(events.is_empty());
        assert_eq!(msgs[0].content, Some(json!("plain string")));
        assert!(msgs[1].content.is_none());
    }

    #[test]
    fn strip_image_parts_with_notice_removes_images_and_appends_notice() {
        let mut msgs = [ChatMessage {
            role: "user".to_string(),
            content: Some(json!([
                {"type": "image_url", "image_url": {"url": "http://img"}},
                {"type": "text", "text": "describe"}
            ])),
            reasoning_content: None,
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }];
        let events = strip_image_parts_with_notice(&mut msgs, "ds", "DeepSeek", "ds-chat", "use vision");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].capability, "vision");
        assert_eq!(events[0].source, "provider_transform");
        assert_eq!(events[0].provider, Some("ds".to_string()));
        assert_eq!(events[0].model, Some("ds-chat".to_string()));
        assert_eq!(events[0].count, Some(1));
        assert_eq!(events[0].reason, Some("model_does_not_support_image_input".to_string()));

        let parts = msgs[0].content.as_ref().unwrap().as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["type"], "text");
        let text = parts[0]["text"].as_str().unwrap();
        assert!(text.starts_with("describe"));
        assert!(text.contains("image stripped"));
    }

    #[test]
    fn append_text_notice_appends_to_existing_text_part() {
        let mut parts = vec![json!({"type": "text", "text": "hello"})];
        append_text_notice(&mut parts, "note");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["text"], "hello\n\nnote");
    }

    #[test]
    fn append_text_notice_creates_new_part_when_no_text_exists() {
        let mut parts = vec![json!({"type": "image_url"})];
        append_text_notice(&mut parts, "note");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1]["type"], "text");
        assert_eq!(parts[1]["text"], "note");
    }

    #[test]
    fn tool_output_image_omitted_event_shape() {
        let e = tool_output_image_omitted_event(3);
        assert_eq!(e.capability, "vision");
        assert_eq!(e.source, "tool_output_transform");
        assert_eq!(e.count, Some(3));
        assert_eq!(e.reason, Some("chat_completions_tool_messages_do_not_support_images".to_string()));
        assert!(e.message.contains("3 image attachments omitted"));
    }

    #[test]
    fn mcp_advisory_event_joins_labels() {
        let e = mcp_advisory_event(&["web_search".to_string(), "custom".to_string()]);
        assert_eq!(e.capability, "mcp");
        assert_eq!(e.source, "tool_transform");
        assert_eq!(e.count, Some(2));
        assert_eq!(
            e.reason,
            Some("upstream_does_not_implement_openai_mcp_runtime".to_string())
        );
        assert!(e.message.contains("web_search, custom"));
    }

    #[test]
    fn web_search_degraded_event_shape() {
        let e = web_search_degraded_event("mimo", Some("mimo-pro"), "plugin_not_enabled");
        assert_eq!(e.capability, "web_search");
        assert_eq!(e.source, "provider_adapter");
        assert_eq!(e.provider, Some("mimo".to_string()));
        assert_eq!(e.model, Some("mimo-pro".to_string()));
        assert_eq!(e.count, Some(1));
        assert_eq!(e.reason, Some("plugin_not_enabled".to_string()));
    }
}
