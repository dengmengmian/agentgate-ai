//! 重启 Codex Desktop 让新写的 config.toml / auth.json 生效。
//!
//! "重启" = 杀掉桌面 App → 等一会 → 重新拉起。如果本来没在跑，就直接拉起。
//! 只针对 **桌面 App**，不会动小写 `codex` CLI 二进制（pkill -x 精确匹配
//! basename，大小写敏感）。
//!
//! 当前只支持 macOS。Windows 留 TODO；Linux Codex Desktop 没有官方包，
//! 直接 supported=false。

use serde::Serialize;
use std::thread;
use std::time::Duration;

use crate::errors::AppError;

#[derive(Debug, Clone, Serialize)]
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
    #[cfg(not(target_os = "macos"))]
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
            // 给 Codex 关窗口、写盘的时间。1000ms 是 mimo2codex 实测值。
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

#[cfg(test)]
mod tests {
    use super::*;

    // macOS path actually invokes `pkill Codex` + `open -a Codex` and would
    // sandbag a developer running Codex Desktop while tests run, so we only
    // exercise it on platforms where restart() is a no-op stub.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn restart_is_unsupported_on_non_macos() {
        let r = restart().unwrap();
        assert!(!r.supported);
        assert_eq!(r.killed, 0);
        assert!(!r.relaunched);
    }
}
