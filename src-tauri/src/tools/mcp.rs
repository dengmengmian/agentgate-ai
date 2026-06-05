//! MCP 服务器配置读取(第一步:只读展示)。
//!
//! 以**客户端文件为真相源**,不建数据库表:
//! - Codex: `~/.codex/config.toml` 的 `[mcp_servers.<name>]`(TOML)
//! - Claude Code: `~/.claude.json` 的 `mcpServers.<name>`(JSON,驼峰)
//!
//! env 只返回 key 名,不外泄 value——MCP 的 env 常含 token 等敏感值。

use std::fs;
use std::path::PathBuf;

use serde::Serialize;

/// 一条 MCP server 配置(跨客户端归一后的形态)。
#[derive(Debug, Clone, Serialize)]
pub struct McpServer {
    /// 来源客户端:"codex" / "claude_code"
    pub client: String,
    pub name: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    /// 只暴露 env 的 key,value 不返回(常含敏感 token)。
    pub env_keys: Vec<String>,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// 读 Codex `~/.codex/config.toml` 的 `[mcp_servers.*]`。文件/段不存在返回空。
pub fn read_codex_mcp() -> Vec<McpServer> {
    let path = home().join(".codex").join("config.toml");
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let doc = match content.parse::<toml_edit::DocumentMut>() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let servers = match doc.get("mcp_servers").and_then(|v| v.as_table()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    for (name, val) in servers.iter() {
        let Some(tbl) = val.as_table() else { continue };
        let command = tbl.get("command").and_then(|v| v.as_str()).map(String::from);
        let args = tbl
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let env_keys = tbl
            .get("env")
            .and_then(|v| v.as_table())
            .map(|e| e.iter().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default();
        out.push(McpServer {
            client: "codex".to_string(),
            name: name.to_string(),
            command,
            args,
            env_keys,
        });
    }
    out
}

/// 读 Claude Code `~/.claude.json` 的 `mcpServers.*`。文件/段不存在返回空。
pub fn read_claude_mcp() -> Vec<McpServer> {
    let path = home().join(".claude.json");
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let json: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let servers = match json.get("mcpServers").and_then(|v| v.as_object()) {
        Some(o) => o,
        None => return vec![],
    };
    let mut out = Vec::new();
    for (name, val) in servers.iter() {
        let command = val.get("command").and_then(|v| v.as_str()).map(String::from);
        let args = val
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let env_keys = val
            .get("env")
            .and_then(|v| v.as_object())
            .map(|e| e.keys().cloned().collect())
            .unwrap_or_default();
        out.push(McpServer {
            client: "claude_code".to_string(),
            name: name.clone(),
            command,
            args,
            env_keys,
        });
    }
    out
}

/// 汇总所有客户端的 MCP server。
pub fn list_all() -> Vec<McpServer> {
    let mut out = read_codex_mcp();
    out.extend(read_claude_mcp());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_toml_mcp_block() {
        let toml = r#"
model_provider = "OpenAI"

[mcp_servers.node_repl]
command = "/path/node_repl"
args = ["--port", "1234"]

[mcp_servers.node_repl.env]
TOKEN = "secret"
PATH_HINT = "/x"
"#;
        let doc = toml.parse::<toml_edit::DocumentMut>().unwrap();
        let servers = doc.get("mcp_servers").and_then(|v| v.as_table()).unwrap();
        let (name, val) = servers.iter().next().unwrap();
        let tbl = val.as_table().unwrap();
        assert_eq!(name, "node_repl");
        assert_eq!(tbl.get("command").and_then(|v| v.as_str()), Some("/path/node_repl"));
        let env: Vec<String> = tbl
            .get("env")
            .and_then(|v| v.as_table())
            .map(|e| e.iter().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default();
        // env 只取 key,不含 "secret" 这个 value
        assert!(env.contains(&"TOKEN".to_string()));
        assert!(!env.iter().any(|k| k == "secret"));
    }

    #[test]
    fn parses_claude_json_mcp_block() {
        let json = serde_json::json!({
            "mcpServers": {
                "pencil": { "command": "/p/mcp", "args": ["--app", "cursor"] }
            }
        });
        let servers = json.get("mcpServers").and_then(|v| v.as_object()).unwrap();
        let (name, val) = servers.iter().next().unwrap();
        assert_eq!(name, "pencil");
        assert_eq!(val.get("command").and_then(|v| v.as_str()), Some("/p/mcp"));
        let args: Vec<String> = val
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        assert_eq!(args, vec!["--app", "cursor"]);
    }
}
