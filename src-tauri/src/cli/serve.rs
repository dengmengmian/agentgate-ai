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

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Parser)]
#[command(name = "agentgate", about = "AgentGate — Local AI gateway for coding agents", version)]
struct Cli {
    /// Database directory path
    #[arg(long, global = true, env = "AGENTGATE_DB_PATH")]
    db_path: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the gateway server
    Serve {
        /// Host to bind to
        #[arg(long, env = "AGENTGATE_HOST", default_value = "127.0.0.1")]
        host: String,
        /// Port to listen on
        #[arg(long, short, env = "AGENTGATE_PORT", default_value = "9090")]
        port: u16,
        /// PEM-encoded TLS certificate file path. Provide both --tls-cert and --tls-key to enable HTTPS.
        #[arg(long, env = "AGENTGATE_TLS_CERT")]
        tls_cert: Option<PathBuf>,
        /// PEM-encoded TLS private key file path.
        #[arg(long, env = "AGENTGATE_TLS_KEY")]
        tls_key: Option<PathBuf>,
    },
    /// Add a provider
    #[command(name = "provider-add")]
    ProviderAdd {
        /// Provider type (deepseek, openai, anthropic, kimi, minimax, groq, etc.)
        #[arg(long, short = 't')]
        r#type: String,
        /// Display name
        #[arg(long, short)]
        name: Option<String>,
        /// API key
        #[arg(long, short = 'k', env = "AGENTGATE_API_KEY")]
        api_key: String,
        /// Base URL (auto-filled from type if omitted)
        #[arg(long)]
        base_url: Option<String>,
        /// Default model (auto-filled from type if omitted)
        #[arg(long, short)]
        model: Option<String>,
        /// Set as active provider
        #[arg(long, default_value = "true")]
        active: bool,
    },
    /// List all providers
    #[command(name = "provider-list")]
    ProviderList,
    /// Remove a provider by name
    #[command(name = "provider-remove")]
    ProviderRemove {
        /// Provider name to remove
        name: String,
    },
    /// Show the gateway access token
    Token,
    /// Regenerate the gateway access token
    #[command(name = "token-regen")]
    TokenRegen,
    /// Show gateway status and config summary
    Status,
}

fn get_db_dir(cli: &Cli) -> PathBuf {
    if let Some(ref path) = cli.db_path {
        PathBuf::from(path)
    } else {
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".agentgate")
    }
}

fn open_db(cli: &Cli) -> rusqlite::Connection {
    let db_dir = get_db_dir(cli);
    match agentgate_lib::storage::db::init_database(&db_dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e.message);
            std::process::exit(1);
        }
    }
}

/// Provider type presets: (base_url, default_model)
fn provider_presets() -> std::collections::HashMap<&'static str, (&'static str, &'static str)> {
    [
        ("deepseek", ("https://api.deepseek.com", "deepseek-v4-pro")),
        ("openai", ("https://api.openai.com", "gpt-4o")),
        ("anthropic", ("https://api.anthropic.com", "claude-sonnet-4-7")),
        ("kimi", ("https://api.moonshot.cn", "kimi-k2")),
        ("minimax", ("https://api.minimax.chat", "MiniMax-M1")),
        ("groq", ("https://api.groq.com/openai", "llama-3.3-70b-versatile")),
        ("together", ("https://api.together.xyz", "meta-llama/Llama-3.3-70B-Instruct-Turbo")),
        ("google_gemini", ("https://generativelanguage.googleapis.com/v1beta/openai", "gemini-2.5-flash")),
        ("xai", ("https://api.x.ai", "grok-3-latest")),
        ("mistral", ("https://api.mistral.ai", "mistral-large-latest")),
    ].into_iter().collect()
}

/// 初始化结构化日志（JSON to stdout）。AGENTGATE_LOG env-filter 控制级别，
/// 默认 info（含 reqwest=warn，避免每次请求一条 hyper trace）。
/// 输出 schema: {timestamp, level, target, fields, message}。
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_env("AGENTGATE_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info,reqwest=warn,hyper=warn,h2=warn,rustls=warn"));
    fmt()
        .with_env_filter(filter)
        .json()
        .with_current_span(false)
        .with_span_list(false)
        .with_target(true)
        .flatten_event(true)
        .init();
}

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Serve { host, port, tls_cert, tls_key }) => {
            cmd_serve(&cli, host, *port, tls_cert.clone(), tls_key.clone()).await
        }
        Some(Commands::ProviderAdd { r#type, name, api_key, base_url, model, active }) => {
            cmd_provider_add(&cli, r#type, name.as_deref(), api_key, base_url.as_deref(), model.as_deref(), *active);
        }
        Some(Commands::ProviderList) => cmd_provider_list(&cli),
        Some(Commands::ProviderRemove { name }) => cmd_provider_remove(&cli, name),
        Some(Commands::Token) => cmd_token(),
        Some(Commands::TokenRegen) => cmd_token_regen(),
        Some(Commands::Status) => cmd_status(&cli),
        None => {
            // Default: serve. env-based TLS（用户不传子命令 + 仅靠 env 配置 TLS）。
            let host = std::env::var("AGENTGATE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
            let port = std::env::var("AGENTGATE_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
                .unwrap_or(9090);
            let tls_cert = std::env::var("AGENTGATE_TLS_CERT").ok().map(PathBuf::from);
            let tls_key = std::env::var("AGENTGATE_TLS_KEY").ok().map(PathBuf::from);
            cmd_serve(&cli, &host, port, tls_cert, tls_key).await;
        }
    }
}

async fn cmd_serve(
    cli: &Cli,
    host: &str,
    port: u16,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
) {
    let db_dir = get_db_dir(cli);
    let conn = open_db(cli);
    let db = Arc::new(Mutex::new(conn));

    let _ = agentgate_lib::security::local_token::ensure_token();
    let token = agentgate_lib::security::local_token::read_token().unwrap_or_default();

    let provider_count = {
        let c = db.lock().unwrap();
        agentgate_lib::storage::providers::list_all(&c).map(|p| p.len()).unwrap_or(0)
    };

    // 只提供 cert 不提供 key（或反之）直接报错——避免用户以为 TLS 开了实际还是 HTTP。
    let tls = match (tls_cert, tls_key) {
        (Some(c), Some(k)) => Some(agentgate_lib::gateway::server::TlsConfig {
            cert_path: c,
            key_path: k,
        }),
        (None, None) => None,
        (Some(_), None) | (None, Some(_)) => {
            eprintln!("Error: --tls-cert and --tls-key must be provided together");
            std::process::exit(2);
        }
    };
    let scheme = if tls.is_some() { "https" } else { "http" };

    eprintln!("AgentGate headless server");
    eprintln!("  Database:   {}", db_dir.display());
    eprintln!("  Providers:  {}", if provider_count > 0 { format!("{provider_count} configured") } else { "none (use `agentgate provider-add` to configure)".to_string() });
    eprintln!("  Token:      {}...{}", &token[..8.min(token.len())], &token[token.len().saturating_sub(4)..]);
    if tls.is_some() {
        eprintln!("  TLS:        enabled (HTTPS)");
    }
    eprintln!();

    // SIGHUP 监听：收到信号清空内存缓存（session_affinity + provider runtime
    // status），DB 里的 provider 配置每次请求都即时读，本来就热的。
    #[cfg(unix)]
    install_sighup_handler(db.clone());

    match agentgate_lib::gateway::server::start(host, port, db, tls).await {
        Ok((shutdown_tx, handle, _active_requests, _port)) => {
            eprintln!("Gateway running on {scheme}://{host}:{port}");
            eprintln!("Send SIGINT (Ctrl+C) or SIGTERM to stop. SIGHUP to reload caches.");
            eprintln!();
            wait_shutdown_signal().await;
            eprintln!("\nShutting down (graceful, up to 30s)...");
            tracing::info!("shutdown signal received, draining in-flight requests");
            let _ = shutdown_tx.send(());
            let _ = handle.await;
            tracing::info!("server stopped");
        }
        Err(e) => {
            eprintln!("Failed to start: {}", e.message);
            if let Some(ref s) = e.suggestion { eprintln!("  {s}"); }
            std::process::exit(1);
        }
    }
}

/// SIGHUP 热重载：清 session_affinity 内存缓存 + DB 里 provider_runtime_status
/// 全部重置为 healthy。背景循环 task，永不退出（直到进程死）。
/// provider 配置（base_url/api_key/model_mapping 等）本来每次请求查 DB 已经
/// 即时生效，所以 SIGHUP 不需要触发 DB-side reload。
#[cfg(unix)]
fn install_sighup_handler(db: Arc<Mutex<rusqlite::Connection>>) {
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut hup = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to install SIGHUP handler — hot reload disabled");
                return;
            }
        };
        loop {
            hup.recv().await;
            tracing::info!("SIGHUP received, clearing runtime caches");
            agentgate_lib::gateway::session_affinity::clear();
            if let Ok(c) = db.lock() {
                if let Err(e) = agentgate_lib::storage::provider_runtime_status::reset_all(&c) {
                    tracing::warn!(error = %e.message, "SIGHUP: reset_all failed");
                }
            }
            tracing::info!("SIGHUP reload complete");
        }
    });
}

/// 监听 shutdown 信号：unix 同时收 SIGINT + SIGTERM，window 走 ctrl-c。
/// 任一信号到达就 return。
async fn wait_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to install SIGTERM handler: {e}");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        let mut int = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to install SIGINT handler: {e}");
                let _ = tokio::signal::ctrl_c().await;
                return;
            }
        };
        tokio::select! {
            _ = term.recv() => tracing::info!("SIGTERM received"),
            _ = int.recv() => tracing::info!("SIGINT received"),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Ctrl-C received");
    }
}

fn cmd_provider_add(cli: &Cli, provider_type: &str, name: Option<&str>, api_key: &str, base_url: Option<&str>, model: Option<&str>, active: bool) {
    let conn = open_db(cli);
    let presets = provider_presets();
    let (default_url, default_model) = presets.get(provider_type).copied().unwrap_or(("", ""));

    let base_url = base_url.unwrap_or(default_url);
    let model = model.unwrap_or(default_model);

    if base_url.is_empty() {
        eprintln!("Error: --base-url required for provider type '{provider_type}'");
        std::process::exit(1);
    }
    if model.is_empty() {
        eprintln!("Error: --model required for provider type '{provider_type}'");
        std::process::exit(1);
    }

    let label = if let Some(n) = name {
        n.to_string()
    } else {
        let mut s = provider_type[..1].to_uppercase();
        s.push_str(&provider_type[1..]);
        s
    };

    let mut input = agentgate_lib::models::provider::CreateProviderInput {
        name: label.clone(),
        provider_type: provider_type.to_string(),
        base_url: base_url.to_string(),
        api_key: Some(api_key.to_string()),
        default_model: model.to_string(),
        reasoning_model: None,
        supported_models: None,
        model_mapping: None,
        extra_headers: None,
        anthropic_base_url: None,
        responses_base_url: None,
        auto_cache_control: None,
        model_capabilities: None,
        protocol: "openai_chat_completions".to_string(),
        timeout_seconds: Some(120),
        enabled: Some(true),
    };
    agentgate_lib::storage::recommended_mappings::apply_to_create_input(&mut input);

    match agentgate_lib::storage::providers::create(&conn, input) {
        Ok(p) => {
            if active {
                let _ = agentgate_lib::storage::providers::set_active(&conn, &p.id);
            }
            println!("✓ Provider '{}' created (type: {}, model: {}, active: {})", label, provider_type, model, active);
        }
        Err(e) => {
            eprintln!("Failed to create provider: {}", e.message);
            std::process::exit(1);
        }
    }
}

fn cmd_provider_list(cli: &Cli) {
    let conn = open_db(cli);
    let providers = agentgate_lib::storage::providers::list_all(&conn).unwrap_or_default();

    if providers.is_empty() {
        println!("No providers configured. Use `agentgate provider-add` to add one.");
        return;
    }

    println!("{:<4} {:<20} {:<15} {:<35} {:<25} {}", "#", "Name", "Type", "Base URL", "Model", "Status");
    println!("{}", "-".repeat(110));
    for (i, p) in providers.iter().enumerate() {
        let active = if p.is_active { " *" } else { "" };
        let key_status = if p.api_key.as_ref().map_or(false, |k| !k.is_empty()) { "✓ key" } else { "✗ no key" };
        println!("{:<4} {:<20} {:<15} {:<35} {:<25} {}{}",
            i + 1,
            &p.name[..p.name.len().min(18)],
            &p.provider_type[..p.provider_type.len().min(13)],
            &p.base_url[..p.base_url.len().min(33)],
            &p.default_model[..p.default_model.len().min(23)],
            key_status,
            active,
        );
    }
    println!("\n  * = active provider");
}

fn cmd_provider_remove(cli: &Cli, name: &str) {
    let conn = open_db(cli);
    let providers = agentgate_lib::storage::providers::list_all(&conn).unwrap_or_default();
    let target = providers.iter().find(|p| p.name.eq_ignore_ascii_case(name));

    match target {
        Some(p) => {
            match agentgate_lib::storage::providers::delete(&conn, &p.id) {
                Ok(_) => println!("✓ Provider '{}' removed", p.name),
                Err(e) => { eprintln!("Failed: {}", e.message); std::process::exit(1); }
            }
        }
        None => {
            eprintln!("Provider '{}' not found. Use `agentgate provider-list` to see all.", name);
            std::process::exit(1);
        }
    }
}

fn cmd_token() {
    let _ = agentgate_lib::security::local_token::ensure_token();
    match agentgate_lib::security::local_token::read_token() {
        Ok(token) => println!("{token}"),
        Err(e) => { eprintln!("Failed to read token: {}", e.message); std::process::exit(1); }
    }
}

fn cmd_token_regen() {
    match agentgate_lib::security::local_token::regenerate_token() {
        Ok(token) => println!("✓ New token: {token}"),
        Err(e) => { eprintln!("Failed: {}", e.message); std::process::exit(1); }
    }
}

fn cmd_status(cli: &Cli) {
    let db_dir = get_db_dir(cli);
    let conn = open_db(cli);
    let providers = agentgate_lib::storage::providers::list_all(&conn).unwrap_or_default();
    let active = providers.iter().find(|p| p.is_active);
    let token = agentgate_lib::security::local_token::read_token().unwrap_or_default();

    println!("AgentGate Status");
    println!("  Database:   {}", db_dir.display());
    println!("  Providers:  {} configured", providers.len());
    if let Some(a) = active {
        println!("  Active:     {} ({} → {})", a.name, a.provider_type, a.default_model);
    }
    println!("  Token:      {}...{}", &token[..8.min(token.len())], &token[token.len().saturating_sub(4)..]);
}
