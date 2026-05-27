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
use tauri::webview::WebviewWindowBuilder;

use app::commands;
use app::state::AppState;
use models::gateway::GatewayRuntimeState;

const PET_DEFAULT_X: f64 = 100.0;
const PET_DEFAULT_Y: f64 = 100.0;
const PET_WIDTH: f64 = 220.0;
const PET_HEIGHT: f64 = 240.0;

fn pet_position_on_screen(monitors: &[tauri::Monitor], x: f64, y: f64) -> bool {
    if !x.is_finite() || !y.is_finite() {
        return false;
    }

    monitors.iter().any(|monitor| {
        let pos = monitor.position();
        let size = monitor.size();
        let scale = monitor.scale_factor().max(1.0);
        let left = pos.x as f64 / scale;
        let top = pos.y as f64 / scale;
        let right = left + size.width as f64 / scale;
        let bottom = top + size.height as f64 / scale;

        x + PET_WIDTH > left && x < right && y + PET_HEIGHT > top && y < bottom
    })
}

fn normalized_pet_position(monitors: &[tauri::Monitor], x: f64, y: f64) -> (f64, f64) {
    if monitors.is_empty() || pet_position_on_screen(monitors, x, y) {
        return (x, y);
    }

    if let Some(monitor) = monitors.first() {
        let pos = monitor.position();
        let scale = monitor.scale_factor().max(1.0);
        (
            pos.x as f64 / scale + PET_DEFAULT_X,
            pos.y as f64 / scale + PET_DEFAULT_Y,
        )
    } else {
        (PET_DEFAULT_X, PET_DEFAULT_Y)
    }
}

fn move_pet_to_visible_area(app: &tauri::AppHandle, pet_win: &tauri::WebviewWindow) {
    let Ok(position) = pet_win.outer_position() else {
        return;
    };
    let Ok(monitors) = app.available_monitors() else {
        return;
    };
    let (x, y) = normalized_pet_position(&monitors, position.x as f64, position.y as f64);
    if (x - position.x as f64).abs() > f64::EPSILON || (y - position.y as f64).abs() > f64::EPSILON {
        let _ = pet_win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
    }
}

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
                        let _ = commands::start_gateway(app_handle.clone(), state).await;
                    }
                });
            }

            // ── Desktop Pet Window ──
            {
                let state: &AppState = app.state::<AppState>().inner();
                let pet_settings = state.db.lock()
                    .ok()
                    .and_then(|conn| storage::pet_settings::get(&conn).ok());

                let pet_visible = pet_settings.as_ref().map(|s| s.visible).unwrap_or(true);
                let saved_x = pet_settings.as_ref().map(|s| s.pos_x).unwrap_or(PET_DEFAULT_X);
                let saved_y = pet_settings.as_ref().map(|s| s.pos_y).unwrap_or(PET_DEFAULT_Y);
                let monitors = app.available_monitors().unwrap_or_default();
                let (pos_x, pos_y) = normalized_pet_position(&monitors, saved_x, saved_y);

                let mut builder = WebviewWindowBuilder::new(
                    app,
                    "pet",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .title("AgentGate Pet")
                .inner_size(PET_WIDTH, PET_HEIGHT)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .visible(pet_visible);

                // Restore saved position
                builder = builder.position(pos_x, pos_y);

                if let Err(e) = builder.build() {
                    tracing::warn!("Failed to create pet window: {e}");
                }
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
                // Hide window instead of closing (for both main and pet windows)
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
            commands::seed_model_capabilities,
            commands::autofill_provider_capabilities,
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
            // Tool Connection Test
            commands::test_tool_connection,
            // Pet
            commands::get_pet_settings,
            commands::update_pet_settings,
            commands::set_pet_visible,
            commands::get_pet_gateway_state,
            commands::get_pet_memory,
            commands::save_pet_memory,
            commands::pet_chat,
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
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .any(|value| locale_value_is_chinese(&value))
        || macos_system_locale_is_chinese()
}

fn locale_value_is_chinese(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.starts_with("zh") || value.contains("zh-hans") || value.contains("zh-hant")
}

#[cfg(target_os = "macos")]
fn macos_system_locale_is_chinese() -> bool {
    fn defaults_value(key: &str) -> Option<String> {
        let output = std::process::Command::new("defaults")
            .args(["read", "-g", key])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout).ok()
    }

    defaults_value("AppleLocale")
        .or_else(|| defaults_value("AppleLanguages"))
        .map(|value| locale_value_is_chinese(&value))
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn macos_system_locale_is_chinese() -> bool {
    false
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let zh = is_chinese_locale();
    let show = MenuItemBuilder::with_id("show", if zh { "显示 AgentGate" } else { "Show AgentGate" }).build(app)?;
    let start_gw = MenuItemBuilder::with_id("start_gateway", if zh { "启动网关" } else { "Start Gateway" }).build(app)?;
    let stop_gw = MenuItemBuilder::with_id("stop_gateway", if zh { "停止网关" } else { "Stop Gateway" }).build(app)?;
    let restart_gw = MenuItemBuilder::with_id("restart_gateway", if zh { "重启网关" } else { "Restart Gateway" }).build(app)?;
    let toggle_pet = MenuItemBuilder::with_id("toggle_pet", if zh { "显示/隐藏宠物" } else { "Toggle Pet" }).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", if zh { "退出" } else { "Quit" }).build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&start_gw)
        .item(&stop_gw)
        .item(&restart_gw)
        .separator()
        .item(&toggle_pet)
        .separator()
        .item(&quit)
        .build()?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip(if zh { "AgentGate - 网关已停止" } else { "AgentGate - Gateway Stopped" })
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
                        let _ = commands::start_gateway(app_handle.clone(), state).await;
                    });
                }
                "stop_gateway" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::stop_gateway(app_handle.clone(), state).await;
                    });
                }
                "restart_gateway" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::restart_gateway(app_handle.clone(), state).await;
                    });
                }
                "toggle_pet" => {
                    if let Some(pet_win) = app.get_webview_window("pet") {
                        let is_visible = pet_win.is_visible().unwrap_or(false);
                        if is_visible {
                            let _ = pet_win.hide();
                        } else {
                            move_pet_to_visible_area(&app, &pet_win);
                            let _ = pet_win.show();
                            let _ = pet_win.set_focus();
                        }
                        // Persist visibility
                        let new_visible = !is_visible;
                        let app_handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let state: tauri::State<'_, AppState> = app_handle.state();
                            let db = state.db.clone();
                            let conn = db.lock().unwrap();
                            let _ = storage::pet_settings::update(&conn, crate::models::pet::UpdatePetSettingsInput {
                                pet_type: None,
                                visible: Some(new_visible),
                                pos_x: None,
                                pos_y: None,
                            });
                        });
                    }
                }
                "quit" => {
                    // Stop gateway before quitting
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::stop_gateway(app_handle.clone(), state).await;
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
