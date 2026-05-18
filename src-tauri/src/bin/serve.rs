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

    /// Quick-setup: provider type (deepseek, openai, anthropic, kimi, etc.)
    #[arg(long, env = "AGENTGATE_PROVIDER")]
    provider: Option<String>,

    /// Quick-setup: API key
    #[arg(long, env = "AGENTGATE_API_KEY")]
    api_key: Option<String>,

    /// Quick-setup: default model
    #[arg(long, env = "AGENTGATE_MODEL")]
    model: Option<String>,

    /// Quick-setup: provider base URL (auto-filled from provider type if omitted)
    #[arg(long, env = "AGENTGATE_BASE_URL")]
    base_url: Option<String>,
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

    // Quick-setup: create provider from CLI args / env vars if no providers exist
    if let Some(ref provider_type) = cli.provider {
        let conn = db.lock().unwrap();
        let existing = agentgate_lib::storage::providers::list_all(&conn).unwrap_or_default();
        if existing.is_empty() || std::env::var("AGENTGATE_FORCE_SETUP").is_ok() {
            let presets: std::collections::HashMap<&str, (&str, &str)> = [
                ("deepseek", ("https://api.deepseek.com", "deepseek-v4-pro")),
                ("openai", ("https://api.openai.com", "gpt-4o")),
                ("anthropic", ("https://api.anthropic.com", "claude-sonnet-4-6")),
                ("kimi", ("https://api.moonshot.cn", "kimi-k2")),
                ("minimax", ("https://api.minimax.chat", "MiniMax-M1")),
                ("groq", ("https://api.groq.com/openai", "llama-3.3-70b-versatile")),
                ("together", ("https://api.together.xyz", "meta-llama/Llama-3.3-70B-Instruct-Turbo")),
                ("google_gemini", ("https://generativelanguage.googleapis.com/v1beta/openai", "gemini-2.5-flash")),
            ].into_iter().collect();

            let (default_url, default_model) = presets.get(provider_type.as_str()).copied().unwrap_or(("", ""));
            let base_url = cli.base_url.as_deref().unwrap_or(default_url);
            let model = cli.model.as_deref().unwrap_or(default_model);

            if base_url.is_empty() || model.is_empty() {
                eprintln!("Error: --base-url and --model required for unknown provider type '{provider_type}'");
                std::process::exit(1);
            }

            let api_key = cli.api_key.clone().or_else(|| std::env::var("AGENTGATE_API_KEY").ok());
            if api_key.is_none() {
                eprintln!("Error: --api-key or AGENTGATE_API_KEY required for quick setup");
                std::process::exit(1);
            }

            let label = provider_type[..1].to_uppercase() + &provider_type[1..];
            let input = agentgate_lib::models::provider::CreateProviderInput {
                name: label.clone(),
                provider_type: provider_type.clone(),
                base_url: base_url.to_string(),
                api_key,
                default_model: model.to_string(),
                reasoning_model: None,
                supported_models: None,
                model_mapping: None,
                extra_headers: None,
                anthropic_base_url: None,
                responses_base_url: None,
                auto_cache_control: None,
                protocol: "openai_chat_completions".to_string(),
                timeout_seconds: Some(120),
                enabled: Some(true),
            };

            match agentgate_lib::storage::providers::create(&conn, input) {
                Ok(p) => {
                    let _ = agentgate_lib::storage::providers::set_active(&conn, &p.id);
                    eprintln!("  Provider: {} ({}) → {} [auto-created]", label, provider_type, base_url);
                }
                Err(e) => eprintln!("  Warning: failed to create provider: {}", e.message),
            }
        } else {
            eprintln!("  Provider: {} existing providers (skip quick-setup)", existing.len());
        }
        drop(conn);
    }

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
