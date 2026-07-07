// 以下为设计性/纯样式 clippy lint,与逻辑无关,crate 级放行:
// - too_many_arguments / type_complexity:协议转换函数参数天然多,硬拆成参数结构体属于提前抽象;
// - doc_lazy_continuation / doc_overindented_list_items:仅注释续行排版,强改会破坏既有中文对齐。
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::doc_overindented_list_items)]

#[cfg(feature = "desktop")]
mod app;
// tools / diagnostics 大部分入口是 Tauri 命令(desktop)。cli(headless)构建
// 不编译命令层,这两个模块会报大片 dead_code——只在 cli 构建静默,desktop
// 构建的告警保持有效,避免掩盖真死代码。
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
mod diagnostics;
pub mod errors;
pub mod gateway;
pub mod models;
pub mod protocol;
pub mod providers;
pub mod runtime;
pub mod security;
pub mod session_sync;
pub mod storage;
#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
mod tools;
pub mod transform;

#[cfg(feature = "desktop")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "desktop")]
use tauri::menu::{MenuBuilder, MenuItemBuilder};
#[cfg(feature = "desktop")]
use tauri::tray::TrayIconBuilder;
#[cfg(feature = "desktop")]
use tauri::webview::WebviewWindowBuilder;
#[cfg(feature = "desktop")]
use tauri::Manager;
#[cfg(feature = "desktop")]
use tauri_specta::Event;

#[cfg(feature = "desktop")]
use app::commands;
#[cfg(feature = "desktop")]
use app::events::{PetClickThroughChanged, PetMemoryReset, PetOpenGateway, PetOpenLogs};
#[cfg(feature = "desktop")]
use app::state::AppState;
#[cfg(feature = "desktop")]
use models::gateway::GatewayRuntimeState;

#[cfg(feature = "desktop")]
const PET_DEFAULT_X: f64 = 100.0;
#[cfg(feature = "desktop")]
const PET_DEFAULT_Y: f64 = 100.0;
#[cfg(feature = "desktop")]
const PET_WIDTH: f64 = 140.0;
#[cfg(feature = "desktop")]
const PET_HEIGHT: f64 = 200.0;

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
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

#[cfg(feature = "desktop")]
fn move_pet_to_visible_area(app: &tauri::AppHandle, pet_win: &tauri::WebviewWindow) {
    let Ok(position) = pet_win.outer_position() else {
        return;
    };
    let Ok(monitors) = app.available_monitors() else {
        return;
    };
    let (x, y) = normalized_pet_position(&monitors, position.x as f64, position.y as f64);
    if (x - position.x as f64).abs() > f64::EPSILON || (y - position.y as f64).abs() > f64::EPSILON
    {
        let _ = pet_win.set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
    }
}

/// 共享的 specta builder:一份 events + commands 清单同时给:
/// 1. dev 时 export TS bindings 到 src/lib/bindings.ts
/// 2. run() 启动时 mount_events,让 Event::emit 时能找到 EventRegistry
///
/// 全部 140 个 #[tauri::command] 都已加 #[specta::specta],invoke_handler 仍走
/// generate_handler!,这里只是把同一份命令清单喂给 specta 用来生成 TS 类型 +
/// 注册事件。
#[cfg(feature = "desktop")]
fn build_specta() -> tauri_specta::Builder<tauri::Wry> {
    use app::events::*;
    use tauri_specta::{collect_commands, collect_events, Builder};

    Builder::<tauri::Wry>::new()
        .events(collect_events![
            PetBubble,
            PetGatewayStateChanged,
            PetSettingsChanged,
            PetClickThroughChanged,
            PetMemoryReset,
            PetChatUpdated,
            PetMemoryChanged,
            PetOpenSettings,
            PetOpenGateway,
            PetOpenLogs,
        ])
        .commands(collect_commands![
            // Providers
            commands::list_providers,
            commands::get_provider,
            commands::get_provider_keys,
            commands::create_provider,
            commands::update_provider,
            commands::delete_provider,
            commands::set_active_provider,
            commands::fetch_provider_models,
            commands::test_provider,
            commands::provider_speedtest,
            commands::provider_speedtest_all,
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
            commands::list_log_models,
            commands::get_session_conversation,
            commands::delete_session,
            commands::count_request_logs,
            commands::get_request_log_detail,
            commands::clear_request_logs,
            commands::aggregate_request_logs_by_session,
            commands::aggregate_cost_by_model,
            commands::aggregate_cost_by_client,
            commands::aggregate_provider_detail_stats,
            commands::aggregate_route_profile_stats,
            commands::sync_claude_sessions,
            commands::sync_codex_sessions,
            commands::sync_gemini_sessions,
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
            commands::disable_codex_agentgate,
            commands::toggle_codex_provider,
            commands::open_codex_config,
            // Claude Code
            commands::detect_claude_desktop,
            commands::preview_claude_desktop_profile,
            commands::apply_claude_desktop_config,
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
            // Post-apply process detection
            commands::detect_client_running,
            commands::restart_codex_desktop,
            // Client apply history
            commands::list_client_apply_history,
            commands::clients_with_apply_history,
            commands::list_mcp_servers,
            commands::upsert_mcp_server,
            commands::delete_mcp_server,
            commands::sync_mcp_server,
            commands::export_mcp_servers,
            commands::import_mcp_servers,
            commands::rollback_client_apply,
            commands::delete_client_apply_history,
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
            commands::get_request_stats_range,
            commands::get_runtime_kpis,
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
            commands::get_pet_gateway_state_lite,
            commands::get_pet_memory,
            commands::save_pet_memory,
            commands::get_pet_chat_history,
            commands::save_pet_chat_history,
            commands::pet_chat,
            commands::pet_open_settings,
            commands::get_pet_click_through,
            commands::set_pet_click_through,
            commands::show_pet_context_menu,
            // Config Import / Export
            commands::export_config_json,
            commands::import_config_json,
            // Global instructions (CLAUDE.md / AGENTS.md)
            commands::list_instructions_templates,
            commands::read_global_instructions,
            commands::write_global_instructions,
            commands::apply_instructions_template,
            commands::export_instructions,
            commands::import_instructions,
            // Local skills (~/.claude/skills)
            commands::list_skills,
            commands::set_skill_enabled,
            commands::delete_skill,
            commands::import_skill_from_zip,
            commands::export_skills,
            commands::import_skills,
        ])
}

/// debug 模式启动时把 TS bindings 写到 src/lib/bindings.ts。
/// 生成文件含 tauri-specta boilerplate(unused imports 等)——加 @ts-nocheck
/// 让 tsc 不去 lint generated artifact。前端 import 这个文件仍能拿类型。
/// BigInt: i64/u64 字段映射成 `number`(JS 没有原生 i64,前端用 number 处理,
/// 超过 2^53 的极端情况靠 backend 保证不出现——AgentGate 的 latency/token
/// count/timestamp 都远小于 2^53)。
#[cfg(debug_assertions)]
#[cfg(feature = "desktop")]
fn export_ts_bindings(builder: &tauri_specta::Builder<tauri::Wry>) {
    use specta_typescript::Typescript;
    let exporter = Typescript::default()
        .header("// @ts-nocheck\n")
        .bigint(specta_typescript::BigIntExportBehavior::Number);
    if let Err(e) = builder.export(exporter, "../src/lib/bindings.ts") {
        eprintln!("[specta] failed to export TS bindings: {e}");
    }
}

#[cfg(all(test, feature = "desktop"))]
mod specta_export_tests {
    /// Smoke test: 全量 export 后 bindings.ts 里应该有覆盖每个域的代表性 type
    /// 和 fn。在 CI/本地 cargo test 时自动重新生成 bindings.ts,前端 tsc 就能
    /// 接到最新结果。
    #[test]
    fn full_export_covers_all_domains() {
        super::export_ts_bindings(&super::build_specta());
        let content = std::fs::read_to_string("../src/lib/bindings.ts")
            .expect("bindings.ts should be generated by export_ts_bindings()");

        // 代表性 types(每个域至少 1 个)
        for ty in [
            "PetSettings",
            "ProviderView",
            "GatewaySettings",
            "RouteProfileView",
            "RequestLogListItem",
            "CheckReport",
            "ModelPricing",
            "GatewayAuthSettings",
            "CodexConfigStatus",
            "CodexApplyConfigResult", // 旧名 ApplyConfigResult 会冲突的兜底
            "ClaudeCodeApplyConfigResult",
            "GeminiCliApplyConfigResult",
            "AtomCodeApplyConfigResult",
            "OpenCodeApplyConfigResult",
            "InstructionsStatus",
            "Skill",
            "McpServer",
            "AppError",
        ] {
            assert!(content.contains(ty), "missing type {ty}");
        }

        // 代表性 commands
        for cmd in [
            "getPetSettings",
            "listProviders",
            "startGateway",
            "listRouteProfiles",
            "listRequestLogs",
            "runHealthCheck",
            "listModelPricing",
            "detectCodexConfig",
        ] {
            assert!(content.contains(cmd), "missing command {cmd}");
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[cfg(feature = "desktop")]
pub fn run() {
    // 同一份 specta builder 用两次:dev export TS + setup 阶段 mount_events
    // (后者注册 EventRegistry,让 PetXxx{..}.emit(&app) 不 panic)。
    let specta_builder = build_specta();
    #[cfg(debug_assertions)]
    export_ts_bindings(&specta_builder);

    tauri::Builder::default()
        // 宠物原生右键菜单(show_pet_context_menu)的事件走全局 handler。
        // tray menu 的事件被 TrayIconBuilder::on_menu_event 优先接走,
        // 所以这里只处理 pet_ 前缀。
        .on_menu_event(|app, event| {
            let id = event.id().as_ref().to_string();

            // 动态: pet_switch:<pet_type>
            if let Some(pet_type) = id.strip_prefix("pet_switch:") {
                let pet_type = pet_type.to_string();
                let app_handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let state: tauri::State<'_, AppState> = app_handle.state();
                    let input = crate::models::pet::UpdatePetSettingsInput {
                        pet_type: Some(pet_type),
                        visible: None,
                        pos_x: None,
                        pos_y: None,
                    };
                    let _ = commands::update_pet_settings(input, app_handle.clone(), state);
                });
                return;
            }

            match id.as_str() {
                "pet_start_gateway" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::start_gateway(app_handle.clone(), state).await;
                    });
                }
                "pet_stop_gateway" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::stop_gateway(app_handle.clone(), state).await;
                    });
                }
                "pet_restart_gateway" => {
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let _ = commands::restart_gateway(app_handle.clone(), state).await;
                    });
                }
                "pet_toggle_click_through" => {
                    let state: tauri::State<'_, AppState> = app.state();
                    let new_value = {
                        let mut lock = match state.pet_click_through.lock() {
                            Ok(l) => l,
                            Err(_) => return,
                        };
                        *lock = !*lock;
                        *lock
                    };
                    let _ = PetClickThroughChanged(new_value).emit(app);
                }
                "pet_cc_hook" => {
                    // 读当前态取反,写 CC Notification hook。完全不碰 env。
                    let new_value = !crate::tools::claude_code::cc_hook_enabled();
                    if let Err(e) = crate::tools::claude_code::set_cc_hook(new_value) {
                        eprintln!("[cc-notify] set_cc_hook({new_value}) failed: {e:?}");
                    }
                }
                "pet_open_settings" => {
                    let _ = commands::pet_open_settings(app.clone());
                }
                "pet_open_gateway" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    let _ = PetOpenGateway.emit(app);
                }
                "pet_open_logs" => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                    let _ = PetOpenLogs.emit(app);
                }
                "pet_reset_memory" => {
                    let state: tauri::State<'_, AppState> = app.state();
                    let _ = commands::save_pet_memory("{}".into(), app.clone(), state);
                    let _ = PetMemoryReset.emit(app);
                }
                "pet_hide" => {
                    if let Some(pet_win) = app.get_webview_window("pet") {
                        let _ = pet_win.hide();
                    }
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let lock = state.db.get();
                        if let Ok(conn) = lock {
                            let _ = storage::pet_settings::update(
                                &conn,
                                crate::models::pet::UpdatePetSettingsInput {
                                    pet_type: None,
                                    visible: Some(false),
                                    pos_x: None,
                                    pos_y: None,
                                },
                            );
                        }
                    });
                }
                _ => {}
            }
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            // 注册 tauri-specta 事件 registry。Event::emit 全依赖这一步,
            // 漏掉会运行时 panic "EventRegistry not found in Tauri state"。
            specta_builder.mount_events(app);

            // CC Notification hook 的本地接收端,收到后同步桌宠并按状态发系统通知。
            crate::app::cc_notify::spawn(app.handle().clone());

            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            let pool = storage::db::init_database(&app_data_dir)
                .expect("Failed to initialize database");

            let state = AppState {
                db: pool,
                gateway_runtime: Arc::new(Mutex::new(GatewayRuntimeState::default())),
                pet_click_through: Arc::new(Mutex::new(false)),
            };

            let cleanup_db = state.db.clone();
            let session_sync_db = state.db.clone();
            let health_probe_db = state.db.clone();
            let cost_alert_db = state.db.clone();
            app.manage(state);

            // ── Ensure local access token exists ──
            let _ = security::local_token::ensure_token();

            // ── Periodic log cleanup ──
            {
                let db = cleanup_db;
                tauri::async_runtime::spawn(async move {
                    loop {
                        if let Ok(conn) = db.get() {
                            let days = storage::gateway_settings::get(&conn)
                                .map(|s| s.log_retention_days)
                                .unwrap_or(14);
                            if let Ok(n) = storage::request_logs::cleanup_older_than(&conn, days) {
                                if n > 0 {
                                    eprintln!("[log-cleanup] Cleaned up {n} old request logs (retention: {days} days)");
                                }
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    }
                });
            }

            // ── Periodic client session log sync ──
            // 启动 5 秒后同步一次，之后每小时一次。从 Claude / Codex / Gemini 本地
            // 日志增量扫出 token 用量，写入 request_logs（source='*_session'），
            // 让绕过网关的请求也能在 Dashboard 和 Logs 里看到。
            crate::session_sync::periodic::spawn(session_sync_db);

            // ── 后台主动健康探测 ──
            // 默认关（gateway_settings.health_probe_enabled）；开启后每 10 分钟对启用
            // 的 provider 发 1-token 探测，结果写 provider_runtime_status.last_probe_*，
            // 仅用于展示，不影响路由。
            crate::diagnostics::health_probe::spawn(health_probe_db);

            // ── 后台成本预警 ──
            // 默认关（gateway_settings.cost_alert_enabled）；开启后每 30 分钟检查今日花费，
            // 超过 cost_alert_threshold（USD）时发系统通知 + 桌宠气泡，当天去重。
            crate::diagnostics::cost_alert::spawn(cost_alert_db, app.handle().clone());

            // ── System Tray ──
            setup_tray(app)?;

            // ── Auto-start Gateway if enabled ──
            {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Small delay to let app fully initialize
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let state: tauri::State<'_, AppState> = app_handle.state();
                    let should_start = state.db.get()
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
                let pet_settings = state.db.get()
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
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .visible(pet_visible);

                // 全平台透明窗口。macOS 上 NSWindow 的 contentView 仍可能带浅色
                // fill(尤其系统"减少透明度"开时),显式叠一层 RGBA(0,0,0,0) 把
                // 底色彻底打掉;Windows 见下面 shadow 的说明。
                builder = builder
                    .transparent(true)
                    .background_color(tauri::window::Color(0, 0, 0, 0));

                // Windows:无边框 + 透明窗口必须显式关 DWM 阴影,否则 DWM 会给
                // 窗口垫一层不透明底,WebView2 默认白底直接露出来。
                //
                // 历史:v1.2.2 试过 transparent 失败,当时归因为 WebView2 环境
                // (runtime 版本/系统透明效果开关),退成显式深色卡片
                // (#1C1A18)。但用户实测连深色 background_color 都画成白的——
                // 说明问题在窗口合成层而非取色:builder 从未设置过 shadow,
                // 默认开启的 DWM 阴影 + transparent 的组合才是白底根因。
                // (Evergreen WebView2 自动更新,2022+ 的 runtime 都支持
                // 透明背景色,当年怀疑的 ① 在 2026 已不成立。)
                #[cfg(target_os = "windows")]
                {
                    builder = builder.shadow(false);
                }

                // Restore saved position
                builder = builder.position(pos_x, pos_y);

                if let Err(e) = builder.build() {
                    eprintln!("[pet] Failed to create pet window: {e}");
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
            commands::get_provider_keys,
            commands::create_provider,
            commands::update_provider,
            commands::delete_provider,
            commands::set_active_provider,
            commands::fetch_provider_models,
            commands::test_provider,
            commands::provider_speedtest,
            commands::provider_speedtest_all,
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
            commands::list_log_models,
            commands::get_session_conversation,
            commands::delete_session,
            commands::count_request_logs,
            commands::get_request_log_detail,
            commands::clear_request_logs,
            commands::aggregate_request_logs_by_session,
            commands::aggregate_cost_by_model,
            commands::aggregate_cost_by_client,
            commands::aggregate_provider_detail_stats,
            commands::aggregate_route_profile_stats,
            commands::sync_claude_sessions,
            commands::sync_codex_sessions,
            commands::sync_gemini_sessions,
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
            commands::disable_codex_agentgate,
            commands::toggle_codex_provider,
            commands::open_codex_config,
            // Claude Code
            commands::detect_claude_desktop,
            commands::preview_claude_desktop_profile,
            commands::apply_claude_desktop_config,
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
            // Post-apply process detection
            commands::detect_client_running,
            commands::restart_codex_desktop,
            // Client apply history
            commands::list_client_apply_history,
            commands::clients_with_apply_history,
            commands::list_mcp_servers,
            commands::upsert_mcp_server,
            commands::delete_mcp_server,
            commands::sync_mcp_server,
            commands::export_mcp_servers,
            commands::import_mcp_servers,
            commands::rollback_client_apply,
            commands::delete_client_apply_history,
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
            commands::get_request_stats_range,
            commands::get_runtime_kpis,
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
            commands::get_pet_gateway_state_lite,
            commands::get_pet_memory,
            commands::save_pet_memory,
            commands::get_pet_chat_history,
            commands::save_pet_chat_history,
            commands::pet_chat,
            commands::pet_open_settings,
            commands::get_pet_click_through,
            commands::set_pet_click_through,
            commands::show_pet_context_menu,
            // Config Import / Export
            commands::export_config_json,
            commands::import_config_json,
            // Global instructions (CLAUDE.md / AGENTS.md)
            commands::list_instructions_templates,
            commands::read_global_instructions,
            commands::write_global_instructions,
            commands::apply_instructions_template,
            commands::export_instructions,
            commands::import_instructions,
            // Local skills (~/.claude/skills)
            commands::list_skills,
            commands::set_skill_enabled,
            commands::delete_skill,
            commands::import_skill_from_zip,
            commands::export_skills,
            commands::import_skills,
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

#[cfg(feature = "desktop")]
pub fn is_chinese_locale_pub() -> bool {
    is_chinese_locale()
}

#[cfg(feature = "desktop")]
fn is_chinese_locale() -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .any(|value| locale_value_is_chinese(&value))
        || macos_system_locale_is_chinese()
}

#[cfg(feature = "desktop")]
fn locale_value_is_chinese(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.starts_with("zh") || value.contains("zh-hans") || value.contains("zh-hant")
}

#[cfg(all(feature = "desktop", target_os = "macos"))]
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

#[cfg(all(feature = "desktop", not(target_os = "macos")))]
fn macos_system_locale_is_chinese() -> bool {
    false
}

#[cfg(feature = "desktop")]
fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let zh = is_chinese_locale();
    // Build a minimal placeholder menu; `app::tray::refresh_tray` rebuilds
    // it with dynamic items (active provider, today count, switch submenu)
    // immediately after the tray is registered.
    let show = MenuItemBuilder::with_id(
        "show",
        if zh {
            "显示 AgentGate"
        } else {
            "Show AgentGate"
        },
    )
    .build(app)?;
    let quit = MenuItemBuilder::with_id("quit", if zh { "退出" } else { "Quit" }).build(app)?;
    let placeholder_menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&quit)
        .build()?;

    let _tray = TrayIconBuilder::with_id(app::tray::TRAY_ID)
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("AgentGate")
        .menu(&placeholder_menu)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref().to_string();
            // Dynamic id: switch_active:<provider_id>
            if id.starts_with("switch_active:") {
                app::tray::handle_switch_active(app, &id);
                return;
            }
            match id.as_str() {
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
                            if let Ok(conn) = db.get() {
                                let _ = storage::pet_settings::update(
                                    &conn,
                                    crate::models::pet::UpdatePetSettingsInput {
                                        pet_type: None,
                                        visible: Some(new_visible),
                                        pos_x: None,
                                        pos_y: None,
                                    },
                                );
                            }
                        });
                    }
                }
                "toggle_pet_click_through" => {
                    // 翻转 AppState 里的值并 emit changed,所有 webview(pet / settings)同步。
                    // 也顺手把宠物窗口拉出来,否则 webview 不跑没法应用 setIgnoreCursorEvents。
                    if let Some(pet_win) = app.get_webview_window("pet") {
                        if !pet_win.is_visible().unwrap_or(false) {
                            move_pet_to_visible_area(&app, &pet_win);
                            let _ = pet_win.show();
                        }
                    }
                    let state: tauri::State<'_, AppState> = app.state();
                    let new_value = {
                        let mut lock = match state.pet_click_through.lock() {
                            Ok(l) => l,
                            Err(_) => return,
                        };
                        *lock = !*lock;
                        *lock
                    };
                    let _ = PetClickThroughChanged(new_value).emit(app);
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

    // Paint the dynamic menu (active provider, today count, switch submenu)
    // immediately, then kick off the 30 s periodic refresh.
    app::tray::refresh_tray(&app.handle());
    app::tray::start_periodic_refresh(app.handle().clone());

    Ok(())
}

#[cfg(test)]
pub(crate) mod test_utils {
    use std::sync::Mutex;

    /// Global filesystem lock for tests that modify HOME or write to token/config files.
    /// Prevents parallel tests from clobbering each other's temp directories.
    pub static FS_LOCK: Mutex<()> = Mutex::new(());

    pub fn setup_temp_home() -> std::path::PathBuf {
        let temp = std::env::temp_dir().join(format!(
            "agentgate_test_{}_{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        std::env::set_var("HOME", &temp);
        temp
    }

    pub fn cleanup(temp: &std::path::PathBuf) {
        let _ = std::fs::remove_dir_all(temp);
    }

    /// 模拟 Windows 式环境跑闭包:HOME 不存在、只有 USERPROFILE。
    /// 持 FS_LOCK 与其他动 HOME 的测试互斥;RAII 恢复原 env,闭包 panic 也不泄漏。
    pub fn with_windows_style_home<F: FnOnce(&std::path::Path)>(f: F) {
        struct EnvRestore {
            home: Option<String>,
            profile: Option<String>,
        }
        impl Drop for EnvRestore {
            fn drop(&mut self) {
                match &self.home {
                    Some(h) => std::env::set_var("HOME", h),
                    None => std::env::remove_var("HOME"),
                }
                match &self.profile {
                    Some(p) => std::env::set_var("USERPROFILE", p),
                    None => std::env::remove_var("USERPROFILE"),
                }
            }
        }
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = EnvRestore {
            home: std::env::var("HOME").ok(),
            profile: std::env::var("USERPROFILE").ok(),
        };
        let fake = std::env::temp_dir().join("agentgate_fake_userprofile");
        std::env::remove_var("HOME");
        std::env::set_var("USERPROFILE", &fake);
        f(&fake);
    }
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::*;

    #[test]
    fn locale_value_is_chinese_zh_cn() {
        assert!(locale_value_is_chinese("zh_CN.UTF-8"));
    }

    #[test]
    fn locale_value_is_chinese_zh_hans() {
        assert!(locale_value_is_chinese("zh-Hans"));
    }

    #[test]
    fn locale_value_is_chinese_zh_hant() {
        assert!(locale_value_is_chinese("zh-Hant"));
    }

    #[test]
    fn locale_value_is_not_chinese_en() {
        assert!(!locale_value_is_chinese("en_US.UTF-8"));
    }

    #[test]
    fn locale_value_is_not_chinese_ja() {
        assert!(!locale_value_is_chinese("ja_JP.UTF-8"));
    }

    #[test]
    fn locale_value_case_insensitive() {
        assert!(locale_value_is_chinese("ZH_CN"));
        assert!(locale_value_is_chinese("Zh-Hans"));
    }
}
