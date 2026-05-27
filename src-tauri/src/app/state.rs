use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::models::gateway::GatewayRuntimeState;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub gateway_runtime: Arc<Mutex<GatewayRuntimeState>>,
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
        };
        assert!(state.gateway_runtime.lock().unwrap().port == 0);
    }
}
