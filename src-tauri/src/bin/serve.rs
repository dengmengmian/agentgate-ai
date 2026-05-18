//! Headless AgentGate server — runs without Tauri/GUI.
//!
//! Usage:
//!   agentgate-serve                          # defaults: 127.0.0.1:9090
//!   agentgate-serve --port 8080              # custom port
//!   agentgate-serve --host 0.0.0.0 --port 80 # bind all interfaces
//!   AGENTGATE_PORT=8080 agentgate-serve      # env var config
//!
//! Environment variables:
//!   AGENTGATE_HOST     — bind address (default: 127.0.0.1)
//!   AGENTGATE_PORT     — port (default: 9090)
//!   AGENTGATE_DB_PATH  — SQLite database directory (default: ~/.agentgate)

use clap::Parser;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(name = "agentgate-serve", about = "AgentGate headless server", version)]
struct Cli {
    /// Host to bind to
    #[arg(long, env = "AGENTGATE_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Port to listen on
    #[arg(long, short, env = "AGENTGATE_PORT", default_value = "9090")]
    port: u16,

    /// Database directory path
    #[arg(long, env = "AGENTGATE_DB_PATH")]
    db_path: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Determine DB path
    let db_dir = if let Some(ref path) = cli.db_path {
        PathBuf::from(path)
    } else {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".agentgate")
    };

    eprintln!("AgentGate headless server");
    eprintln!("  Database: {}", db_dir.display());
    eprintln!("  Binding:  {}:{}", cli.host, cli.port);

    // Initialize database
    let conn = match agentgate_lib::storage::db::init_database(&db_dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e.message);
            std::process::exit(1);
        }
    };

    let db = Arc::new(Mutex::new(conn));

    // Ensure local access token exists
    let _ = agentgate_lib::security::local_token::ensure_token();

    let token = agentgate_lib::security::local_token::read_token()
        .unwrap_or_else(|_| "unknown".to_string());
    eprintln!("  Token:    {}...{}", &token[..8.min(token.len())], &token[token.len().saturating_sub(4)..]);
    eprintln!();

    // Start the gateway server
    match agentgate_lib::gateway::server::start(&cli.host, cli.port, db).await {
        Ok((_shutdown_tx, handle)) => {
            eprintln!("Gateway running on http://{}:{}", cli.host, cli.port);
            eprintln!("Press Ctrl+C to stop.");
            eprintln!();

            // Wait for Ctrl+C
            tokio::signal::ctrl_c().await.ok();
            eprintln!("\nShutting down...");

            // The handle will be dropped, stopping the server
            drop(handle);
        }
        Err(e) => {
            eprintln!("Failed to start gateway: {}", e.message);
            if let Some(ref detail) = e.detail {
                eprintln!("  Detail: {detail}");
            }
            if let Some(ref suggestion) = e.suggestion {
                eprintln!("  Suggestion: {suggestion}");
            }
            std::process::exit(1);
        }
    }
}
