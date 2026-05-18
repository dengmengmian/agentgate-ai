mod app;
mod diagnostics;
pub mod errors;
pub mod gateway;
pub mod models;
pub mod protocol;
pub mod providers;
pub mod security;
pub mod storage;
mod tools;
pub mod transform;

use std::sync::{Arc, Mutex};
use tauri::Manager;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;

use app::commands;
use app::state::AppState;
use models::gateway::GatewayRuntimeState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            let conn = storage::db::init_database(&app_data_dir)
                .expect("Failed to initialize database");

            let state = AppState {
                db: Arc::new(Mutex::new(conn)),
                gateway_runtime: Arc::new(Mutex::new(GatewayRuntimeState::default())),
            };

            let cleanup_db = state.db.clone();
            app.manage(state);

            // ── Ensure local access token exists ──
            let _ = security::local_token::ensure_token();

            // ── Periodic log cleanup ──
            {
                let db = cleanup_db;
                tauri::async_runtime::spawn(async move {
                    loop {
                        if let Ok(conn) = db.lock() {
                            let days = storage::gateway_settings::get(&conn)
                                .map(|s| s.log_retention_days)
                                .unwrap_or(14);
                            if let Ok(n) = storage::request_logs::cleanup_older_than(&conn, days) {
                                if n > 0 {
                                    tracing::info!("Cleaned up {n} old request logs (retention: {days} days)");
                                }
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    }
                });
            }

            // ── System Tray ──
            setup_tray(app)?;

            // ── Auto-start Gateway if enabled ──
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Small delay to let app fully initialize
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let state: tauri::State<'_, AppState> = app_handle.state();
                    let should_start = state.db.lock()
                        .ok()
                        .and_then(|conn| storage::gateway_settings::get(&conn).ok())
                        .map(|s| s.auto_start)
                        .unwrap_or(false);
                    if should_start {
                        let _ = commands::start_gateway(state).await;
                    }
                });
            }

            // ── Close-to-tray: hide window on close ──
            let window = app.get_webview_window("main").unwrap();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = event;
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Hide window instead of closing
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Providers
            commands::list_providers,
            commands::get_provider,
            commands::create_provider,
            commands::update_provider,
            commands::delete_provider,
            commands::set_active_provider,
            commands::fetch_provider_models,
            commands::test_provider,
            commands::detect_provider_vision,
            // Gateway
            commands::get_gateway_status,
            commands::get_gateway_settings,
            commands::update_gateway_settings,
            commands::start_gateway,
            commands::stop_gateway,
            commands::restart_gateway,
            // Logs
            commands::list_request_logs,
            commands::get_request_log_detail,
            commands::clear_request_logs,
            // Tools
            commands::list_tools,
            commands::generate_codex_config,
            // Gateway Auth
            commands::get_gateway_auth_settings,
            commands::regenerate_local_access_token,
            commands::ensure_local_access_token,
            commands::get_local_access_token,
            commands::open_token_folder,
            // Codex Config
            commands::detect_codex_config,
            commands::apply_codex_config,
            commands::toggle_codex_provider,
            commands::open_codex_config,
            // Claude Code
            commands::detect_claude_code_env,
            commands::apply_claude_code_config,
            commands::toggle_claude_code_provider,
            commands::open_claude_code_config,
            commands::generate_claude_code_env,
            // OpenCode
            commands::detect_opencode_config,
            commands::apply_opencode_config,
            commands::generate_opencode_config,
            commands::open_opencode_config,
            // Gemini CLI
            commands::detect_gemini_config,
            commands::apply_gemini_config,
            commands::generate_gemini_config,
            commands::toggle_gemini_provider,
            commands::open_gemini_config,
            commands::detect_provider_cache,
            commands::get_provider_health,
            commands::update_route_provider_conditions,
            // Pricing
            commands::list_model_pricing,
            commands::upsert_model_pricing,
            commands::delete_model_pricing,
            // AtomCode
            commands::detect_atomcode_config,
            commands::apply_atomcode_config,
            commands::generate_atomcode_config,
            commands::toggle_atomcode_provider,
            commands::open_atomcode_config,
            // Route Profiles
            commands::list_route_profiles,
            commands::get_route_profile,
            commands::create_route_profile,
            commands::update_route_profile,
            commands::delete_route_profile,
            commands::set_default_route_profile,
            commands::set_route_profile_mode,
            commands::set_route_active_provider,
            commands::add_provider_to_route,
            commands::remove_provider_from_route,
            commands::reorder_route_providers,
            // Runtime Status
            commands::list_provider_runtime_status,
            commands::reset_provider_runtime_status,
            commands::reset_all_provider_runtime_status,
            // Stats
            commands::get_request_stats,
            // Diagnostics
            commands::run_health_check,
            commands::run_database_check,
            commands::run_gateway_auth_check,
            commands::run_provider_check,
            commands::run_codex_config_check,
            commands::run_claude_code_config_check,
            commands::run_route_profile_check,
            commands::run_full_self_test,
            commands::export_diagnostic_bundle,
            commands::open_app_data_dir,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            // Re-show window when clicking Dock icon on macOS
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                if let Some(window) = _app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        });
}

fn is_chinese_locale() -> bool {
    std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .map(|v| v.starts_with("zh"))
        .unwrap_or(false)
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let zh = is_chinese_locale();
    let show = MenuItemBuilder::with_id("show", if zh { "显示 AgentGate" } else { "Show AgentGate" }).build(app)?;
    let start_gw = MenuItemBuilder::with_id("start_gateway", if zh { "启动网关" } else { "Start Gateway" }).build(app)?;
    let stop_gw = MenuItemBuilder::with_id("stop_gateway", if zh { "停止网关" } else { "Stop Gateway" }).build(app)?;
    let restart_gw = MenuItemBuilder::with_id("restart_gateway", if zh { "重启网关" } else { "Restart Gateway" }).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", if zh { "退出" } else { "Quit" }).build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&start_gw)
        .item(&stop_gw)
        .item(&restart_gw)
        .separator()
        .item(&quit)
        .build()?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip(if is_chinese_locale() { "AgentGate - 网关已停止" } else { "AgentGate - Gateway Stopped" })
        .menu(&menu)
        .on_menu_event(move |app, event| {
            match event.id().as_ref() {
                "show" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                "start_gateway" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::start_gateway(state).await;
                    });
                }
                "stop_gateway" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::stop_gateway(state).await;
                    });
                }
                "restart_gateway" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::restart_gateway(state).await;
                    });
                }
                "quit" => {
                    // Stop gateway before quitting
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::stop_gateway(state).await;
                        app_handle.exit(0);
                    });
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg(test)]
pub(crate) mod test_utils {
    use std::sync::Mutex;

    /// Global filesystem lock for tests that modify HOME or write to token/config files.
    /// Prevents parallel tests from clobbering each other's temp directories.
    pub static FS_LOCK: Mutex<()> = Mutex::new(());

    pub fn setup_temp_home() -> std::path::PathBuf {
        let temp = std::env::temp_dir().join(format!("agentgate_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        std::env::set_var("HOME", &temp);
        temp
    }

    pub fn cleanup(temp: &std::path::PathBuf) {
        let _ = std::fs::remove_dir_all(temp);
    }
}
