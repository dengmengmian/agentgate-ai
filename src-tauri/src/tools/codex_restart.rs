//! 重启 Codex Desktop 让新写的 config.toml / auth.json 生效。
//!
//! 2026-07 起 Codex 桌面端并入 ChatGPT 桌面应用（主进程名 "ChatGPT"，
//! 内嵌 codex 二进制），旧独立 Codex.app 仍可能存在，两个名字都要处理。
//!
//! "重启" = 杀掉桌面 App → 等一会 → 重新拉起。如果本来没在跑，就直接拉起。
//! 只针对 **桌面 App**，不会动小写 `codex` CLI 二进制（pkill -x 精确匹配
//! basename，大小写敏感）。
//!
//! 支持 macOS 和 Windows。Windows 用 `taskkill /IM Codex.exe /F` 按映像名
//! 精确匹配（注意：Windows 映像名匹配大小写不敏感，若 CLI 也以 codex.exe
//! 进程名运行会被一并杀掉——npm 版 CLI 实际跑在 node.exe 下，不受影响），
//! 再从常见安装目录拉起。Linux Codex Desktop 没有官方包，直接 supported=false。

use serde::Serialize;
// thread / Duration 只在 macOS / Windows 的 restart 路径用到;Linux 下这两条
// import 会变成 unused(CI 在 Linux 上以 -D warnings 编译会因此失败)。
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::{thread, time::Duration};

use crate::errors::AppError;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CodexRestartResult {
    /// 本平台是否实现了重启路径。false 表示前端不该显示按钮。
    pub supported: bool,
    pub platform: String,
    /// kill 前桌面 App 是不是在跑。
    pub was_running: bool,
    /// 实际杀掉的进程数（macOS 上 pkill 一发一组，记 1 即可）。
    pub killed: u32,
    /// 是否成功重新拉起。
    pub relaunched: bool,
}

pub fn restart() -> Result<CodexRestartResult, AppError> {
    #[cfg(target_os = "macos")]
    {
        Ok(restart_macos())
    }
    #[cfg(target_os = "windows")]
    {
        Ok(restart_windows())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(CodexRestartResult {
            supported: false,
            platform: std::env::consts::OS.to_string(),
            was_running: false,
            killed: 0,
            relaunched: false,
        })
    }
}

#[cfg(target_os = "macos")]
fn restart_macos() -> CodexRestartResult {
    use std::process::Command;

    // pkill -x 严格按 basename 精确匹配：大写 "Codex"/"ChatGPT" 只匹到
    // 桌面 App 主进程，不会动小写 "codex" CLI，也不会匹到 "ChatGPT Classic"。
    // 退码 0 = 至少杀掉一个；1 = 没匹到；其他 = pkill 不存在或权限不足，
    // 都按"本来就没跑"处理。
    let mut killed_apps: Vec<&str> = Vec::new();
    for name in DESKTOP_APP_NAMES {
        if let Ok(status) = Command::new("pkill").args(["-x", name]).status() {
            if status.success() {
                killed_apps.push(name);
            }
        }
    }
    let was_running = !killed_apps.is_empty();
    if was_running {
        // 给桌面 App 关窗口、写盘的时间。1000ms 是实测值。
        thread::sleep(Duration::from_millis(1000));
    }

    let open_app = |app: &str| {
        Command::new("open")
            .args(["-a", app])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    let relaunched = if was_running {
        // 杀掉的都要拉回来
        relaunch_targets(&killed_apps)
            .iter()
            .all(|app| open_app(app))
    } else {
        // 本来没跑：装了哪个就拉起哪个
        relaunch_targets(&killed_apps)
            .iter()
            .any(|app| open_app(app))
    };

    CodexRestartResult {
        supported: true,
        platform: "macos".to_string(),
        was_running,
        killed: killed_apps.len() as u32,
        relaunched,
    }
}

/// Windows 版与 macOS 同语义：杀掉桌面 App → 等 1 秒 → 重新拉起。
/// 拉起靠枚举常见安装路径找 Codex.exe；都不存在则 relaunched=false，
/// 让前端提示用户手动启动（不假装成功）。
#[cfg(target_os = "windows")]
fn restart_windows() -> CodexRestartResult {
    use std::process::Command;

    let mut was_running = false;
    let mut killed = 0u32;

    // /IM 按映像名匹配，/F 强杀。退码 0 = 至少杀掉一个；128 = 没匹到；
    // 其他 = taskkill 缺失或权限不足，都按「本来就没跑」处理（同 macOS pkill）。
    for name in DESKTOP_APP_NAMES {
        if let Ok(status) = Command::new("taskkill")
            .args(["/IM", &format!("{name}.exe"), "/F"])
            .status()
        {
            if taskkill_killed(status.code()) {
                was_running = true;
                killed += 1;
            }
        }
    }
    if was_running {
        // 给桌面 App 释放文件句柄、写盘的时间，与 macOS 路径一致。
        thread::sleep(Duration::from_millis(1000));
    }

    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let relaunched = windows_codex_exe_candidates(&local_app_data)
        .into_iter()
        .find(|p| p.exists())
        .map(|exe| Command::new(exe).spawn().is_ok())
        .unwrap_or(false);

    CodexRestartResult {
        supported: true,
        platform: "windows".to_string(),
        was_running,
        killed,
        relaunched,
    }
}

/// 桌面 App 进程/应用名，按优先级：旧 Codex 独立 App 在前，
/// 2026-07 合并后 Codex 并入 ChatGPT 桌面应用（主进程名 "ChatGPT"）。
/// 注意 pkill -x / open -a 都是精确名匹配，不会误伤 "ChatGPT Classic"。
#[cfg(any(target_os = "macos", target_os = "windows", test))]
const DESKTOP_APP_NAMES: &[&str] = &["Codex", "ChatGPT"];

/// 决定 kill 之后要重新拉起哪些 App：优先只拉起刚杀掉的；
/// 一个都没杀到（本来没在跑）就按已知 App 名依次尝试。
#[cfg(any(target_os = "macos", test))]
fn relaunch_targets<'a>(killed_apps: &[&'a str]) -> Vec<&'a str> {
    if killed_apps.is_empty() {
        DESKTOP_APP_NAMES.to_vec()
    } else {
        killed_apps.to_vec()
    }
}

/// Windows：解释 `taskkill /IM Codex.exe /F` 的退出码。
/// 0 = 至少杀掉一个；128 = 没有匹配的进程；其他（taskkill 缺失、权限不足等）
/// 一律按「本来就没跑」处理——与 macOS pkill 的容错语义对齐。
#[cfg(any(windows, test))]
fn taskkill_killed(code: Option<i32>) -> bool {
    code == Some(0)
}

/// Windows：Codex 桌面 App 可执行文件的候选安装路径（按优先级）。
/// 传入 %LOCALAPPDATA%；为空时返回空列表（让调用方按「拉起失败」处理）。
#[cfg(any(windows, test))]
fn windows_codex_exe_candidates(local_app_data: &str) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;

    let base = local_app_data.trim();
    if base.is_empty() {
        return Vec::new();
    }
    let base = PathBuf::from(base);
    let mut candidates = Vec::new();
    for name in DESKTOP_APP_NAMES {
        let exe = format!("{name}.exe");
        // NSIS 风格安装目录（Claude Desktop 等同类 App 的常见位置）
        candidates.push(base.join("Programs").join(name).join(&exe));
        // Squirrel 风格安装目录
        candidates.push(base.join(name).join(&exe));
    }
    candidates
}

// macOS / Windows 路径会真的 kill + 拉起 Codex Desktop，跑测试会误伤开发者
// 正在用的 App，所以只在 restart() 为 no-op 桩的平台上执行。
#[cfg(all(test, not(any(target_os = "macos", target_os = "windows"))))]
mod tests {
    use super::*;

    #[test]
    fn restart_is_unsupported_on_other_platforms() {
        let r = restart().unwrap();
        assert!(!r.supported);
        assert_eq!(r.killed, 0);
        assert!(!r.relaunched);
    }
}

// Windows 纯逻辑（退出码解释、候选路径），平台无关，macOS 上也可跑。
#[cfg(test)]
mod windows_logic_tests {
    use super::*;

    #[test]
    fn taskkill_zero_means_killed() {
        assert!(taskkill_killed(Some(0)));
    }

    #[test]
    fn taskkill_128_means_not_running() {
        assert!(!taskkill_killed(Some(128)));
    }

    #[test]
    fn taskkill_other_codes_treated_as_not_running() {
        assert!(!taskkill_killed(Some(1)));
        assert!(!taskkill_killed(None));
    }

    #[test]
    fn codex_exe_candidates_under_local_app_data() {
        let c = windows_codex_exe_candidates(r"C:\Users\me\AppData\Local");
        assert_eq!(c.len(), 4);
        // 旧 Codex 独立安装优先，其次合并后的 ChatGPT 桌面应用
        assert!(c[0].ends_with("Programs/Codex/Codex.exe"));
        assert!(c[1].ends_with("Codex/Codex.exe"));
        assert!(c[2].ends_with("Programs/ChatGPT/ChatGPT.exe"));
        assert!(c[3].ends_with("ChatGPT/ChatGPT.exe"));
        assert!(c[0].starts_with(r"C:\Users\me\AppData\Local"));
    }

    #[test]
    fn relaunch_targets_prefer_killed_apps() {
        // 只重新拉起刚才真的杀掉的那个 App
        assert_eq!(relaunch_targets(&["ChatGPT"]), vec!["ChatGPT"]);
        assert_eq!(relaunch_targets(&["Codex"]), vec!["Codex"]);
        assert_eq!(
            relaunch_targets(&["Codex", "ChatGPT"]),
            vec!["Codex", "ChatGPT"]
        );
    }

    #[test]
    fn relaunch_targets_fall_back_to_all_known_apps_when_none_killed() {
        // 本来没在跑：依次尝试旧 Codex.app、合并后的 ChatGPT.app
        assert_eq!(relaunch_targets(&[]), DESKTOP_APP_NAMES.to_vec());
    }

    #[test]
    fn codex_exe_candidates_empty_when_no_local_app_data() {
        assert!(windows_codex_exe_candidates("").is_empty());
        assert!(windows_codex_exe_candidates("  ").is_empty());
    }
}
