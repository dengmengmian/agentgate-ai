//! GitHub Copilot 上游接入(v1)。
//!
//! 用户在 Provider 的 api_key 字段填 GitHub OAuth token(gho_/ghu_ 开头),
//! 网关在调 api.githubcopilot.com 前先换取短期 Copilot bearer token:
//! GET {github_api_base}/copilot_internal/v2/token,进程内缓存、到期前提前刷新。
//!
//! 计费优化:Copilot 用 `x-initiator` 区分「用户发起」(user,计 premium 额度)
//! 和「agent 续写」(agent,不额外计费)。`classify_initiator` 对 Anthropic
//! Messages 请求体做分类,语义对齐 cc-switch copilot_optimizer::classify_request。
//!
//! 模型归一化:Copilot 上游只接受 dot 形式的 Claude 4.x 模型 ID
//! (claude-sonnet-4.6),Claude Code 发的是 dash 形式(claude-sonnet-4-6),
//! 不归一化会 400 model_not_supported——`normalize_model` 负责改写。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use serde_json::Value;

use crate::errors::AppError;

/// Copilot API Header 常量(取自 cc-switch,模拟 VS Code Copilot Chat 客户端)。
pub const COPILOT_EDITOR_VERSION: &str = "vscode/1.110.1";
pub const COPILOT_PLUGIN_VERSION: &str = "copilot-chat/0.38.2";
pub const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.38.2";
pub const COPILOT_API_VERSION: &str = "2025-10-01";
pub const COPILOT_INTEGRATION_ID: &str = "vscode-chat";

/// token 到期前的提前刷新缓冲(秒),与 cc-switch 一致。
const TOKEN_REFRESH_BUFFER_SECONDS: i64 = 60;

/// 默认 GitHub API 基址。测试 / GHES 场景可用环境变量
/// `AGENTGATE_COPILOT_GITHUB_API_BASE` 覆盖(见 `get_copilot_token`)。
const DEFAULT_GITHUB_API_BASE: &str = "https://api.github.com";

/// Claude Code 上下文压缩请求的 system prompt 专用前缀
/// (用户无法手动设置 system,是最可靠的机器特征)。
const COMPACT_SYSTEM_PREFIX: &str =
    "You are a helpful AI assistant tasked with summarizing conversations";

/// 判断 provider_type 是否为 Copilot。
pub fn is_copilot(provider_type: &str) -> bool {
    provider_type.trim().eq_ignore_ascii_case("copilot")
}

// ─── 请求头 ─────────────────────────────────────────────

/// 调 Copilot API 必带的 headers(不含 Authorization,由调用方注入 bearer)。
pub fn copilot_request_headers() -> [(&'static str, &'static str); 5] {
    [
        ("copilot-integration-id", COPILOT_INTEGRATION_ID),
        ("editor-version", COPILOT_EDITOR_VERSION),
        ("editor-plugin-version", COPILOT_PLUGIN_VERSION),
        ("user-agent", COPILOT_USER_AGENT),
        ("x-github-api-version", COPILOT_API_VERSION),
    ]
}

/// 把 Copilot 必带 headers 应用到 reqwest builder 上。
pub fn apply_request_headers(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    let mut b = builder;
    for (k, v) in copilot_request_headers() {
        b = b.header(k, v);
    }
    b
}

// ─── 计费分类 ───────────────────────────────────────────

/// 对 Anthropic Messages 请求体分类,返回 `x-initiator` 的值("user" / "agent")。
///
/// 语义对齐 cc-switch copilot_optimizer::classify_request:
/// 1. system 文本以 Claude Code 压缩专用前缀开头 → "agent"(压缩请求)
/// 2. 无消息 / 最后一条 role 非 user → "user"(安全默认)
/// 3. 最后一条 role=user 且 content 数组含 tool_result block → "agent"
///    (cc-switch 用「含 tool_result 即 agent」而非「全为 tool_result」:
///    skill/edit hook 等场景是 tool_result+text 混合形态,仍属工具续写)
/// 4. 其余 → "user"
pub fn classify_initiator(body: &Value) -> &'static str {
    // 信号 1:Claude Code 压缩请求(system 专用前缀,用户无法手动设置 system)
    if extract_system_text(body).starts_with(COMPACT_SYSTEM_PREFIX) {
        return "agent";
    }

    let Some(messages) = body.get("messages").and_then(|m| m.as_array()) else {
        return "user";
    };
    let Some(last) = messages.last() else {
        return "user";
    };
    // 只有最后一条 role=user 的消息需要细分,其余安全默认 user
    if last.get("role").and_then(|r| r.as_str()) != Some("user") {
        return "user";
    }

    match last.get("content") {
        Some(Value::Array(blocks)) => {
            let has_tool_result = blocks
                .iter()
                .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"));
            if has_tool_result {
                "agent"
            } else {
                "user"
            }
        }
        // 字符串 content / 缺失 → 用户输入
        _ => "user",
    }
}

/// 从请求体的 `system` 字段提取文本(string / block 数组两种形态)。
fn extract_system_text(body: &Value) -> String {
    match body.get("system") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(" "),
        _ => String::new(),
    }
}

// ─── 模型归一化 ─────────────────────────────────────────

/// 把客户端 Claude 4.x 模型 ID 归一化为 Copilot 上游接受的 dot 形式。
/// 返回 `None` 表示无需变换(已归一化、非 Claude 4.x、或空输入)。
/// 语义对齐 cc-switch copilot_model_map::normalize_to_copilot_id:
/// - `claude-sonnet-4-6` → `claude-sonnet-4.6`
/// - `claude-sonnet-4-6[1m]` → `claude-sonnet-4.6-1m`
/// - `claude-haiku-4-5-20251001` → `claude-haiku-4.5`(剥日期后缀)
/// - `claude-3-5-sonnet` 等历史三段版本不动(保守避免误伤)
pub fn normalize_model(model: &str) -> Option<String> {
    let trimmed = model.trim();
    let bytes = trimmed.as_bytes();

    if bytes.len() < 8 || !bytes[..7].eq_ignore_ascii_case(b"claude-") {
        return None;
    }

    let has_one_m_bracket = ends_with_ascii_ci(bytes, b"[1m]");

    // Fast path:已含点 + 不带 [1m] → 已是 Copilot 形式
    if trimmed.contains('.') && !has_one_m_bracket {
        return None;
    }

    let (base, has_1m_suffix) = split_one_m_suffix(trimmed);
    let stripped = strip_trailing_date(base);
    let dotted = dashes_to_dot_in_last_version(stripped);

    if dotted.is_none() && !has_1m_suffix {
        return None;
    }

    let mut candidate = dotted.unwrap_or_else(|| stripped.to_string());
    if has_1m_suffix {
        candidate.push_str("-1m");
    }
    (candidate != trimmed).then_some(candidate)
}

fn ends_with_ascii_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len()
        && haystack[haystack.len() - needle.len()..].eq_ignore_ascii_case(needle)
}

/// 拆出 `[1m]` / `-1m` 长上下文后缀,返回 (基础 ID, 是否带 1m)。
fn split_one_m_suffix(id: &str) -> (&str, bool) {
    let bytes = id.as_bytes();
    if ends_with_ascii_ci(bytes, b"[1m]") {
        return (&id[..bytes.len() - 4], true);
    }
    if ends_with_ascii_ci(bytes, b"-1m") {
        return (&id[..bytes.len() - 3], true);
    }
    (id, false)
}

/// 剥掉末尾 8 位日期段(如 `-20251001`)。
fn strip_trailing_date(id: &str) -> &str {
    let Some(last_dash) = id.rfind('-') else {
        return id;
    };
    let suffix = &id[last_dash + 1..];
    if suffix.len() == 8 && suffix.bytes().all(|b| b.is_ascii_digit()) {
        &id[..last_dash]
    } else {
        id
    }
}

/// 把 `…-X-Y`(末两段都是纯数字)变成 `…-X.Y`。
/// 模式不匹配返回 `None`(保守,避免误伤 `claude-3-5-sonnet` 等历史 ID)。
fn dashes_to_dot_in_last_version(id: &str) -> Option<String> {
    let last_dash = id.rfind('-')?;
    let last_segment = &id[last_dash + 1..];
    if last_segment.is_empty() || !last_segment.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let head = &id[..last_dash];
    let prev_dash = head.rfind('-')?;
    let prev_segment = &head[prev_dash + 1..];
    if prev_segment.is_empty() || !prev_segment.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(format!("{head}.{last_segment}"))
}

// ─── token 缓存 ─────────────────────────────────────────

/// 进程内 Copilot token 缓存:key 是 GitHub token 的 hash(不存明文),
/// value 是 (copilot_token, expires_at)。写法参考 gateway/probe_latency.rs。
static TOKEN_CACHE: Mutex<Option<HashMap<String, (String, i64)>>> = Mutex::new(None);

fn with_cache<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, (String, i64)>) -> R,
{
    let mut guard = TOKEN_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    f(guard.as_mut().unwrap())
}

/// GitHub token → 缓存 key(hash,不落明文)。
fn cache_key(github_token: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    github_token.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 取缓存的有效 Copilot token;到期前 `TOKEN_REFRESH_BUFFER_SECONDS` 内视为过期。
pub fn cached_token(github_token: &str) -> Option<String> {
    let now = chrono::Utc::now().timestamp();
    let key = cache_key(github_token);
    with_cache(|m| {
        m.get(&key).and_then(|(token, expires_at)| {
            (expires_at - now >= TOKEN_REFRESH_BUFFER_SECONDS).then(|| token.clone())
        })
    })
}

/// 写入交换得到的 Copilot token。
pub fn store_token(github_token: &str, copilot_token: &str, expires_at: i64) {
    let key = cache_key(github_token);
    with_cache(|m| {
        m.insert(key, (copilot_token.to_string(), expires_at));
    });
}

// ─── token 交换 ─────────────────────────────────────────

#[derive(serde::Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: i64,
}

/// 用 GitHub OAuth token 换取 Copilot bearer token。
/// `github_api_base` 可注入(wiremock 测试 / GHES),生产默认 api.github.com。
/// 返回 (copilot_token, expires_at)。
pub async fn exchange_copilot_token(
    client: &reqwest::Client,
    github_token: &str,
    github_api_base: &str,
) -> Result<(String, i64), AppError> {
    let url = format!(
        "{}/copilot_internal/v2/token",
        github_api_base.trim_end_matches('/')
    );

    let resp = client
        .get(&url)
        .header("Authorization", format!("token {github_token}"))
        .header("User-Agent", COPILOT_USER_AGENT)
        .header("Editor-Version", COPILOT_EDITOR_VERSION)
        .header("Editor-Plugin-Version", COPILOT_PLUGIN_VERSION)
        .header("copilot-integration-id", COPILOT_INTEGRATION_ID)
        .send()
        .await
        .map_err(|e| {
            AppError::new(
                crate::errors::codes::PROVIDER_REQUEST_FAILED,
                format!("连接 GitHub API 交换 Copilot token 失败: {e}"),
            )
            .with_suggestion("检查网络是否可达 api.github.com")
        })?;

    let status = resp.status();

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::new(
            crate::errors::codes::PROVIDER_REQUEST_FAILED,
            "GitHub token 无效或已过期,无法交换 Copilot token",
        )
        .with_suggestion(
            "确认 Provider 的 API Key 填的是有效的 GitHub OAuth token(gho_/ghu_ 开头),\
             并检查 GitHub 账号是否有 Copilot 订阅",
        ));
    }

    if status == reqwest::StatusCode::FORBIDDEN {
        return Err(AppError::new(
            crate::errors::codes::PROVIDER_REQUEST_FAILED,
            "GitHub 账号没有可用的 Copilot 订阅",
        )
        .with_suggestion(
            "检查该 GitHub token 对应账号的 Copilot 订阅状态:https://github.com/settings/copilot",
        ));
    }

    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::new(
            crate::errors::codes::PROVIDER_REQUEST_FAILED,
            format!("Copilot token 交换失败: HTTP {status}"),
        )
        .with_detail(body.chars().take(2000).collect::<String>())
        .with_suggestion("检查 GitHub token 是否有 Copilot 订阅,或稍后重试"));
    }

    let token_resp: CopilotTokenResponse = resp.json().await.map_err(|e| {
        AppError::new(
            crate::errors::codes::PROVIDER_REQUEST_FAILED,
            format!("解析 Copilot token 响应失败: {e}"),
        )
        .with_suggestion("GitHub API 返回了非预期格式,检查 token 是否为 Copilot 可用的 OAuth token")
    })?;

    Ok((token_resp.token, token_resp.expires_at))
}

/// 取有效的 Copilot token:缓存命中直接返回,否则交换并缓存。
///
/// 基址默认 api.github.com,可用 `AGENTGATE_COPILOT_GITHUB_API_BASE` 覆盖
/// (集成测试注入 wiremock;将来 GHES 也走这里)。
/// 并发未加锁:极端情况下重复交换一次,结果幂等、无副作用。
pub async fn get_copilot_token(
    client: &reqwest::Client,
    github_token: &str,
) -> Result<String, AppError> {
    if let Some(token) = cached_token(github_token) {
        return Ok(token);
    }
    let base = std::env::var("AGENTGATE_COPILOT_GITHUB_API_BASE")
        .unwrap_or_else(|_| DEFAULT_GITHUB_API_BASE.to_string());
    let (token, expires_at) = exchange_copilot_token(client, github_token, &base).await?;
    store_token(github_token, &token, expires_at);
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── is_copilot ──

    #[test]
    fn is_copilot_matches_type() {
        assert!(is_copilot("copilot"));
        assert!(is_copilot(" Copilot "));
        assert!(!is_copilot("anthropic"));
        assert!(!is_copilot("openai"));
    }

    // ── classify_initiator ──

    #[test]
    fn classify_plain_text_is_user() {
        let body = json!({
            "model": "claude-sonnet-4.6",
            "messages": [{"role": "user", "content": "帮我写段代码"}]
        });
        assert_eq!(classify_initiator(&body), "user");
    }

    #[test]
    fn classify_text_block_array_is_user() {
        let body = json!({
            "messages": [{"role": "user", "content": [{"type": "text", "text": "解释这段代码"}]}]
        });
        assert_eq!(classify_initiator(&body), "user");
    }

    #[test]
    fn classify_no_messages_is_user() {
        assert_eq!(classify_initiator(&json!({"model": "claude-sonnet-4.6"})), "user");
        assert_eq!(classify_initiator(&json!({"messages": []})), "user");
    }

    #[test]
    fn classify_last_role_not_user_is_user() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": "hello"}
            ]
        });
        assert_eq!(classify_initiator(&body), "user");
    }

    #[test]
    fn classify_tool_result_only_is_agent() {
        let body = json!({
            "messages": [
                {"role": "user", "content": "读文件"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "Read", "input": {}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "file contents"}
                ]}
            ]
        });
        assert_eq!(classify_initiator(&body), "agent");
    }

    #[test]
    fn classify_tool_result_with_text_is_agent() {
        // skill / edit hook 的常见形态:tool_result + text 混合,仍是工具续写。
        let body = json!({
            "messages": [{"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "ok"},
                {"type": "text", "text": "继续"}
            ]}]
        });
        assert_eq!(classify_initiator(&body), "agent");
    }

    #[test]
    fn classify_compact_system_string_is_agent() {
        let body = json!({
            "system": "You are a helpful AI assistant tasked with summarizing conversations. Summarize now.",
            "messages": [{"role": "user", "content": "历史对话……"}]
        });
        assert_eq!(classify_initiator(&body), "agent");
    }

    #[test]
    fn classify_compact_system_array_is_agent() {
        let body = json!({
            "system": [{"type": "text", "text": "You are a helpful AI assistant tasked with summarizing conversations."}],
            "messages": [{"role": "user", "content": "Summarize"}]
        });
        assert_eq!(classify_initiator(&body), "agent");
    }

    #[test]
    fn classify_manual_summarize_request_is_user() {
        // 用户手动要求总结,不能误判成压缩请求。
        let body = json!({
            "messages": [{"role": "user", "content": "Please summarize the conversation so far."}]
        });
        assert_eq!(classify_initiator(&body), "user");
    }

    // ── normalize_model ──

    #[test]
    fn normalize_dash_to_dot() {
        assert_eq!(
            normalize_model("claude-sonnet-4-6"),
            Some("claude-sonnet-4.6".to_string())
        );
        assert_eq!(
            normalize_model("claude-opus-4-6"),
            Some("claude-opus-4.6".to_string())
        );
        assert_eq!(
            normalize_model("claude-haiku-4-5"),
            Some("claude-haiku-4.5".to_string())
        );
    }

    #[test]
    fn normalize_one_m_bracket() {
        assert_eq!(
            normalize_model("claude-sonnet-4-6[1m]"),
            Some("claude-sonnet-4.6-1m".to_string())
        );
        assert_eq!(
            normalize_model("claude-sonnet-4.6[1m]"),
            Some("claude-sonnet-4.6-1m".to_string())
        );
    }

    #[test]
    fn normalize_strips_date_suffix() {
        assert_eq!(
            normalize_model("claude-haiku-4-5-20251001"),
            Some("claude-haiku-4.5".to_string())
        );
        assert_eq!(
            normalize_model("claude-sonnet-4-5-20250929"),
            Some("claude-sonnet-4.5".to_string())
        );
    }

    #[test]
    fn normalize_already_dotted_is_none() {
        assert_eq!(normalize_model("claude-sonnet-4.6"), None);
        assert_eq!(normalize_model("claude-opus-4.6-1m"), None);
    }

    #[test]
    fn normalize_legacy_and_non_claude_untouched() {
        assert_eq!(normalize_model("claude-3-5-sonnet"), None);
        assert_eq!(normalize_model("claude-3-5-sonnet-20241022"), None);
        assert_eq!(normalize_model("gpt-5"), None);
        assert_eq!(normalize_model(""), None);
    }

    // ── token 缓存 ──

    #[test]
    fn cache_hit_when_not_expiring() {
        let now = chrono::Utc::now().timestamp();
        store_token("gho_cache_hit", "copilot_tok_1", now + 3600);
        assert_eq!(
            cached_token("gho_cache_hit"),
            Some("copilot_tok_1".to_string())
        );
    }

    #[test]
    fn cache_miss_when_expiring_soon() {
        let now = chrono::Utc::now().timestamp();
        // 30 秒后过期,落在 60 秒提前刷新缓冲内 → 视为过期
        store_token("gho_cache_expiring", "copilot_tok_2", now + 30);
        assert_eq!(cached_token("gho_cache_expiring"), None);
        // 已过期同样 miss
        store_token("gho_cache_expired", "copilot_tok_3", now - 100);
        assert_eq!(cached_token("gho_cache_expired"), None);
    }

    #[test]
    fn cache_isolated_per_github_token() {
        let now = chrono::Utc::now().timestamp();
        store_token("gho_user_a", "tok_a", now + 3600);
        store_token("gho_user_b", "tok_b", now + 3600);
        assert_eq!(cached_token("gho_user_a"), Some("tok_a".to_string()));
        assert_eq!(cached_token("gho_user_b"), Some("tok_b".to_string()));
        assert_eq!(cached_token("gho_user_c"), None);
    }

    #[test]
    fn cache_key_does_not_contain_plaintext() {
        let key = cache_key("gho_secret_token_value");
        assert!(!key.contains("gho_secret_token_value"));
        assert!(!key.contains("secret"));
    }

    // ── 请求头 ──

    #[test]
    fn request_headers_contain_required_pairs() {
        let headers = copilot_request_headers();
        let find = |name: &str| {
            headers
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| *v)
        };
        assert_eq!(find("copilot-integration-id"), Some("vscode-chat"));
        assert_eq!(find("editor-version"), Some("vscode/1.110.1"));
        assert_eq!(find("editor-plugin-version"), Some("copilot-chat/0.38.2"));
        assert_eq!(find("user-agent"), Some("GitHubCopilotChat/0.38.2"));
        assert_eq!(find("x-github-api-version"), Some("2025-10-01"));
    }
}
