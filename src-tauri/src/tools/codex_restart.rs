//! 重启 Codex Desktop 让新写的 config.toml / auth.json 生效。
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

    let mut was_running = false;
    let mut killed = 0u32;

    // pkill -x 严格按 basename 精确匹配，大写 "Codex" 只匹到桌面 App，
    // 不会动小写 "codex" CLI 二进制。退码 0 = 至少杀掉一个；1 = 没匹到；
    // 其他 = pkill 不存在或权限不足，都按"本来就没跑"处理。
    if let Ok(status) = Command::new("pkill").args(["-x", "Codex"]).status() {
        if status.success() {
            was_running = true;
            killed = 1;
            // 给 Codex 关窗口、写盘的时间。1000ms 是实测值。
            thread::sleep(Duration::from_millis(1000));
        }
    }

    let relaunched = Command::new("open")
        .args(["-a", "Codex"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    CodexRestartResult {
        supported: true,
        platform: "macos".to_string(),
        was_running,
        killed,
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
    if let Ok(status) = Command::new("taskkill")
        .args(["/IM", "Codex.exe", "/F"])
        .status()
    {
        if taskkill_killed(status.code()) {
            was_running = true;
            killed = 1;
            // 给 Codex 释放文件句柄、写盘的时间，与 macOS 路径一致。
            thread::sleep(Duration::from_millis(1000));
        }
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
    vec![
        // NSIS 风格安装目录（Claude Desktop 等同类 App 的常见位置）
        base.join("Programs").join("Codex").join("Codex.exe"),
        // Squirrel 风格安装目录
        base.join("Codex").join("Codex.exe"),
    ]
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
        assert_eq!(c.len(), 2);
        // NSIS 风格安装目录优先，其次 Squirrel 风格
        assert!(c[0].ends_with("Programs/Codex/Codex.exe"));
        assert!(c[1].ends_with("Codex/Codex.exe"));
        assert!(c[0].starts_with(r"C:\Users\me\AppData\Local"));
    }

    #[test]
    fn codex_exe_candidates_empty_when_no_local_app_data() {
        assert!(windows_codex_exe_candidates("").is_empty());
        assert!(windows_codex_exe_candidates("  ").is_empty());
    }
}
