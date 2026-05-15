use serde::Serialize;

use crate::errors::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RouteMode {
    PassThrough,
    Transform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ClientProtocol {
    OpenAIResponses,
    OpenAIChatCompletions,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteDecision {
    pub client_protocol: ClientProtocol,
    pub mode: RouteMode,
    pub target_url: String,
    pub reason: String,
}

/// Decide whether to pass-through or transform based on the client endpoint
/// and the provider's protocol.
///
/// Rules:
/// - /v1/chat/completions, /chat/completions => client = OpenAIChatCompletions
/// - /v1/responses, /responses              => client = OpenAIResponses
///
/// Provider protocol is read from `providers.protocol` column. Currently all
/// supported providers use `openai_chat_completions`.
///
/// Same protocol => PassThrough. Different => Transform.
pub fn decide(
    route: &str,
    provider_protocol: &str,
    provider_base_url: &str,
) -> Result<RouteDecision, AppError> {
    // Parse provider_protocol as JSON array or single string
    let protocols: Vec<String> = serde_json::from_str(provider_protocol)
        .unwrap_or_else(|_| vec![provider_protocol.to_string()]);

    decide_with_protocols(route, &protocols, provider_base_url)
}

fn decide_with_protocols(
    route: &str,
    provider_protocols: &[String],
    provider_base_url: &str,
) -> Result<RouteDecision, AppError> {
    let client_protocol = match route {
        "/v1/chat/completions" | "/chat/completions" => ClientProtocol::OpenAIChatCompletions,
        "/v1/responses" | "/responses" => ClientProtocol::OpenAIResponses,
        _ => {
            return Err(AppError::new("ROUTE_NOT_SUPPORTED", format!("Route '{route}' is not supported"))
                .with_suggestion("Use /v1/chat/completions, /v1/responses, or /v1/models"));
        }
    };

    let has = |p: &str| provider_protocols.iter().any(|pp| pp == p);

    match client_protocol {
        ClientProtocol::OpenAIChatCompletions if has("openai_chat_completions") => {
            Ok(RouteDecision {
                client_protocol,
                mode: RouteMode::PassThrough,
                target_url: build_chat_completions_url(provider_base_url),
                reason: "Client and provider both use OpenAI Chat Completions".to_string(),
            })
        }
        ClientProtocol::OpenAIResponses if has("openai_responses") => {
            // Prefer pass-through when provider natively supports Responses API
            Ok(RouteDecision {
                client_protocol,
                mode: RouteMode::PassThrough,
                target_url: build_responses_url(provider_base_url),
                reason: "Client and provider both use OpenAI Responses API".to_string(),
            })
        }
        ClientProtocol::OpenAIResponses if has("openai_chat_completions") => {
            // Fallback: transform Responses → Chat Completions
            Ok(RouteDecision {
                client_protocol,
                mode: RouteMode::Transform,
                target_url: build_chat_completions_url(provider_base_url),
                reason: "OpenAI Responses -> Chat Completions transform".to_string(),
            })
        }
        _ => {
            let protocols_str = provider_protocols.join(", ");
            Err(AppError::new(
                "PROTOCOL_TRANSFORM_NOT_SUPPORTED",
                format!("Transform from {route} to provider protocols [{protocols_str}] is not supported"),
            ).with_suggestion("Change the active provider or use a compatible endpoint"))
        }
    }
}

/// Build the responses URL, avoiding double /v1.
pub fn build_responses_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/responses")
    } else {
        format!("{base}/v1/responses")
    }
}

/// Build the chat completions URL, avoiding double /v1.
pub fn build_chat_completions_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decide_chat_completions_pass_through() {
        let decision = decide("/v1/chat/completions", "openai_chat_completions", "https://api.openai.com").unwrap();
        assert_eq!(decision.client_protocol, ClientProtocol::OpenAIChatCompletions);
        assert_eq!(decision.mode, RouteMode::PassThrough);
        assert_eq!(decision.target_url, "https://api.openai.com/v1/chat/completions");
    }

    #[test]
    fn test_decide_responses_transform() {
        let decision = decide("/v1/responses", "openai_chat_completions", "https://api.deepseek.com").unwrap();
        assert_eq!(decision.client_protocol, ClientProtocol::OpenAIResponses);
        assert_eq!(decision.mode, RouteMode::Transform);
        assert_eq!(decision.target_url, "https://api.deepseek.com/v1/chat/completions");
    }

    #[test]
    fn test_decide_unsupported_route() {
        let err = decide("/v1/unknown", "openai_chat_completions", "https://api.openai.com").unwrap_err();
        assert_eq!(err.code, "ROUTE_NOT_SUPPORTED");
    }

    #[test]
    fn test_decide_unsupported_protocol() {
        let err = decide("/v1/chat/completions", "anthropic_messages", "https://api.anthropic.com").unwrap_err();
        assert_eq!(err.code, "PROTOCOL_TRANSFORM_NOT_SUPPORTED");
    }

    #[test]
    fn test_build_chat_completions_url_no_trailing_slash() {
        assert_eq!(
            build_chat_completions_url("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_build_chat_completions_url_with_trailing_slash() {
        assert_eq!(
            build_chat_completions_url("https://api.openai.com/"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_build_chat_completions_url_with_v1() {
        assert_eq!(
            build_chat_completions_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_build_chat_completions_url_with_v1_and_slash() {
        assert_eq!(
            build_chat_completions_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_build_chat_completions_url_localhost() {
        assert_eq!(
            build_chat_completions_url("http://127.0.0.1:8080"),
            "http://127.0.0.1:8080/v1/chat/completions"
        );
    }

    #[test]
    fn test_build_responses_url() {
        assert_eq!(build_responses_url("https://api.openai.com"), "https://api.openai.com/v1/responses");
        assert_eq!(build_responses_url("https://api.openai.com/v1"), "https://api.openai.com/v1/responses");
    }

    // ── Multi-protocol (JSON array) tests ──

    #[test]
    fn test_decide_json_array_chat_completions_pass_through() {
        let decision = decide("/v1/chat/completions", r#"["openai_chat_completions","openai_responses"]"#, "https://newapi.com").unwrap();
        assert_eq!(decision.mode, RouteMode::PassThrough);
        assert_eq!(decision.target_url, "https://newapi.com/v1/chat/completions");
    }

    #[test]
    fn test_decide_json_array_responses_pass_through() {
        let decision = decide("/v1/responses", r#"["openai_chat_completions","openai_responses"]"#, "https://newapi.com").unwrap();
        assert_eq!(decision.mode, RouteMode::PassThrough);
        assert_eq!(decision.target_url, "https://newapi.com/v1/responses");
    }

    #[test]
    fn test_decide_json_array_responses_fallback_to_transform() {
        // Provider only supports chat_completions, not responses → must transform
        let decision = decide("/v1/responses", r#"["openai_chat_completions"]"#, "https://api.deepseek.com").unwrap();
        assert_eq!(decision.mode, RouteMode::Transform);
        assert_eq!(decision.target_url, "https://api.deepseek.com/v1/chat/completions");
    }

    #[test]
    fn test_decide_json_array_all_three_protocols() {
        let proto = r#"["openai_chat_completions","openai_responses","anthropic_messages"]"#;
        let d1 = decide("/v1/chat/completions", proto, "https://newapi.com").unwrap();
        assert_eq!(d1.mode, RouteMode::PassThrough);
        let d2 = decide("/v1/responses", proto, "https://newapi.com").unwrap();
        assert_eq!(d2.mode, RouteMode::PassThrough);
    }

    #[test]
    fn test_decide_json_array_responses_prefers_pass_through() {
        // Provider supports both responses and chat_completions → prefer pass-through for /v1/responses
        let decision = decide("/v1/responses", r#"["openai_responses","openai_chat_completions"]"#, "https://newapi.com").unwrap();
        assert_eq!(decision.mode, RouteMode::PassThrough);
        assert_eq!(decision.target_url, "https://newapi.com/v1/responses");
    }

    #[test]
    fn test_decide_unsupported_protocol_json_array() {
        let err = decide("/v1/chat/completions", r#"["anthropic_messages"]"#, "https://api.anthropic.com").unwrap_err();
        assert_eq!(err.code, "PROTOCOL_TRANSFORM_NOT_SUPPORTED");
    }
}
