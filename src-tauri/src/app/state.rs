use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::models::gateway::GatewayRuntimeState;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub gateway_runtime: Arc<Mutex<GatewayRuntimeState>>,
    /// 宠物窗口鼠标穿透开关(运行时状态,不持久化)。
    /// 多窗口共享:右键菜单 / tray / Settings 三处都可改;
    /// 改动后 emit `pet-click-through-changed` 通知所有 webview。
    pub pet_click_through: Arc<Mutex<bool>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_creation() {
        let conn = Connection::open_in_memory().unwrap();
        let state = AppState {
            db: Arc::new(Mutex::new(conn)),
            gateway_runtime: Arc::new(Mutex::new(GatewayRuntimeState::default())),
            pet_click_through: Arc::new(Mutex::new(false)),
        };
        assert!(state.gateway_runtime.lock().unwrap().port == 0);
    }
}
