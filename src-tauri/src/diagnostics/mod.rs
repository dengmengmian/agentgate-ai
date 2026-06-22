pub mod checks;
// cost_alert 依赖 tauri(系统通知 + 桌宠气泡),仅桌面构建编译;cli headless 无此功能。
#[cfg(feature = "desktop")]
pub mod cost_alert;
pub mod health_probe;
pub mod report;
pub mod speedtest;
pub mod test_failure;
