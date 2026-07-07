//! Pet 窗口 ↔ 主窗口 ↔ Rust 后端之间的事件总线。
//!
//! 所有事件用 `tauri_specta::Event` 派生,在 `lib.rs::export_ts_bindings`
//! 的 `collect_events!` 里登记后,前端 `src/lib/bindings.ts` 自动生成类型化
//! listener(`events.petBubble.listen(...)`)。
//!
//! **加新事件**:在此文件加一个 struct + derive 那一坨 + 在 `collect_events!`
//! 列出,跑 `cargo test` 自动 update bindings,前端就能 listen。
//!
//! 改名规则:Rust struct PascalCase → 前端 events.camelCase / event name kebab-case。
//! 例 `PetBubble` → `events.petBubble.listen()` / Rust 内部 `name = "pet-bubble"`.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

/// 宠物窗口顶部气泡——Gateway 启停、错误、统计提示等都走这条。
/// `r#type` 字面值:`"info" | "success" | "error" | "chat"`,前端窄类型在 src/types。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetBubble {
    pub text: String,
    pub text_zh: Option<String>,
    #[serde(rename = "type")]
    pub r#type: String,
}

/// 网关运行态切换(running / stopped / active)。让前端 polling 立即刷一次。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetGatewayStateChanged(pub String);

/// 宠物 settings 改了——pet 窗口跨实例同步用。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetSettingsChanged(pub crate::models::pet::PetSettings);

/// 鼠标穿透开关变了——三个入口(右键菜单 / tray / Settings)共用这一个事件。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetClickThroughChanged(pub bool);

/// 「清空记忆」触发,Pet 前端清本地缓存 + 弹气泡。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetMemoryReset;

/// 宠物聊天记录变了——载荷是完整历史 JSON。宠物窗口和主窗口聊天页
/// 都 listen 它做实时同步(哪个窗口发消息,另一个立刻刷新)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetChatUpdated(pub String);

/// 宠物记忆变了——载荷是完整记忆 JSON。聊天里自动提取或聊天页手动编辑
/// 都会广播,宠物窗口据此更新内存中的记忆(否则会用旧名字/旧话题)。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetMemoryChanged(pub String);

/// Pet 右键菜单的「打开设置」,主窗口路由到 /settings?tab=pet。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetOpenSettings;

/// Pet 右键菜单的「打开网关页」,主窗口路由到 /gateway。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetOpenGateway;

/// Pet 右键菜单的「打开日志」,主窗口路由到 /logs?source=gateway。
#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct PetOpenLogs;
