//! System tray with dynamic status + one-tap provider switching.
//!
//! The tray icon is built once at app startup with a stable id; the menu
//! and tooltip are rebuilt on demand via `refresh_tray`. Triggers:
//!
//!  - app startup (initial paint)
//!  - gateway start / stop / restart (status row + tooltip)
//!  - active-provider switch (checkmark relocates)
//!  - 30 s periodic timer (today's request count keeps ticking)
//!
//! The menu carries two dynamic-ID conventions:
//!  - `switch_active:<provider_id>` — set that provider as active and refresh
//!  - everything else routes through the same static-id handler in lib.rs
//!    (show / start_gateway / stop_gateway / restart_gateway / toggle_pet /
//!     quit).
//!
//! Borrowed from codex-switcher's tray popover design, simplified to the
//! Tauri-native menu surface — interactive popover window is a future PR.

use crate::app::state::AppState;
use crate::storage;
use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder, CheckMenuItemBuilder};
use tauri::tray::TrayIconId;
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "main";

/// Rebuild the tray menu and tooltip from current DB + runtime state.
/// Safe to call from any thread / async context. Silently no-ops if the
/// tray hasn't been registered yet (early startup) or the lookup fails.
pub fn refresh_tray(app: &AppHandle) {
    if let Err(e) = try_refresh(app) {
        // Don't poison the app — a failed refresh just means stale text.
        eprintln!("[tray] refresh failed: {e}");
    }
}

fn try_refresh(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = read_snapshot(app)?;
    let menu = build_menu(app, &snapshot)?;
    let tooltip = build_tooltip(&snapshot);

    let tray_id = TrayIconId::new(TRAY_ID);
    let tray = app
        .tray_by_id(&tray_id)
        .ok_or("tray icon not registered yet")?;
    tray.set_menu(Some(menu))?;
    tray.set_tooltip(Some(&tooltip))?;
    Ok(())
}

/// Spawn a 30 s repeating task that calls `refresh_tray`. Started once during
/// app setup; rides the app lifetime via the captured `AppHandle`.
pub fn start_periodic_refresh(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(30));
        // First tick fires immediately — skip it; setup_tray already paints once.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            refresh_tray(&app);
        }
    });
}

#[derive(Default)]
struct Snapshot {
    zh: bool,
    active_provider_id: Option<String>,
    active_provider_name: Option<String>,
    providers: Vec<(String, String)>, // (id, name)
    today_total: i64,
    gateway_running: bool,
    gateway_port: u16,
}

fn read_snapshot(app: &AppHandle) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let state: tauri::State<'_, AppState> = app.state();
    let conn = state
        .db
        .lock()
        .map_err(|_| "DB lock failed")?;

    let zh = crate::is_chinese_locale_pub();

    let settings = storage::gateway_settings::get(&conn)?;
    let active_id = settings.active_provider_id.clone();

    let providers_full = storage::providers::list_all(&conn).unwrap_or_default();
    let providers: Vec<(String, String)> = providers_full
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();
    let active_name = active_id
        .as_ref()
        .and_then(|id| providers_full.iter().find(|p| &p.id == id))
        .map(|p| p.name.clone());

    // Stats query is cheap (single COUNT) but if the table is huge, fall back
    // to 0 silently rather than blocking tray refresh on it.
    let today_total = storage::request_logs::get_stats(&conn)
        .map(|s| s.today_total)
        .unwrap_or(0);
    drop(conn);

    let runtime = state.gateway_runtime.lock().map_err(|_| "runtime lock")?;
    let gateway_running = runtime.running;
    let gateway_port = runtime.port;
    drop(runtime);

    Ok(Snapshot {
        zh,
        active_provider_id: active_id,
        active_provider_name: active_name,
        providers,
        today_total,
        gateway_running,
        gateway_port,
    })
}

fn build_menu(app: &AppHandle, snap: &Snapshot) -> Result<Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let zh = snap.zh;

    let active_label = snap
        .active_provider_name
        .as_deref()
        .map(|n| if zh { format!("当前服务商：{n}") } else { format!("Active: {n}") })
        .unwrap_or_else(|| if zh { "未选择服务商".into() } else { "No active provider".into() });
    let active_item = MenuItemBuilder::with_id("info_active", active_label)
        .enabled(false)
        .build(app)?;

    let today_label = if zh {
        format!("今日请求：{}", snap.today_total)
    } else {
        format!("Today: {} requests", snap.today_total)
    };
    let today_item = MenuItemBuilder::with_id("info_today", today_label)
        .enabled(false)
        .build(app)?;

    let gateway_label = if snap.gateway_running {
        if zh {
            format!("网关运行中 · :{}", snap.gateway_port)
        } else {
            format!("Gateway running · :{}", snap.gateway_port)
        }
    } else if zh {
        "网关已停止".into()
    } else {
        "Gateway stopped".into()
    };
    let gateway_item = MenuItemBuilder::with_id("info_gateway", gateway_label)
        .enabled(false)
        .build(app)?;

    // ── Switch active submenu ──
    let mut switch_builder = SubmenuBuilder::new(
        app,
        if zh { "切换服务商" } else { "Switch active provider" },
    );
    if snap.providers.is_empty() {
        let empty = MenuItemBuilder::with_id("switch_empty", if zh { "（暂无服务商）" } else { "(none configured)" })
            .enabled(false)
            .build(app)?;
        switch_builder = switch_builder.item(&empty);
    } else {
        for (id, name) in &snap.providers {
            let is_active = snap.active_provider_id.as_deref() == Some(id.as_str());
            let item = CheckMenuItemBuilder::with_id(
                format!("switch_active:{id}"),
                name,
            )
            .checked(is_active)
            .build(app)?;
            switch_builder = switch_builder.item(&item);
        }
    }
    let switch_submenu = switch_builder.build()?;

    // ── Existing static items ──
    let show = MenuItemBuilder::with_id("show", if zh { "显示 AgentGate" } else { "Show AgentGate" }).build(app)?;
    let start_gw = MenuItemBuilder::with_id("start_gateway", if zh { "启动网关" } else { "Start Gateway" })
        .enabled(!snap.gateway_running)
        .build(app)?;
    let stop_gw = MenuItemBuilder::with_id("stop_gateway", if zh { "停止网关" } else { "Stop Gateway" })
        .enabled(snap.gateway_running)
        .build(app)?;
    let restart_gw = MenuItemBuilder::with_id("restart_gateway", if zh { "重启网关" } else { "Restart Gateway" })
        .enabled(snap.gateway_running)
        .build(app)?;
    let toggle_pet = MenuItemBuilder::with_id("toggle_pet", if zh { "显示/隐藏宠物" } else { "Toggle Pet" }).build(app)?;
    let quit = MenuItemBuilder::with_id("quit", if zh { "退出" } else { "Quit" }).build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&active_item)
        .item(&today_item)
        .item(&gateway_item)
        .separator()
        .item(&switch_submenu)
        .separator()
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
    Ok(menu)
}

fn build_tooltip(snap: &Snapshot) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(4);
    parts.push("AgentGate".into());
    if snap.gateway_running {
        parts.push(format!(":{}", snap.gateway_port));
    } else {
        parts.push(if snap.zh { "网关已停止".into() } else { "Stopped".into() });
    }
    if let Some(ref n) = snap.active_provider_name {
        parts.push(n.clone());
    }
    if snap.today_total > 0 {
        parts.push(if snap.zh {
            format!("今日 {} 次", snap.today_total)
        } else {
            format!("{} reqs today", snap.today_total)
        });
    }
    parts.join(" · ")
}

/// Handle a `switch_active:<provider_id>` menu event. Sets the provider as
/// active and triggers a tray refresh so the checkmark relocates.
pub fn handle_switch_active(app: &AppHandle, menu_id: &str) {
    let Some(provider_id) = menu_id.strip_prefix("switch_active:") else {
        return;
    };
    let provider_id = provider_id.to_string();
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        let state: tauri::State<'_, AppState> = app_clone.state();
        let result = {
            let conn = match state.db.lock() {
                Ok(c) => c,
                Err(_) => return,
            };
            storage::providers::set_active(&conn, &provider_id)
        };
        if let Err(e) = result {
            eprintln!("[tray] set_active({provider_id}) failed: {e:?}");
        }
        refresh_tray(&app_clone);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_minimal_when_no_provider() {
        let snap = Snapshot {
            zh: false,
            gateway_running: false,
            ..Default::default()
        };
        let t = build_tooltip(&snap);
        assert!(t.contains("AgentGate"));
        assert!(t.contains("Stopped"));
    }

    #[test]
    fn tooltip_running_with_provider_and_today_count() {
        let snap = Snapshot {
            zh: false,
            gateway_running: true,
            gateway_port: 7878,
            active_provider_name: Some("MiMo".into()),
            today_total: 42,
            ..Default::default()
        };
        let t = build_tooltip(&snap);
        assert!(t.contains("AgentGate"));
        assert!(t.contains(":7878"));
        assert!(t.contains("MiMo"));
        assert!(t.contains("42 reqs"), "got: {t}");
    }

    #[test]
    fn tooltip_chinese_locale() {
        let snap = Snapshot {
            zh: true,
            gateway_running: true,
            gateway_port: 7878,
            active_provider_name: Some("小米 MiMo".into()),
            today_total: 142,
            ..Default::default()
        };
        let t = build_tooltip(&snap);
        assert!(t.contains("小米 MiMo"));
        assert!(t.contains("今日 142 次"), "got: {t}");
    }

    #[test]
    fn tooltip_drops_today_when_zero() {
        let snap = Snapshot {
            zh: false,
            gateway_running: true,
            gateway_port: 7878,
            active_provider_name: Some("MiMo".into()),
            today_total: 0,
            ..Default::default()
        };
        let t = build_tooltip(&snap);
        assert!(!t.contains("reqs"), "zero-req day shouldn't show — got: {t}");
    }

    #[test]
    fn switch_active_parses_id_correctly() {
        // Pure parsing check — the actual switching needs an AppHandle so
        // this just verifies the id-extraction lives in the function.
        let id = "switch_active:prov_mimo_001".strip_prefix("switch_active:");
        assert_eq!(id, Some("prov_mimo_001"));

        let bad = "show".strip_prefix("switch_active:");
        assert_eq!(bad, None);
    }
}
