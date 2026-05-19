use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewaySettings {
    pub id: i64,
    pub host: String,
    pub port: i64,
    pub active_provider_id: Option<String>,
    pub input_protocol: String,
    pub output_protocol: String,
    pub auto_start: bool,
    pub log_retention_days: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateGatewaySettingsInput {
    pub host: Option<String>,
    pub port: Option<i64>,
    pub active_provider_id: Option<String>,
    pub input_protocol: Option<String>,
    pub output_protocol: Option<String>,
    pub auto_start: Option<bool>,
    pub log_retention_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewayStatus {
    pub running: bool,
    pub host: String,
    pub port: i64,
    pub active_provider: Option<String>,
    pub input_protocol: String,
    pub output_protocol: String,
    pub started_at: Option<String>,
}

/// Runtime state for the gateway HTTP server.
/// The shutdown_tx and server_handle cannot be Clone,
/// so they live behind Option and are taken when stopping.
pub struct GatewayRuntimeState {
    pub running: bool,
    pub host: String,
    pub port: u16,
    pub started_at: Option<String>,
    pub shutdown_tx: Option<oneshot::Sender<()>>,
    pub server_handle: Option<JoinHandle<()>>,
    pub active_requests: Option<Arc<AtomicU64>>,
}

impl Default for GatewayRuntimeState {
    fn default() -> Self {
        Self {
            running: false,
            host: String::new(),
            port: 0,
            started_at: None,
            shutdown_tx: None,
            server_handle: None,
            active_requests: None,
        }
    }
}
