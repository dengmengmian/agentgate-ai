use axum::routing::{get, post};
use axum::Router;
use rusqlite::Connection;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use crate::errors::AppError;
use crate::gateway::routes::{self, GatewayState};

/// Start the gateway HTTP server. Returns the shutdown sender and the join handle.
pub async fn start(
    host: &str,
    port: u16,
    db: Arc<Mutex<Connection>>,
) -> Result<(oneshot::Sender<()>, tokio::task::JoinHandle<()>), AppError> {
    // Reject 0.0.0.0 for security
    if host == "0.0.0.0" {
        return Err(AppError::new(
            "GATEWAY_BIND_ERROR",
            "Binding to 0.0.0.0 is not allowed for security reasons",
        )
        .with_suggestion("Use 127.0.0.1 to listen on localhost only"));
    }

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::internal(format!("Failed to create HTTP client: {e}")))?;

    let state = GatewayState {
        db,
        http_client,
    };

    let app = Router::new()
        .route("/health", get(routes::health))
        .route("/v1/models", get(routes::list_models))
        .route("/v1/responses", post(routes::handle_responses))
        .route("/responses", post(routes::handle_responses))
        .route("/v1/chat/completions", post(routes::handle_chat_completions))
        .route("/chat/completions", post(routes::handle_chat_completions))
        .route("/v1/messages", post(routes::handle_messages))
        .route("/messages", post(routes::handle_messages))
        .route("/v1beta/models/{model}:generateContent", post(routes::handle_gemini_generate))
        .route("/v1beta/models/{model}:streamGenerateContent", post(routes::handle_gemini_generate))
        .with_state(state);

    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
        AppError::new("GATEWAY_BIND_ERROR", format!("Invalid address: {e}"))
    })?;

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            AppError::new("GATEWAY_PORT_IN_USE", "Gateway port is already in use")
                .with_detail(format!("{host}:{port}"))
                .with_suggestion(
                    "Change the gateway port in Settings or stop the process using this port",
                )
        } else {
            AppError::new("GATEWAY_BIND_ERROR", format!("Failed to bind: {e}"))
        }
    })?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    Ok((shutdown_tx, handle))
}
