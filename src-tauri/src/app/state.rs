use std::sync::{Arc, Mutex};

use crate::models::gateway::GatewayRuntimeState;
use crate::storage::db::DbPool;

pub struct AppState {
    /// 连接池(r2d2 + SqliteConnectionManager)。各处用 `state.db.get()` 借连接,
    /// 取代旧的全局 Mutex<Connection>。Pool 本身是 Clone + Send,
    /// 内部 Arc 共享池状态——AppState 持有 owned Pool 即可,无需再包 Arc。
    pub db: DbPool,
    pub gateway_runtime: Arc<Mutex<GatewayRuntimeState>>,
    /// 宠物窗口鼠标穿透开关(运行时状态,不持久化)。
    /// 多窗口共享:右键菜单 / tray / Settings 三处都可改;
    /// 改动后 emit `pet-click-through-changed` 通知所有 webview。
    pub pet_click_through: Arc<Mutex<bool>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2d2::Pool;
    use r2d2_sqlite::SqliteConnectionManager;

    #[test]
    fn app_state_creation() {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();
        let state = AppState {
            db: pool,
            gateway_runtime: Arc::new(Mutex::new(GatewayRuntimeState::default())),
            pet_click_through: Arc::new(Mutex::new(false)),
        };
        assert!(state.gateway_runtime.lock().unwrap().port == 0);
    }
}
