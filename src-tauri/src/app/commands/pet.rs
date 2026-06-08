use tauri::{Manager, State};
use tauri_specta::Event;

use crate::app::events::{PetClickThroughChanged, PetOpenSettings, PetSettingsChanged};
use crate::app::state::AppState;
use crate::errors::AppError;
use crate::storage;

// ── Pet Commands ──────────────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn get_pet_settings(
    state: State<'_, AppState>,
) -> Result<crate::models::pet::PetSettings, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::pet_settings::get(&conn)
}

#[tauri::command]
#[specta::specta]
pub fn update_pet_settings(
    input: crate::models::pet::UpdatePetSettingsInput,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::models::pet::PetSettings, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let result = storage::pet_settings::update(&conn, input)?;
    let _ = PetSettingsChanged(result.clone()).emit(&app_handle);
    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub fn set_pet_visible(
    visible: bool,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::models::pet::PetSettings, AppError> {
    if let Some(pet_win) = app_handle.get_webview_window("pet") {
        if visible {
            crate::move_pet_to_visible_area(&app_handle, &pet_win);
            let _ = pet_win.show();
            let _ = pet_win.set_focus();
        } else {
            let _ = pet_win.hide();
        }
    }
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::pet_settings::update(
        &conn,
        crate::models::pet::UpdatePetSettingsInput {
            pet_type: None,
            visible: Some(visible),
            pos_x: None,
            pos_y: None,
        },
    )
}

/// 轻量版:只返回 state + last_error,**不**做全表 stats 聚合。
/// 给 10s 轮询用,频次高所以必须便宜。
/// last_error 走 idx_request_logs_timestamp 索引,O(log n) 几乎免费。
/// stats 数据用单独的 `get_pet_gateway_state`(原命令)在 30 分钟 stats bubble 触发前调一次。
#[tauri::command]
#[specta::specta]
pub fn get_pet_gateway_state_lite(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, AppError> {
    let (running, active) = {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        let active = runtime
            .active_requests
            .as_ref()
            .map(|c| c.load(std::sync::atomic::Ordering::Relaxed) > 0)
            .unwrap_or(false);
        (runtime.running, active)
    };

    let gw_state = if !running {
        "stopped"
    } else if active {
        "active"
    } else {
        "running"
    };

    let last_error = if running {
        let conn = state
            .db
            .get()
            .map_err(|_| AppError::internal("DB lock failed"))?;
        conn.query_row(
            "SELECT error_message, provider, timestamp FROM request_logs
             WHERE error_message IS NOT NULL AND error_message != ''
             ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| {
                let msg: String = row.get(0)?;
                let provider: Option<String> = row.get(1)?;
                let ts: String = row.get(2)?;
                Ok(serde_json::json!({ "message": msg, "provider": provider, "timestamp": ts }))
            },
        )
        .ok()
    } else {
        None
    };

    Ok(serde_json::json!({
        "state": gw_state,
        "last_error": last_error,
    }))
}

#[tauri::command]
#[specta::specta]
pub fn get_pet_gateway_state(state: State<'_, AppState>) -> Result<serde_json::Value, AppError> {
    let (running, active, runtime_host, runtime_port) = {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        let active = runtime
            .active_requests
            .as_ref()
            .map(|c| c.load(std::sync::atomic::Ordering::Relaxed) > 0)
            .unwrap_or(false);
        (
            runtime.running,
            active,
            runtime.host.clone(),
            runtime.port as i64,
        )
    };

    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    let settings = storage::gateway_settings::get(&conn)?;
    let active_provider = settings
        .active_provider_id
        .as_ref()
        .and_then(|pid| storage::providers::get_by_id(&conn, pid).ok())
        .map(
            |p| serde_json::json!({ "id": p.id, "name": p.name, "default_model": p.default_model }),
        );

    let gw_state = if !running {
        "stopped"
    } else if active {
        "active"
    } else {
        "running"
    };

    let last_error = if running {
        conn.query_row(
            "SELECT error_message, provider, timestamp FROM request_logs
             WHERE error_message IS NOT NULL AND error_message != ''
             ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| {
                let msg: String = row.get(0)?;
                let provider: Option<String> = row.get(1)?;
                let ts: String = row.get(2)?;
                Ok(serde_json::json!({ "message": msg, "provider": provider, "timestamp": ts }))
            },
        )
        .ok()
    } else {
        None
    };

    let stats = storage::request_logs::get_stats(&conn).ok();
    let today_stats = stats
        .as_ref()
        .map(|s| {
            serde_json::json!({
                "requests": s.today_total,
                "errors": s.today_errors,
                "input_tokens": s.today_input_tokens,
                "output_tokens": s.today_output_tokens,
                "cache_read_tokens": s.today_cache_read_tokens,
                "cache_write_tokens": s.today_cache_write_tokens,
                "cost": s.today_cost,
            })
        })
        .unwrap_or_else(|| {
            serde_json::json!({
                "requests": 0,
                "errors": 0,
                "input_tokens": 0,
                "output_tokens": 0,
                "cache_read_tokens": 0,
                "cache_write_tokens": 0,
                "cost": 0.0,
            })
        });

    let latest_model = conn
        .query_row(
            "SELECT model FROM request_logs
             WHERE source = 'gateway' AND model IS NOT NULL AND model != ''
             ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok();

    Ok(serde_json::json!({
        "state": gw_state,
        "running": running,
        "host": if running { runtime_host } else { settings.host },
        "port": if running { runtime_port } else { settings.port },
        "active_provider": active_provider,
        "latest_model": latest_model,
        "last_error": last_error,
        "today": today_stats,
    }))
}

// ── Pet Chat Commands ─────────────────────────────────────────

#[tauri::command]
#[specta::specta]
pub fn get_pet_memory(state: State<'_, AppState>) -> Result<String, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    Ok(storage::app_settings::get(&conn, "pet_memory")?.unwrap_or_else(|| "{}".to_string()))
}

/// 原生右键菜单(替代 HTML 实现)——HTML 菜单画在宠物窗口里,菜单展开
/// 期间窗口区域全部接事件,挡底层应用。换成 OS 弹出菜单完全脱离 webview,
/// 不挡也不需要 resize 窗口。
///
/// 9 个角色用子菜单 + checked 标记当前选中。鼠标穿透用 CheckMenuItem。
/// 菜单事件统一在 lib.rs 的 on_menu_event 里处理(pet_ 前缀)。
#[tauri::command]
#[specta::specta]
pub fn show_pet_context_menu(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    use tauri::menu::{
        CheckMenuItemBuilder, ContextMenu, MenuBuilder, MenuItemBuilder, SubmenuBuilder,
    };

    let pet_win = app_handle
        .get_webview_window("pet")
        .ok_or_else(|| AppError::internal("pet window not found"))?;

    let zh = crate::is_chinese_locale_pub();
    let click_through = *state
        .pet_click_through
        .lock()
        .map_err(|_| AppError::internal("ct lock"))?;

    let (current_pet_type, active_provider_name, today_total) = {
        let conn = state.db.get().map_err(|_| AppError::internal("db lock"))?;
        let current_pet_type = storage::pet_settings::get(&conn)
            .map(|s| s.pet_type)
            .unwrap_or_else(|_| "robot".into());
        let settings = storage::gateway_settings::get(&conn)?;
        let active_provider_name = settings
            .active_provider_id
            .as_ref()
            .and_then(|pid| storage::providers::get_by_id(&conn, pid).ok())
            .map(|p| p.name);
        let today_total = storage::request_logs::get_stats(&conn)
            .map(|s| s.today_total)
            .unwrap_or(0);
        (current_pet_type, active_provider_name, today_total)
    };
    let (gateway_running, gateway_port) = {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("runtime lock"))?;
        (runtime.running, runtime.port)
    };

    let pet_types: &[(&str, &str, &str)] = &[
        ("robot", "网关机器人", "Gateway Bot"),
        ("pixel-cat", "像素猫", "Pixel Cat"),
        ("slime", "史莱姆", "Slime"),
        ("fox", "CEO", "CEO"),
        ("octopus", "章鱼", "Octopus"),
        ("ghost", "麻凡", "MaFan"),
        ("ox", "奎奎", "KuiKui"),
        ("soldier", "分总", "FenZong"),
        ("coder", "振振", "ZhenZhen"),
    ];

    let mut switch_builder =
        SubmenuBuilder::new(&app_handle, if zh { "切换角色" } else { "Switch Pet" });
    for (id, zh_n, en_n) in pet_types {
        let label = if zh { *zh_n } else { *en_n };
        let item = CheckMenuItemBuilder::with_id(format!("pet_switch:{id}"), label)
            .checked(current_pet_type == *id)
            .build(&app_handle)
            .map_err(|e| AppError::internal(format!("menu: {e}")))?;
        switch_builder = switch_builder.item(&item);
    }
    let switch_submenu = switch_builder
        .build()
        .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let click_through_item = CheckMenuItemBuilder::with_id(
        "pet_toggle_click_through",
        if zh { "鼠标穿透" } else { "Click-through" },
    )
    .checked(click_through)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let gateway_status_label = if gateway_running {
        if zh {
            format!("网关运行中 · :{gateway_port}")
        } else {
            format!("Gateway running · :{gateway_port}")
        }
    } else if zh {
        "网关已停止".to_string()
    } else {
        "Gateway stopped".to_string()
    };
    let gateway_status_item = MenuItemBuilder::with_id("pet_info_gateway", gateway_status_label)
        .enabled(false)
        .build(&app_handle)
        .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let provider_label = active_provider_name
        .map(|name| {
            if zh {
                format!("当前供应商：{name}")
            } else {
                format!("Active: {name}")
            }
        })
        .unwrap_or_else(|| {
            if zh {
                "未选择供应商".to_string()
            } else {
                "No active provider".to_string()
            }
        });
    let provider_item = MenuItemBuilder::with_id("pet_info_provider", provider_label)
        .enabled(false)
        .build(&app_handle)
        .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let today_item = MenuItemBuilder::with_id(
        "pet_info_today",
        if zh {
            format!("今日请求：{today_total}")
        } else {
            format!("Today: {today_total} requests")
        },
    )
    .enabled(false)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let start_gateway_item = MenuItemBuilder::with_id(
        "pet_start_gateway",
        if zh { "启动网关" } else { "Start Gateway" },
    )
    .enabled(!gateway_running)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let stop_gateway_item = MenuItemBuilder::with_id(
        "pet_stop_gateway",
        if zh { "停止网关" } else { "Stop Gateway" },
    )
    .enabled(gateway_running)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let restart_gateway_item = MenuItemBuilder::with_id(
        "pet_restart_gateway",
        if zh {
            "重启网关"
        } else {
            "Restart Gateway"
        },
    )
    .enabled(gateway_running)
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let open_gateway_item = MenuItemBuilder::with_id(
        "pet_open_gateway",
        if zh {
            "打开网关页"
        } else {
            "Open Gateway"
        },
    )
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let open_logs_item =
        MenuItemBuilder::with_id("pet_open_logs", if zh { "打开日志" } else { "Open Logs" })
            .build(&app_handle)
            .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let open_settings_item = MenuItemBuilder::with_id(
        "pet_open_settings",
        if zh { "打开设置" } else { "Open Settings" },
    )
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let reset_memory_item = MenuItemBuilder::with_id(
        "pet_reset_memory",
        if zh { "清空记忆" } else { "Reset Memory" },
    )
    .build(&app_handle)
    .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let hide_pet_item =
        MenuItemBuilder::with_id("pet_hide", if zh { "隐藏宠物" } else { "Hide Pet" })
            .build(&app_handle)
            .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    let menu = MenuBuilder::new(&app_handle)
        .item(&gateway_status_item)
        .item(&provider_item)
        .item(&today_item)
        .separator()
        .item(&start_gateway_item)
        .item(&stop_gateway_item)
        .item(&restart_gateway_item)
        .separator()
        .item(&open_gateway_item)
        .item(&open_logs_item)
        .separator()
        .item(&switch_submenu)
        .separator()
        .item(&click_through_item)
        .item(&open_settings_item)
        .item(&reset_memory_item)
        .separator()
        .item(&hide_pet_item)
        .build()
        .map_err(|e| AppError::internal(format!("menu: {e}")))?;

    // popup 要的是 Window 不是 WebviewWindow——从 WebviewWindow 拿底层 window 句柄。
    menu.popup(pet_win.as_ref().window().clone())
        .map_err(|e| AppError::internal(format!("popup: {e}")))?;

    Ok(())
}

/// 宠物窗口的鼠标穿透状态。三个入口(右键菜单 / tray / Settings)都改这里,
/// emit `pet-click-through-changed` 让所有 webview 同步。
#[tauri::command]
#[specta::specta]
pub fn get_pet_click_through(state: State<'_, AppState>) -> Result<bool, AppError> {
    Ok(*state
        .pet_click_through
        .lock()
        .map_err(|_| AppError::internal("lock failed"))?)
}

#[tauri::command]
#[specta::specta]
pub fn set_pet_click_through(
    value: bool,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    *state
        .pet_click_through
        .lock()
        .map_err(|_| AppError::internal("lock failed"))? = value;
    let _ = PetClickThroughChanged(value).emit(&app_handle);
    Ok(value)
}

/// 从宠物右键菜单触发:把主窗口拉起来 + 通知前端导航到「宠物」设置页。
/// 主窗口可能被最小化/隐藏,所以先 unminimize 再 show + set_focus。
#[tauri::command]
#[specta::specta]
pub fn pet_open_settings(app_handle: tauri::AppHandle) -> Result<bool, AppError> {
    if let Some(w) = app_handle.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = PetOpenSettings.emit(&app_handle);
    Ok(true)
}

#[tauri::command]
#[specta::specta]
pub fn save_pet_memory(memory: String, state: State<'_, AppState>) -> Result<bool, AppError> {
    let conn = state
        .db
        .get()
        .map_err(|_| AppError::internal("DB lock failed"))?;
    storage::app_settings::set(&conn, "pet_memory", &memory)?;
    Ok(true)
}

#[tauri::command]
#[specta::specta]
pub async fn pet_chat(
    messages: Vec<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let (host, port) = {
        let runtime = state
            .gateway_runtime
            .lock()
            .map_err(|_| AppError::internal("Runtime lock failed"))?;
        if !runtime.running {
            return Err(AppError::new(
                "GATEWAY_NOT_RUNNING",
                "Gateway is not running",
            )
            .with_suggestion("Start the gateway from the pet menu or Gateway page"));
        }
        (runtime.host.clone(), runtime.port)
    };

    let token = crate::security::local_token::ensure_token()?;
    let host = gateway_client_host(&host);
    let url = format!("http://{}:{}/v1/chat/completions", format_host_for_url(&host), port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::internal(format!("HTTP client error: {e}")))?;

    let body = serde_json::json!({
        "model": "agentgate",
        "messages": messages,
        "max_tokens": 200,
        "temperature": 0.8,
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("User-Agent", "AgentGate-Pet/1.0")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(pet_gateway_error(status.as_u16(), &text));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::internal(format!("Parse error: {e}")))?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("...")
        .to_string();

    Ok(content)
}

fn gateway_client_host(host: &str) -> String {
    match host.trim() {
        "" | "0.0.0.0" | "::" => "127.0.0.1".to_string(),
        other => other.to_string(),
    }
}

fn format_host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') && !host.ends_with(']') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn pet_gateway_error(status: u16, body: &str) -> AppError {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let err = parsed.as_ref().and_then(|v| v.get("error"));
    let code = err
        .and_then(|e| e.get("code"))
        .and_then(|v| v.as_str())
        .unwrap_or("PET_GATEWAY_CHAT_ERROR");
    let message = err
        .and_then(|e| e.get("message"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Gateway chat failed with HTTP {status}"));
    let detail = err
        .and_then(|e| e.get("detail"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let trimmed = body.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.chars().take(1000).collect())
            }
        });
    let suggestion = err
        .and_then(|e| e.get("suggestion"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty());

    let mut app_error = AppError::new(code, message);
    if let Some(detail) = detail {
        app_error = app_error.with_detail(detail);
    }
    if let Some(suggestion) = suggestion {
        app_error = app_error.with_suggestion(suggestion);
    }
    app_error
}

#[cfg(test)]
mod pet_chat_tests {
    use super::*;

    #[test]
    fn gateway_client_host_uses_loopback_for_wildcard_bind() {
        assert_eq!(gateway_client_host("0.0.0.0"), "127.0.0.1");
        assert_eq!(gateway_client_host("::"), "127.0.0.1");
        assert_eq!(gateway_client_host("127.0.0.1"), "127.0.0.1");
    }

    #[test]
    fn format_host_for_url_wraps_ipv6() {
        assert_eq!(format_host_for_url("127.0.0.1"), "127.0.0.1");
        assert_eq!(format_host_for_url("::1"), "[::1]");
        assert_eq!(format_host_for_url("[::1]"), "[::1]");
    }

    #[test]
    fn pet_gateway_error_extracts_openai_error_shape() {
        let err = pet_gateway_error(
            503,
            r#"{"error":{"message":"No active provider configured","code":"ACTIVE_PROVIDER_NOT_FOUND","detail":"none","suggestion":"pick one"}}"#,
        );
        assert_eq!(err.code, "ACTIVE_PROVIDER_NOT_FOUND");
        assert_eq!(err.message, "No active provider configured");
        assert_eq!(err.detail, Some("none".to_string()));
        assert_eq!(err.suggestion, Some("pick one".to_string()));
    }

    #[test]
    fn pet_gateway_error_keeps_plain_body_as_detail() {
        let err = pet_gateway_error(500, "plain failure");
        assert_eq!(err.code, "PET_GATEWAY_CHAT_ERROR");
        assert_eq!(err.message, "Gateway chat failed with HTTP 500");
        assert_eq!(err.detail, Some("plain failure".to_string()));
    }
}

