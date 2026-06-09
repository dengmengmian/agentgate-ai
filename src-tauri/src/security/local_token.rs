use std::fs;
use std::path::PathBuf;

use rand::Rng;
use serde::Serialize;

use crate::errors::AppError;

const TOKEN_PREFIX: &str = "ag_local_";
const TOKEN_RANDOM_LEN: usize = 40;

/// 显式注入的 token(headless / 容器部署用固定 token,免去现查文件)。
/// 设了 `AGENTGATE_TOKEN` 就以它为准,不再读写 token 文件。
fn env_token() -> Option<String> {
    std::env::var("AGENTGATE_TOKEN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Get the token directory path.
pub fn token_dir() -> PathBuf {
    // headless 部署:token 落在数据目录(随 volume 持久化),不丢在容器临时 HOME。
    if let Ok(dir) = std::env::var("AGENTGATE_DB_PATH") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".agentgate")
}

/// Get the token file path.
pub fn token_path() -> PathBuf {
    token_dir().join("token")
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct GatewayAuthSettings {
    pub gateway_auth_enabled: bool,
    pub auth_mode: String,
    pub token_path: String,
    pub masked_token: String,
    pub codex_auth_type: String,
    pub claude_code_auth_type: String,
}

/// Generate a new secure random local access token.
fn generate_token() -> String {
    let charset: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    let random_part: String = (0..TOKEN_RANDOM_LEN)
        .map(|_| {
            let idx = rng.random_range(0..charset.len());
            charset[idx] as char
        })
        .collect();
    format!("{TOKEN_PREFIX}{random_part}")
}

/// Mask a token for display.
pub fn mask_token(token: &str) -> String {
    if token.len() <= 16 {
        return format!("{}••••", &token[..token.len().min(8)]);
    }
    let prefix = &token[..12];
    let suffix = &token[token.len() - 4..];
    format!("{prefix}••••••••{suffix}")
}

/// Ensure token file exists. Create if not present.
pub fn ensure_token() -> Result<String, AppError> {
    if let Some(t) = env_token() {
        return Ok(t);
    }
    let path = token_path();

    if path.exists() {
        let token = fs::read_to_string(&path).map_err(|e| {
            AppError::new(
                crate::errors::codes::LOCAL_ACCESS_TOKEN_NOT_FOUND,
                format!("Cannot read token file: {e}"),
            )
        })?;
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Generate new token
    let dir = token_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        AppError::new(
            crate::errors::codes::LOCAL_ACCESS_TOKEN_GENERATE_FAILED,
            format!("Cannot create token directory: {e}"),
        )
    })?;

    let token = generate_token();
    fs::write(&path, &token).map_err(|e| {
        AppError::new(
            crate::errors::codes::LOCAL_ACCESS_TOKEN_GENERATE_FAILED,
            format!("Cannot write token file: {e}"),
        )
    })?;

    // Set file permissions to 0600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    Ok(token)
}

/// Read the current token. Returns error if not found.
pub fn read_token() -> Result<String, AppError> {
    if let Some(t) = env_token() {
        return Ok(t);
    }
    let path = token_path();
    if !path.exists() {
        return Err(AppError::new(
            crate::errors::codes::LOCAL_ACCESS_TOKEN_NOT_FOUND,
            "Token file does not exist",
        )
        .with_suggestion("Restart AgentGate to auto-generate a token"));
    }
    let token = fs::read_to_string(&path).map_err(|e| {
        AppError::new(
            crate::errors::codes::LOCAL_ACCESS_TOKEN_NOT_FOUND,
            format!("Cannot read token: {e}"),
        )
    })?;
    Ok(token.trim().to_string())
}

/// Regenerate the token. Old token is immediately invalidated.
pub fn regenerate_token() -> Result<String, AppError> {
    let dir = token_dir();
    fs::create_dir_all(&dir).map_err(|e| {
        AppError::new(
            crate::errors::codes::LOCAL_ACCESS_TOKEN_REGENERATE_FAILED,
            format!("Cannot create directory: {e}"),
        )
    })?;

    let token = generate_token();
    let path = token_path();
    fs::write(&path, &token).map_err(|e| {
        AppError::new(
            crate::errors::codes::LOCAL_ACCESS_TOKEN_REGENERATE_FAILED,
            format!("Cannot write token: {e}"),
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    Ok(token)
}

/// Get auth settings view (masked).
pub fn get_auth_settings() -> GatewayAuthSettings {
    let path = token_path();
    let masked = match read_token() {
        Ok(t) => mask_token(&t),
        Err(_) => "Not generated".to_string(),
    };

    GatewayAuthSettings {
        gateway_auth_enabled: true,
        auth_mode: "local_token_file".to_string(),
        token_path: path.to_string_lossy().to_string(),
        masked_token: masked,
        codex_auth_type: "key_swap".to_string(),
        claude_code_auth_type: "inline_token".to_string(),
    }
}

/// Validate a token against the stored token. Uses constant-time comparison.
pub fn validate_token(provided: &str) -> bool {
    match read_token() {
        Ok(stored) => {
            // Simple constant-time comparison
            if provided.len() != stored.len() {
                return false;
            }
            let mut result = 0u8;
            for (a, b) in provided.bytes().zip(stored.bytes()) {
                result |= a ^ b;
            }
            result == 0
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{cleanup, setup_temp_home, FS_LOCK};

    #[test]
    fn test_generate_token_format() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let token = ensure_token().unwrap();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert_eq!(token.len(), TOKEN_PREFIX.len() + TOKEN_RANDOM_LEN);
        cleanup(&temp);
    }

    #[test]
    fn test_ensure_token_idempotent() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let t1 = ensure_token().unwrap();
        let t2 = ensure_token().unwrap();
        assert_eq!(t1, t2);
        cleanup(&temp);
    }

    #[test]
    fn test_regenerate_token_changes() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let t1 = ensure_token().unwrap();
        let t2 = regenerate_token().unwrap();
        assert_ne!(t1, t2);
        assert!(t2.starts_with(TOKEN_PREFIX));
        cleanup(&temp);
    }

    #[test]
    fn test_read_token_not_found() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        // Ensure no token exists
        let _ = std::fs::remove_file(token_path());
        assert!(read_token().is_err());
        cleanup(&temp);
    }

    #[test]
    fn test_validate_token_correct() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let token = ensure_token().unwrap();
        assert!(validate_token(&token));
        cleanup(&temp);
    }

    #[test]
    fn test_validate_token_wrong() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let _ = ensure_token().unwrap();
        assert!(!validate_token(
            "ag_local_wrongtoken1234567890abcdefghijklmnopqrstuvwxyz"
        ));
        cleanup(&temp);
    }

    #[test]
    fn test_validate_token_length_mismatch() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let _ = ensure_token().unwrap();
        assert!(!validate_token("short"));
        cleanup(&temp);
    }

    #[test]
    fn test_validate_token_no_file() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let _ = std::fs::remove_file(token_path());
        assert!(!validate_token(
            "ag_local_abcdefghijklmnopqrstuvwxyz1234567890"
        ));
        cleanup(&temp);
    }

    #[test]
    fn test_mask_token_long() {
        let token = "ag_local_abcdefghijklmnopqrstuvwxyz1234";
        let masked = mask_token(token);
        assert!(masked.starts_with("ag_local_abc"));
        assert!(masked.ends_with("1234"));
        assert!(masked.contains("••••••••"));
    }

    #[test]
    fn test_mask_token_short() {
        assert_eq!(mask_token("abc"), "abc••••");
        assert_eq!(mask_token("abcdefgh"), "abcdefgh••••");
    }

    #[test]
    fn test_mask_token_exactly_16() {
        let token = "ag_local_12345678";
        let masked = mask_token(token);
        assert!(masked.starts_with("ag_local_"));
        assert!(masked.contains("••••"));
    }

    #[test]
    fn test_get_auth_settings() {
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let _ = ensure_token().unwrap();
        let settings = get_auth_settings();
        assert!(settings.gateway_auth_enabled);
        assert_eq!(settings.auth_mode, "local_token_file");
        assert!(settings.masked_token.contains("••••"));
        cleanup(&temp);
    }

    #[test]
    fn test_constant_time_comparison_timing() {
        // This doesn't measure timing, but ensures the logic is correct
        let _guard = FS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let temp = setup_temp_home();
        let token = ensure_token().unwrap();
        // Same token
        assert!(validate_token(&token));
        // One char different at start
        let mut wrong = token.clone();
        let replacement = if &token[10..11] == "X" { "Y" } else { "X" };
        wrong.replace_range(10..11, replacement);
        assert!(!validate_token(&wrong));
        // One char different at end
        let mut wrong2 = token.clone();
        wrong2.replace_range(token.len() - 1..token.len(), "X");
        assert!(!validate_token(&wrong2));
        cleanup(&temp);
    }
}
