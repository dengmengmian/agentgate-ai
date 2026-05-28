use axum::body::Body;
use axum::routing::{get, post};
use axum::Router;
use http_body::{Body as HttpBody, Frame, SizeHint};
use rusqlite::Connection;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::sync::oneshot;

use crate::errors::AppError;
use crate::gateway::routes::{self, GatewayState};

/// 包一层 Body，使 active-requests 计数器在 **body 真正流完**（或被
/// drop——客户端断开）时才 decrement。
///
/// 修复的 bug：axum 的 `middleware::from_fn` 模式里 `next.run(req).await`
/// 在 response **headers 发完**就返回，**body 还没流给客户端**——对 99%
/// 是 SSE 流式请求的 AI gateway 来说，这个 await 几毫秒就完事，counter
/// 在 dashboard 3 秒一拉之前早就回 0 了，"活跃连接"永远显示 0。
///
/// 现在 increment 在 middleware 入口、decrement 在这个 wrapper 的 Drop
/// 里——body 完整流完或客户端断开后才触发，反映真实在飞的请求数。
struct CountingBody {
    inner: Body,
    counter: Arc<AtomicU64>,
    decremented: bool,
}

impl Drop for CountingBody {
    fn drop(&mut self) {
        if !self.decremented {
            self.counter.fetch_sub(1, Ordering::Relaxed);
            self.decremented = true;
        }
    }
}

impl HttpBody for CountingBody {
    type Data = bytes::Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.inner).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

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
                // 不在这 decrement —— next.run 对 streaming 响应几毫秒就返回，
                // body 还没流给 client。包一层 CountingBody，body Drop 时才减。
                let (parts, body) = response.into_parts();
                let wrapped = CountingBody { inner: body, counter, decremented: false };
                axum::http::Response::from_parts(parts, Body::new(wrapped))
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
