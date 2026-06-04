//! 后台主动健康探测：定期对启用的 provider 发 1-token 探测请求（复用 speedtest），
//! 结果写入 provider_runtime_status 的 last_probe_* 列。**仅用于展示，不影响路由**
//! ——绝不碰 available / cooldown。受 gateway_settings.health_probe_enabled 控制，
//! 默认关（开启才探测，会消耗少量额度）。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;

/// 启动延迟：让网关 / DB 先就绪。
const STARTUP_DELAY: Duration = Duration::from_secs(30);
/// 探测间隔。固定 10 分钟——1-token 探测开销极小，10 分钟粒度足够发现 provider 掉线。
const INTERVAL: Duration = Duration::from_secs(600);

/// 启动后台探测循环。db 句柄由 AppState 传入。
/// 用 tauri::async_runtime::spawn（与项目其它后台任务一致）——Tauri setup 是同步
/// 上下文，直接 tokio::spawn 会 panic "no reactor running"。
pub fn spawn(db: Arc<Mutex<Connection>>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        loop {
            run_once(&db).await;
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

async fn run_once(db: &Arc<Mutex<Connection>>) {
    // 短暂持锁读开关 + provider 列表；持锁期间绝不 await。
    let providers = {
        let conn = match db.lock() {
            Ok(c) => c,
            Err(p) => p.into_inner(),
        };
        match crate::storage::gateway_settings::get(&conn) {
            Ok(s) if s.health_probe_enabled => {}
            Ok(_) => return, // 未开启：正常跳过
            Err(e) => {
                eprintln!("[health_probe] 读取设置失败，跳过本轮: {e}");
                return;
            }
        }
        match crate::storage::providers::list_all(&conn) {
            Ok(ps) => ps.into_iter().filter(|p| p.enabled).collect::<Vec<_>>(),
            Err(e) => {
                eprintln!("[health_probe] 读取 provider 列表失败，跳过本轮: {e}");
                return;
            }
        }
    };
    if providers.is_empty() {
        return;
    }

    // 探测阶段不持锁（要发 HTTP）。复用 speedtest 的 1-token 探测。
    let reports = match crate::diagnostics::speedtest::probe_many(&providers).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[health_probe] 探测失败，跳过本轮: {e}");
            return;
        }
    };

    // 短暂持锁写结果。
    let conn = match db.lock() {
        Ok(c) => c,
        Err(p) => p.into_inner(),
    };
    for r in reports {
        if let Err(e) = crate::storage::provider_runtime_status::record_probe(
            &conn,
            &r.provider_id,
            r.success,
            r.total_ms as i64,
            r.error.as_deref(),
        ) {
            eprintln!("[health_probe] 写入探测结果失败 ({}): {e}", r.provider_id);
        }
    }
}
