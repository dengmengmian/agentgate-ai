//! 客户端配置写后验证。
//!
//! 各客户端 apply 把配置写进 `~/.codex/config.toml`、`~/.claude/settings.json`
//! 等文件后,调用此处把落盘内容读回、逐字节比对。防御"写入声称成功但磁盘内容
//! 与预期不符"(文件系统异常、并发改写、编码问题),避免对外报假成功——配置一旦
//! 写歪,用户客户端会直接连不上网关却看到"应用成功"。
//!
//! 返回 `Result<(), String>`,由各 apply 用自己的 `*_CONFIG_WRITE_FAILED` 错误码包装。

use std::path::Path;

/// 读回 `path` 的内容,与 `expected` 逐字节比对。不一致或读取失败返回 `Err`。
pub fn verify_written(path: &Path, expected: &[u8]) -> Result<(), String> {
    let actual = std::fs::read(path)
        .map_err(|e| format!("写入校验失败:无法读回 {}: {e}", path.display()))?;
    if actual != expected {
        return Err(format!(
            "写入校验失败:{} 落盘内容与预期不一致(预期 {} 字节,实际 {} 字节)",
            path.display(),
            expected.len(),
            actual.len()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_file(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("agentgate_verify_test_{name}"));
        p
    }

    #[test]
    fn matching_content_passes() {
        let path = tmp_file("ok");
        std::fs::write(&path, b"hello world").unwrap();
        assert!(verify_written(&path, b"hello world").is_ok());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn mismatched_content_fails() {
        let path = tmp_file("mismatch");
        std::fs::write(&path, b"actually written").unwrap();
        let err = verify_written(&path, b"what we expected").unwrap_err();
        assert!(err.contains("落盘内容与预期不一致"), "got: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_fails() {
        let path = tmp_file("missing_should_not_exist");
        let _ = std::fs::remove_file(&path);
        let err = verify_written(&path, b"anything").unwrap_err();
        assert!(err.contains("无法读回"), "got: {err}");
    }
}
