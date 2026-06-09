//! 后台任务 spawn 的运行时抽象。
//!
//! desktop:用 Tauri 托管的 runtime —— Tauri 的 `setup` 是同步上下文,在那里直接
//! `tokio::spawn` 会 panic「no reactor running」,必须走 `tauri::async_runtime`。
//! headless:`agentgate-serve` 本身在 `#[tokio::main]` 下,直接 `tokio::spawn`。

use std::future::Future;

#[cfg(feature = "desktop")]
pub fn spawn<F>(future: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tauri::async_runtime::spawn(future);
}

#[cfg(not(feature = "desktop"))]
pub fn spawn<F>(future: F)
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future);
}
