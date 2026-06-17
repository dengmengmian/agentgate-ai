//! Prometheus metrics 暴露层。
//!
//! 指标列表：
//! - `agentgate_requests_total{route,client,provider,status}` — counter
//! - `agentgate_request_duration_seconds{route,client,provider}` — histogram
//! - `agentgate_active_requests` — gauge（实时在飞请求数）
//! - `agentgate_upstream_tokens_total{provider,model,direction}` — counter（input/output）
//! - `agentgate_failover_attempts_total{from_provider,reason}` — counter
//!
//! 接入：`/metrics` endpoint 返回 Prometheus text 格式，监控系统按 scrape 拉。
//! 初始化在 server::start 里调一次 init()；记录点散布在 routes.rs / pass_through.rs /
//! provider_selector.rs。

use axum::response::IntoResponse;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

/// 全局唯一 PrometheusHandle —— OnceLock 保证 init 幂等（重复调 init 没事）。
static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// 初始化 Prometheus recorder。失败返回 false 但不 panic（已经被另一个 init 占了）。
pub fn init() -> bool {
    if HANDLE.get().is_some() {
        return false;
    }
    let builder = PrometheusBuilder::new();
    match builder.install_recorder() {
        Ok(handle) => {
            // 第一次 set 成功，后续重复 init 直接返回。
            let _ = HANDLE.set(handle);
            true
        }
        Err(_) => {
            // 推荐做法：失败也不阻断 server 启动。/metrics endpoint 会显示空。
            false
        }
    }
}

/// GET /metrics handler —— 返回 Prometheus text format。
pub async fn render() -> impl IntoResponse {
    match HANDLE.get() {
        Some(h) => (
            axum::http::StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4",
            )],
            h.render(),
        ),
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            "metrics recorder not initialized\n".to_string(),
        ),
    }
}

// ── Convenience helpers — 让 hotpath 调用点不用记 metric 名 ──

/// 记一次完整请求的结果：route + client + provider + status_code。
pub fn record_request(route: &str, client: &str, provider: &str, status: u16, latency_secs: f64) {
    metrics::counter!(
        "agentgate_requests_total",
        "route" => route.to_string(),
        "client" => client.to_string(),
        "provider" => provider.to_string(),
        "status" => status.to_string(),
    )
    .increment(1);
    metrics::histogram!(
        "agentgate_request_duration_seconds",
        "route" => route.to_string(),
        "client" => client.to_string(),
        "provider" => provider.to_string(),
    )
    .record(latency_secs);
}

/// 记 upstream token 用量（input / output / cache_read / cache_creation 分别记）。
pub fn record_tokens(provider: &str, model: &str, direction: &str, count: i64) {
    if count <= 0 {
        return;
    }
    metrics::counter!(
        "agentgate_upstream_tokens_total",
        "provider" => provider.to_string(),
        "model" => model.to_string(),
        "direction" => direction.to_string(),
    )
    .increment(count as u64);
}

/// 记 failover 尝试：上一条 provider 挂的原因（http 状态码或 net-error 类）。
pub fn record_failover(from_provider: &str, reason: &str) {
    metrics::counter!(
        "agentgate_failover_attempts_total",
        "from_provider" => from_provider.to_string(),
        "reason" => reason.to_string(),
    )
    .increment(1);
}

/// 实时活跃请求数（gauge），从 server.rs::CountingBody 的 AtomicU64 镜像而来。
/// 在 /metrics render 之前 sync 一次。
pub fn set_active_requests(n: u64) {
    metrics::gauge!("agentgate_active_requests").set(n as f64);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn init_second_call_returns_false() {
        // First call may have already happened in another test in the same binary,
        // so we only assert on the idempotent property.
        let _ = init();
        assert!(!init());
    }

    #[tokio::test]
    #[serial]
    async fn render_after_init_returns_ok() {
        // Other tests may have already installed the global recorder; init is idempotent.
        let _ = init();
        record_request("/v1/chat/completions", "Codex", "openai", 200, 0.123);
        record_tokens("openai", "gpt-5", "input", 100);
        record_failover("openai", "429");
        set_active_requests(3);

        let response = render().await;
        let (parts, body) = response.into_response().into_parts();
        assert_eq!(parts.status, axum::http::StatusCode::OK);
        let body_text = body_to_string(body).await;
        assert!(body_text.contains("agentgate_requests_total"));
        assert!(body_text.contains("agentgate_upstream_tokens_total"));
        assert!(body_text.contains("agentgate_failover_attempts_total"));
        assert!(body_text.contains("agentgate_active_requests"));
    }

    async fn body_to_string(body: axum::body::Body) -> String {
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        String::from_utf8_lossy(&bytes).to_string()
    }

    #[test]
    #[serial]
    fn record_tokens_ignores_non_positive_counts() {
        // Just ensure it does not panic; recorder may or may not be installed.
        record_tokens("p", "m", "input", 0);
        record_tokens("p", "m", "input", -5);
    }
}
