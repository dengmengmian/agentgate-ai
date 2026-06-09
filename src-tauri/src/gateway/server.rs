use axum::body::Body;
use axum::routing::{get, post};
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use http_body::{Body as HttpBody, Frame, SizeHint};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::oneshot;

use crate::errors::AppError;
use crate::gateway::routes::{self, GatewayState};

/// TLS 配置——同时提供 cert + key 文件路径才启 HTTPS，缺一退回 HTTP。
#[derive(Clone, Debug)]
pub struct TlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

/// 优雅 shutdown 等 in-flight 完成的最大时长。SSE 长流式响应仍可能超时被切，
/// 但 30s 给"正常 chat completion + 短 SSE"留够余地。
const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Start the gateway HTTP/HTTPS server. Returns shutdown sender, join handle,
/// active-request counter, and the actually-bound port (useful when callers
/// pass `port=0` to let the OS pick — integration tests rely on this).
///
/// `tls`: `Some` 启 HTTPS（用 rustls 加载 cert/key），`None` 启 HTTP。
/// shutdown signal 通过 oneshot 接，内部桥接到 `axum_server::Handle`，触发
/// 后等 `GRACEFUL_SHUTDOWN_TIMEOUT` in-flight 完成再强收。
pub async fn start(
    host: &str,
    port: u16,
    db: crate::storage::db::DbPool,
    tls: Option<TlsConfig>,
) -> Result<
    (
        oneshot::Sender<()>,
        tokio::task::JoinHandle<()>,
        Arc<AtomicU64>,
        u16,
    ),
    AppError,
> {
    let http_client = reqwest::Client::builder()
        // 不设 .timeout() —— 那是"整请求总时限"，对 streaming AI 请求语义错配：
        // 模型只要还在持续吐 token 就不该被打断，长 prompt + thinking + 长输出
        // 突破任何固定 5/10/30 分钟上限都很正常。客户端真不要了，axum 检测到
        // connection drop 会取消上游 reqwest，资源不会泄露。
        //
        // 改用 .read_timeout()：单次 socket read 的 idle 超时。
        // 只要持续有字节到达就一直重置；连续一段时间一个字节都没收到才 abort。
        // MiMo / DeepSeek 这类 thinking 模型在长 prompt prefill 阶段可能超过
        // 60s 不吐 SSE 字节；这里按 10 分钟给真实思考留空间。
        // 等价于"流仍活跃就持续延长，真正卡死才放弃"——这是 streaming AI
        // 请求该有的语义。错误通过 bytes_stream() 的 Err(reqwest::Error)
        // 自然冒泡，下游各 SSE 处理器用 e.is_timeout() 识别后给中文文案。
        .read_timeout(std::time::Duration::from_secs(
            crate::gateway::sse_bootstrap::STREAM_READ_IDLE_HINT_SECS,
        ))
        // 建连超时：上游 IP 可达但 TCP 不响应（安全组拦截 / BGP 黑洞）时，不设此项
        // 会挂到 OS 默认（常 2min+），叠加重试会让网关长时间卡死。只管"建连"阶段，
        // 建连后交给 read_timeout，不影响 streaming 的长输出。
        .connect_timeout(std::time::Duration::from_secs(10))
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

    // metrics recorder 幂等初始化（重复调 init 直接返回 false 不报错）。
    crate::gateway::metrics::init();

    let counter = active_requests.clone();
    let app = Router::new()
        .route("/health", get(routes::health))
        .route(
            "/metrics",
            get({
                // 渲染前同步当前 active_requests gauge —— gauge 平时由 SSE 流入流出
                // 的 CountingBody 增减，但 metrics 系统不直接访问 AtomicU64，渲染前
                // 镜像一次。
                let counter = active_requests.clone();
                move || {
                    let n = counter.load(Ordering::Relaxed);
                    crate::gateway::metrics::set_active_requests(n);
                    crate::gateway::metrics::render()
                }
            }),
        )
        .route("/v1/models", get(routes::list_models))
        .route("/v1/responses", post(routes::handle_responses))
        .route("/responses", post(routes::handle_responses))
        .route(
            "/v1/chat/completions",
            post(routes::handle_chat_completions),
        )
        .route("/chat/completions", post(routes::handle_chat_completions))
        .route("/v1/messages", post(routes::handle_messages))
        .route("/messages", post(routes::handle_messages))
        .route(
            "/v1/messages/count_tokens",
            post(routes::handle_count_tokens),
        )
        .route("/messages/count_tokens", post(routes::handle_count_tokens))
        .route("/v1beta/models", get(routes::list_gemini_models))
        .route(
            "/v1beta/models/:model_action",
            post(routes::handle_gemini_generate),
        )
        .layer(axum::middleware::from_fn(
            move |req, next: axum::middleware::Next| {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    let response = next.run(req).await;
                    // 不在这 decrement —— next.run 对 streaming 响应几毫秒就返回，
                    // body 还没流给 client。包一层 CountingBody，body Drop 时才减。
                    let (parts, body) = response.into_parts();
                    let wrapped = CountingBody {
                        inner: body,
                        counter,
                        decremented: false,
                    };
                    axum::http::Response::from_parts(parts, Body::new(wrapped))
                }
            },
        ))
        .with_state(state);

    // 可选 per-IP 限流(默认关)。AGENTGATE_RATE_LIMIT = 每 IP 每秒最大请求数。
    // 在请求入口计一次,不占整条 SSE 流;SmartIp 提取器兼容反代,否则回落 peer IP。
    let rate = std::env::var("AGENTGATE_RATE_LIMIT")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|n| *n > 0);
    let app = if let Some(r) = rate {
        use tower_governor::governor::GovernorConfigBuilder;
        use tower_governor::key_extractor::SmartIpKeyExtractor;
        use tower_governor::GovernorLayer;
        let conf = Arc::new(
            GovernorConfigBuilder::default()
                .period(Duration::from_nanos(1_000_000_000u64 / r as u64))
                .burst_size(r)
                .key_extractor(SmartIpKeyExtractor)
                .finish()
                .expect("build governor config"),
        );
        // 后台定期清理过期 IP 桶,防止内存随不同 IP 数无界增长。
        let limiter = conf.limiter().clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                limiter.retain_recent();
            }
        });
        tracing::info!(per_ip_rps = r, "per-IP rate limiting enabled");
        app.layer(GovernorLayer { config: conf })
    } else {
        app
    };

    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
        AppError::new(
            crate::errors::codes::GATEWAY_BIND_ERROR,
            format!("Invalid address: {e}"),
        )
    })?;

    // axum_server 接受 std::net::TcpListener。先用 std bind 拿到 bound_port（port=0
    // 时 OS 才分配），set_nonblocking 让 tokio runtime 能 poll。
    let std_listener = std::net::TcpListener::bind(addr).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            AppError::new(
                crate::errors::codes::GATEWAY_PORT_IN_USE,
                "Gateway port is already in use",
            )
            .with_detail(format!("{host}:{port}"))
            .with_suggestion(
                "Change the gateway port in Settings or stop the process using this port",
            )
        } else {
            AppError::new(
                crate::errors::codes::GATEWAY_BIND_ERROR,
                format!("Failed to bind: {e}"),
            )
        }
    })?;
    std_listener.set_nonblocking(true).map_err(|e| {
        AppError::new(
            crate::errors::codes::GATEWAY_BIND_ERROR,
            format!("set_nonblocking failed: {e}"),
        )
    })?;

    let bound_port = std_listener.local_addr().map(|a| a.port()).unwrap_or(port);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server_handle = axum_server::Handle::new();

    // 桥接 oneshot → axum_server::Handle::graceful_shutdown。
    // shutdown_tx.send(()) 触发后，axum_server 停接新连接、等 in-flight 完成
    // 直到 GRACEFUL_SHUTDOWN_TIMEOUT 再强切——比原 oneshot+5s 强杀 SSE 友好得多。
    {
        let sh = server_handle.clone();
        tokio::spawn(async move {
            let _ = shutdown_rx.await;
            sh.graceful_shutdown(Some(GRACEFUL_SHUTDOWN_TIMEOUT));
        });
    }

    // with_connect_info:SmartIpKeyExtractor 在没有 X-Forwarded-For 时回落到 peer IP。
    let make_service = app.into_make_service_with_connect_info::<SocketAddr>();

    tracing::info!(
        host = %host,
        port = bound_port,
        tls = tls.is_some(),
        "gateway listening"
    );

    let join_handle = if let Some(tls_cfg) = tls {
        // HTTPS：加载 cert/key，启 axum_server with TLS
        let rustls = RustlsConfig::from_pem_file(&tls_cfg.cert_path, &tls_cfg.key_path)
            .await
            .map_err(|e| {
                AppError::new(
                    crate::errors::codes::GATEWAY_TLS_LOAD_FAILED,
                    format!("Failed to load TLS cert/key: {e}"),
                )
                .with_detail(format!(
                    "cert={} key={}",
                    tls_cfg.cert_path.display(),
                    tls_cfg.key_path.display()
                ))
                .with_suggestion("Verify both files exist and are valid PEM-encoded")
            })?;
        tokio::spawn(async move {
            let _ = axum_server::from_tcp_rustls(std_listener, rustls)
                .handle(server_handle)
                .serve(make_service)
                .await;
        })
    } else {
        // HTTP
        tokio::spawn(async move {
            let _ = axum_server::from_tcp(std_listener)
                .handle(server_handle)
                .serve(make_service)
                .await;
        })
    };

    Ok((shutdown_tx, join_handle, active_requests, bound_port))
}
