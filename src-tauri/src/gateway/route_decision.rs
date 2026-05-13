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
    let client_protocol = match route {
        "/v1/chat/completions" | "/chat/completions" => ClientProtocol::OpenAIChatCompletions,
        "/v1/responses" | "/responses" => ClientProtocol::OpenAIResponses,
        _ => {
            return Err(AppError::new("ROUTE_NOT_SUPPORTED", format!("Route '{route}' is not supported"))
                .with_suggestion("Use /v1/chat/completions, /v1/responses, or /v1/models"));
        }
    };

    let target_url = build_chat_completions_url(provider_base_url);

    match (client_protocol, provider_protocol) {
        // Chat Completions -> Chat Completions = pass-through
        (ClientProtocol::OpenAIChatCompletions, "openai_chat_completions") => {
            Ok(RouteDecision {
                client_protocol,
                mode: RouteMode::PassThrough,
                target_url,
                reason: "Client and provider both use OpenAI Chat Completions".to_string(),
            })
        }
        // Responses -> Chat Completions = transform (handled by existing code)
        (ClientProtocol::OpenAIResponses, "openai_chat_completions") => {
            Ok(RouteDecision {
                client_protocol,
                mode: RouteMode::Transform,
                target_url,
                reason: "OpenAI Responses -> Chat Completions transform".to_string(),
            })
        }
        // Other combinations not yet supported
        _ => {
            Err(AppError::new(
                "PROTOCOL_TRANSFORM_NOT_SUPPORTED",
                format!("Transform from {route} to provider protocol '{provider_protocol}' is not supported"),
            ).with_suggestion("Change the active provider or use a compatible endpoint"))
        }
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
}
