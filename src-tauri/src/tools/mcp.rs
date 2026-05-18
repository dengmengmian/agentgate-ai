use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::AppError;

/// A single MCP server config (common format across Claude Code / Gemini CLI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    pub enabled: bool,
}

/// MCP config source (which client file it came from).
#[derive(Debug, Clone, Serialize)]
pub struct McpSource {
    pub client: String,
    pub config_path: String,
    pub servers: Vec<McpServer>,
}

/// Unified MCP view across all clients.
#[derive(Debug, Clone, Serialize)]
pub struct McpOverview {
    pub sources: Vec<McpSource>,
    pub total_servers: usize,
    pub total_clients: usize,
}

/// Known MCP config file locations.
fn mcp_config_paths() -> Vec<(&'static str, PathBuf, &'static str)> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let home = PathBuf::from(home);

    vec![
        ("Claude Code", home.join(".claude.json"), "mcpServers"),
        ("Claude Code (settings)", home.join(".claude").join("settings.json"), "mcpServers"),
        ("Gemini CLI", home.join(".gemini").join("settings.json"), "mcpServers"),
    ]
}

/// Read MCP servers from a JSON file's specified field.
fn read_mcp_from_file(path: &PathBuf, field: &str) -> Result<Vec<McpServer>, AppError> {
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(path).map_err(|e| {
        AppError::new("MCP_READ_FAILED", format!("Cannot read {}: {e}", path.display()))
    })?;

    let doc: Value = serde_json::from_str(&content).map_err(|e| {
        AppError::new("MCP_PARSE_FAILED", format!("Cannot parse {}: {e}", path.display()))
    })?;

    let servers_obj = match doc.get(field) {
        Some(Value::Object(map)) => map.clone(),
        _ => return Ok(vec![]),
    };

    let mut servers = Vec::new();
    for (name, config) in servers_obj {
        let command = config.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let args: Vec<String> = config.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|a| a.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let timeout = config.get("timeout").and_then(|v| v.as_i64());
        let env = config.get("env")
            .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok());
        let enabled = config.get("disabled").and_then(|v| v.as_bool()).map(|d| !d).unwrap_or(true);

        if !command.is_empty() {
            servers.push(McpServer { name, command, args, timeout, env, enabled });
        }
    }

    Ok(servers)
}

/// Get unified MCP overview from all known clients.
pub fn get_overview() -> McpOverview {
    let mut sources = Vec::new();
    let mut total_servers = 0;

    for (client, path, field) in mcp_config_paths() {
        if let Ok(servers) = read_mcp_from_file(&path, field) {
            if !servers.is_empty() {
                total_servers += servers.len();
                sources.push(McpSource {
                    client: client.to_string(),
                    config_path: path.to_string_lossy().to_string(),
                    servers,
                });
            }
        }
    }

    let total_clients = sources.len();
    McpOverview { sources, total_servers, total_clients }
}

/// Add an MCP server to a specific client's config file.
pub fn add_server(client: &str, name: &str, command: &str, args: &[String], timeout: Option<i64>) -> Result<(), AppError> {
    let (path, field) = find_client_config(client)?;

    let content = if path.exists() {
        fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string())
    } else {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        "{}".to_string()
    };

    let mut doc: Value = serde_json::from_str(&content).map_err(|e| {
        AppError::new("MCP_PARSE_FAILED", format!("Cannot parse: {e}"))
    })?;

    if doc.get(field).is_none() {
        doc[field] = serde_json::json!({});
    }

    let mut server = serde_json::json!({
        "command": command,
        "args": args,
    });
    if let Some(t) = timeout {
        server["timeout"] = serde_json::json!(t);
    }

    doc[field][name] = server;

    let new_content = serde_json::to_string_pretty(&doc).map_err(|e| {
        AppError::new("MCP_WRITE_FAILED", format!("Cannot serialize: {e}"))
    })?;

    fs::write(&path, format!("{new_content}\n")).map_err(|e| {
        AppError::new("MCP_WRITE_FAILED", format!("Cannot write: {e}"))
    })?;

    Ok(())
}

/// Remove an MCP server from a specific client's config file.
pub fn remove_server(client: &str, name: &str) -> Result<(), AppError> {
    let (path, field) = find_client_config(client)?;

    if !path.exists() {
        return Err(AppError::new("MCP_CONFIG_NOT_FOUND", "Config file not found"));
    }

    let content = fs::read_to_string(&path).map_err(|e| {
        AppError::new("MCP_READ_FAILED", format!("Cannot read: {e}"))
    })?;

    let mut doc: Value = serde_json::from_str(&content).map_err(|e| {
        AppError::new("MCP_PARSE_FAILED", format!("Cannot parse: {e}"))
    })?;

    if let Some(servers) = doc.get_mut(field).and_then(|v| v.as_object_mut()) {
        servers.remove(name);
    }

    let new_content = serde_json::to_string_pretty(&doc).map_err(|e| {
        AppError::new("MCP_WRITE_FAILED", format!("Cannot serialize: {e}"))
    })?;

    fs::write(&path, format!("{new_content}\n")).map_err(|e| {
        AppError::new("MCP_WRITE_FAILED", format!("Cannot write: {e}"))
    })?;

    Ok(())
}

/// Toggle enable/disable of an MCP server.
pub fn toggle_server(client: &str, name: &str, enabled: bool) -> Result<(), AppError> {
    let (path, field) = find_client_config(client)?;

    if !path.exists() {
        return Err(AppError::new("MCP_CONFIG_NOT_FOUND", "Config file not found"));
    }

    let content = fs::read_to_string(&path).map_err(|e| {
        AppError::new("MCP_READ_FAILED", format!("Cannot read: {e}"))
    })?;

    let mut doc: Value = serde_json::from_str(&content).map_err(|e| {
        AppError::new("MCP_PARSE_FAILED", format!("Cannot parse: {e}"))
    })?;

    if let Some(server) = doc.get_mut(field).and_then(|v| v.get_mut(name)) {
        if enabled {
            if let Some(obj) = server.as_object_mut() {
                obj.remove("disabled");
            }
        } else {
            server["disabled"] = serde_json::json!(true);
        }
    }

    let new_content = serde_json::to_string_pretty(&doc).map_err(|e| {
        AppError::new("MCP_WRITE_FAILED", format!("Cannot serialize: {e}"))
    })?;

    fs::write(&path, format!("{new_content}\n")).map_err(|e| {
        AppError::new("MCP_WRITE_FAILED", format!("Cannot write: {e}"))
    })?;

    Ok(())
}

fn find_client_config(client: &str) -> Result<(PathBuf, &'static str), AppError> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let home = PathBuf::from(home);

    match client {
        "Claude Code" => Ok((home.join(".claude.json"), "mcpServers")),
        "Claude Code (settings)" => Ok((home.join(".claude").join("settings.json"), "mcpServers")),
        "Gemini CLI" => Ok((home.join(".gemini").join("settings.json"), "mcpServers")),
        _ => Err(AppError::new("MCP_UNKNOWN_CLIENT", format!("Unknown client: {client}"))),
    }
}
