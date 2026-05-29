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
