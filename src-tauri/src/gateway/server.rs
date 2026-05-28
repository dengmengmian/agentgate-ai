use axum::routing::{get, post};
use axum::Router;
use rusqlite::Connection;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use crate::errors::AppError;
use crate::gateway::routes::{self, GatewayState};

/// Start the gateway HTTP server. Returns shutdown sender, join handle,
/// active-request counter, and the actually-bound port (useful when callers
/// pass `port=0` to let the OS pick — integration tests rely on this).
pub async fn start(
    host: &str,
    port: u16,
    db: Arc<Mutex<Connection>>,
) -> Result<(oneshot::Sender<()>, tokio::task::JoinHandle<()>, Arc<AtomicU64>, u16), AppError> {
    let http_client = reqwest::Client::builder()
        // 不设 .timeout() —— 那是"整请求总时限"，对 streaming AI 请求语义错配：
        // 模型只要还在持续吐 token 就不该被打断，长 prompt + thinking + 长输出
        // 突破任何固定 5/10/30 分钟上限都很正常。客户端真不要了，axum 检测到
        // connection drop 会取消上游 reqwest，资源不会泄露。
        //
        // 改用 .read_timeout()：单次 socket read 的 idle 超时。
        // 只要持续有字节到达就一直重置；连续 60s 一个字节都没收到才 abort。
        // 等价于"流仍活跃就持续延长，真正卡死才放弃"——这是 streaming AI
        // 请求该有的语义。错误通过 bytes_stream() 的 Err(reqwest::Error)
        // 自然冒泡，下游各 SSE 处理器用 e.is_timeout() 识别后给中文文案。
        .read_timeout(std::time::Duration::from_secs(60))
        // Drop idle keep-alive connections after 30s. Without this, the pool
        // hands out connections the remote has already RST'd after a long
        // pause (e.g. Claude Code stays open for minutes between user
        // messages), causing the first request after the pause to fail with
        // "error sending request" before reaching the upstream.
        .pool_idle_timeout(std::time::Duration::from_secs(30))
        .tcp_keepalive(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::internal(format!("Failed to create HTTP client: {e}")))?;

    let active_requests = Arc::new(AtomicU64::new(0));

    let state = GatewayState {
        db,
        http_client,
        active_requests: active_requests.clone(),
    };

    let counter = active_requests.clone();
    let app = Router::new()
        .route("/health", get(routes::health))
        .route("/v1/models", get(routes::list_models))
        .route("/v1/responses", post(routes::handle_responses))
        .route("/responses", post(routes::handle_responses))
        .route("/v1/chat/completions", post(routes::handle_chat_completions))
        .route("/chat/completions", post(routes::handle_chat_completions))
        .route("/v1/messages", post(routes::handle_messages))
        .route("/messages", post(routes::handle_messages))
        .route("/v1beta/models/{model_action}", post(routes::handle_gemini_generate))
        .layer(axum::middleware::from_fn(move |req, next: axum::middleware::Next| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::Relaxed);
                let response = next.run(req).await;
                counter.fetch_sub(1, Ordering::Relaxed);
                response
            }
        }))
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

    let bound_port = listener
        .local_addr()
        .map(|a| a.port())
        .unwrap_or(port);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .ok();
    });

    Ok((shutdown_tx, handle, active_requests, bound_port))
}
