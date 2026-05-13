use rusqlite::Connection;
use std::sync::{Arc, Mutex};

use crate::models::gateway::GatewayRuntimeState;

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub gateway_runtime: Arc<Mutex<GatewayRuntimeState>>,
}
