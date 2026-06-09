fn main() {
    // 仅 desktop feature 下生成 Tauri context;headless(cli)构建跳过,
    // 不依赖 tauri.conf.json,也不拉 GUI 上下文。
    if std::env::var("CARGO_FEATURE_DESKTOP").is_ok() {
        tauri_build::build();
    }
}
