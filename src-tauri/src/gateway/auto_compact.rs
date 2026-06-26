//! 长历史自压缩(autoCompact)。
//!
//! 背景:Codex 接 MiMo 这类小窗口(~128K)上游时,大对话(实测一条 ~300K tokens)有两个
//! 连带问题:① 超过上游真实上限直接 400;② Codex 的 remote compaction 又产不出它要的格式。
//! 根治办法是在网关侧主动摘要长历史,让请求落回阈值内。
//!
//! 策略(非增量,v1):
//! 1. 估算 messages 的 token(chars/4,图片按固定值)。低于阈值零改动。
//! 2. 按"工具配对块"切分:head = 开头 system,tail = 最近若干块(verbatim),middle = 中间老历史。
//! 3. 把 middle 渲染成纯文本 transcript,额外调一次上游摘要(关思考、无工具)。middle 过大时
//!    按 token 预算分块 map-reduce,避免摘要调用自身超上游上限。
//! 4. splice 回 head + [摘要消息] + tail。
//! 5. 摘要按 transcript 内容 hash 缓存(挡客户端重试/相同请求)。
//!
//! 只作用于 Responses→Chat 路径(Codex → MiMo / OpenAI 兼容)。Claude Code 不需要网关侧
//! 自压缩:它在客户端自己做 /compact,网关只在 messages 路由识别其压缩请求并关思考 +
//! 去工具(见 routes/messages.rs 的 is_claude_code_compaction)。Gemini 走各自分支不经过这里。
//! 默认开启。阈值按 provider-catalog 里模型的上下文窗口 ×85% 自适应(留 15% 给 reasoning +
//! output),未收录的模型退回默认 110K。窗口自适应保证大窗口 provider 不会被过早压缩,所以
//! 不再需要逐 provider 白名单。覆盖手段:`AGENTGATE_AUTO_COMPACT=off` 全关;
//! `AGENTGATE_AUTO_COMPACT_PROVIDERS` 设置后收窄为白名单;`AGENTGATE_AUTO_COMPACT_TOKENS`
//! 显式覆盖阈值。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use reqwest::Client;
use serde_json::{json, Value};

use crate::protocol::chat_completions::{
    CapabilityDegradationEvent, ChatCompletionsRequest, ChatMessage,
};
use crate::providers::adapter::{self, ProviderConfig};
use crate::providers::capabilities;

/// 未收录窗口的模型退回的默认触发阈值(绝对 token 数)。按 128K 上游留余量。
const DEFAULT_THRESHOLD_TOKENS: usize = 110_000;
/// 上下文窗口的可用比例:留 15% 给 reasoning + output,其余可装 history。
const CONTEXT_WINDOW_USAGE_PERCENT: usize = 85;
/// tail 预算:保留多少 token 的最近历史 verbatim。
const TAIL_BUDGET_TOKENS: usize = 40_000;
/// 单次摘要调用的 middle 输入分块预算(token)。须远小于上游上限,留空间给提示词+输出。
const CHUNK_BUDGET_TOKENS: usize = 80_000;
/// middle 低于此 token 数不值得单独发一次摘要调用,直接跳过压缩(省上游调用/成本/延迟)。
const MIN_MIDDLE_TOKENS: usize = 1_000;
/// 图片块的估算 token(粗略,按低分辨率计)。
const IMAGE_TOKENS: usize = 1024;
/// 单条消息渲染进 transcript 时的字符上限,防止单条巨大 tool 结果撑爆分块。
const PER_MESSAGE_CHAR_CAP: usize = 6000;
/// 摘要调用的输出上限。
const SUMMARY_MAX_TOKENS: i64 = 2000;

// ── 摘要缓存(content hash → summary,定长 LRU)──────────────────

static CACHE: Mutex<Option<HashMap<String, (String, u64)>>> = Mutex::new(None);
static CACHE_COUNTER: AtomicU64 = AtomicU64::new(0);
const CACHE_MAX_ENTRIES: usize = 256;

fn with_cache<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, (String, u64)>) -> R,
{
    let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    f(guard.as_mut().unwrap())
}

fn content_hash(text: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h1 = DefaultHasher::new();
    text.hash(&mut h1);
    let hash1 = h1.finish();
    let mut h2 = DefaultHasher::new();
    hash1.hash(&mut h2);
    text.hash(&mut h2);
    let hash2 = h2.finish();
    format!("{:016x}{:016x}_{}", hash1, hash2, text.len())
}

fn cache_get(key: &str) -> Option<String> {
    let counter = CACHE_COUNTER.fetch_add(1, Ordering::Relaxed);
    with_cache(|map| {
        map.get_mut(key).map(|(v, c)| {
            *c = counter;
            v.clone()
        })
    })
}

fn cache_put(key: String, value: String) {
    let counter = CACHE_COUNTER.fetch_add(1, Ordering::Relaxed);
    with_cache(|map| {
        if map.len() >= CACHE_MAX_ENTRIES {
            // 淘汰最旧 1/4
            let mut entries: Vec<(String, u64)> =
                map.iter().map(|(k, (_, c))| (k.clone(), *c)).collect();
            entries.sort_by_key(|(_, c)| *c);
            for (k, _) in entries.into_iter().take(CACHE_MAX_ENTRIES / 4) {
                map.remove(&k);
            }
        }
        map.insert(key, (value, counter));
    });
}

// ── token 估算 ──────────────────────────────────────────────

/// CJK 字符(中日韩 + 全角标点)。这类字符 1 字 ≈ 1 token,套 chars/4 会低估 2.5~4 倍,
/// 中文重的对话就估不到阈值、压缩不触发。
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF      // CJK 统一表意文字
        | 0x3400..=0x4DBF    // 扩展 A
        | 0x20000..=0x2A6DF  // 扩展 B
        | 0xF900..=0xFAFF    // 兼容表意文字
        | 0x3040..=0x30FF    // 日文平假名/片假名
        | 0xAC00..=0xD7AF    // 韩文音节
        | 0x3000..=0x303F    // CJK 标点
        | 0xFF00..=0xFFEF    // 全角形式
    )
}

/// 文本 token 估算:CJK 按 1 字 1 token,其余按 chars/4(英文/代码的经验值)。
fn estimate_text_tokens(s: &str) -> usize {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for c in s.chars() {
        if is_cjk(c) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other / 4
}

fn estimate_value_tokens(content: &Value) -> usize {
    match content {
        Value::String(s) => estimate_text_tokens(s),
        Value::Array(parts) => parts
            .iter()
            .map(|p| {
                let ty = p.get("type").and_then(|t| t.as_str()).unwrap_or("");
                if ty.contains("image") || p.get("image_url").is_some() {
                    IMAGE_TOKENS
                } else if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                    estimate_text_tokens(t)
                } else {
                    estimate_text_tokens(&p.to_string())
                }
            })
            .sum(),
        Value::Null => 0,
        other => estimate_text_tokens(&other.to_string()),
    }
}

fn estimate_msg_tokens(m: &ChatMessage) -> usize {
    let mut t = 4; // 每条消息的固定开销(role 等)
    if let Some(c) = &m.content {
        t += estimate_value_tokens(c);
    }
    if let Some(rc) = &m.reasoning_content {
        t += estimate_text_tokens(rc);
    }
    if let Some(tcs) = &m.tool_calls {
        for tc in tcs {
            t += estimate_text_tokens(&tc.function.name)
                + estimate_text_tokens(&tc.function.arguments)
                + 4;
        }
    }
    t
}

fn estimate_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(estimate_msg_tokens).sum()
}

// ── 工具配对块切分 ──────────────────────────────────────────

/// 把消息序列切成"块":assistant(含 tool_calls)+ 紧随的 tool 结果算一个不可分块,
/// 其余各自成块。只在块边界切割,保证 tool_calls ↔ tool 配对永不被切断。
fn group_blocks(messages: &[ChatMessage]) -> Vec<Vec<ChatMessage>> {
    let mut blocks: Vec<Vec<ChatMessage>> = Vec::new();
    for m in messages {
        if m.role == "tool" {
            // tool 结果挂到上一个块(它的 assistant 调用)。无上一个块则单独成块(异常历史容错)。
            if let Some(last) = blocks.last_mut() {
                last.push(m.clone());
            } else {
                blocks.push(vec![m.clone()]);
            }
        } else {
            // 其余消息(含带 tool_calls 的 assistant)各自起新块,后续 tool 结果再挂上来。
            blocks.push(vec![m.clone()]);
        }
    }
    blocks
}

fn block_tokens(block: &[ChatMessage]) -> usize {
    block.iter().map(estimate_msg_tokens).sum()
}

// ── transcript 渲染 ─────────────────────────────────────────

fn truncate_chars(s: &str, cap: usize) -> String {
    if s.chars().count() <= cap {
        return s.to_string();
    }
    let head: String = s.chars().take(cap).collect();
    format!("{head}…[截断]")
}

fn value_to_plain(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|p| {
                if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                    t.to_string()
                } else if p
                    .get("type")
                    .and_then(|t| t.as_str())
                    .is_some_and(|t| t.contains("image"))
                    || p.get("image_url").is_some()
                {
                    "[图片]".to_string()
                } else {
                    p.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn render_transcript(messages: &[ChatMessage]) -> String {
    let mut lines = Vec::new();
    for m in messages {
        let mut text = m.content.as_ref().map(value_to_plain).unwrap_or_default();
        if let Some(tcs) = &m.tool_calls {
            for tc in tcs {
                text.push_str(&format!(
                    "\n[调用工具 {}({})]",
                    tc.function.name,
                    truncate_chars(&tc.function.arguments, 500)
                ));
            }
        }
        let role = match m.role.as_str() {
            "tool" => "工具结果",
            "assistant" => "助手",
            "user" => "用户",
            "system" => "系统",
            other => other,
        };
        let text = truncate_chars(text.trim(), PER_MESSAGE_CHAR_CAP);
        if text.is_empty() {
            continue;
        }
        lines.push(format!("{role}: {text}"));
    }
    lines.join("\n")
}

// ── 规划:head / middle 分块 / tail ─────────────────────────

struct Plan {
    head: Vec<ChatMessage>,
    middle_chunks: Vec<Vec<ChatMessage>>,
    tail: Vec<ChatMessage>,
}

/// 计算压缩规划。返回 None 表示无需压缩(中段为空)。
fn plan_compaction(messages: &[ChatMessage], tail_budget: usize) -> Option<Plan> {
    let blocks = group_blocks(messages);
    if blocks.is_empty() {
        return None;
    }

    // head:开头连续的纯 system 块。
    let mut head_end = 0;
    while head_end < blocks.len() && blocks[head_end].iter().all(|m| m.role == "system") {
        head_end += 1;
    }

    // tail:从末尾往前累加块,直到达到预算。
    let mut tail_start = blocks.len();
    let mut acc = 0usize;
    while tail_start > head_end {
        let bt = block_tokens(&blocks[tail_start - 1]);
        if acc + bt > tail_budget && acc > 0 {
            break;
        }
        acc += bt;
        tail_start -= 1;
    }

    if tail_start <= head_end {
        return None; // head + tail 已覆盖全部,无中段可压
    }

    let head: Vec<ChatMessage> = blocks[..head_end].iter().flatten().cloned().collect();
    let tail: Vec<ChatMessage> = blocks[tail_start..].iter().flatten().cloned().collect();

    // middle 块按分块预算打包,供 map-reduce 摘要。
    let mut middle_chunks: Vec<Vec<ChatMessage>> = Vec::new();
    let mut cur: Vec<ChatMessage> = Vec::new();
    let mut cur_tok = 0usize;
    for block in &blocks[head_end..tail_start] {
        let bt = block_tokens(block);
        if cur_tok + bt > CHUNK_BUDGET_TOKENS && !cur.is_empty() {
            middle_chunks.push(std::mem::take(&mut cur));
            cur_tok = 0;
        }
        cur.extend(block.iter().cloned());
        cur_tok += bt;
    }
    if !cur.is_empty() {
        middle_chunks.push(cur);
    }

    if middle_chunks.is_empty() {
        return None;
    }

    // middle 过小不值得一次摘要 round-trip：跳过,让请求原样透传(仍在阈值边缘,
    // 下一轮历史再长一点自然会触发)。
    let middle_tokens: usize = middle_chunks.iter().map(|c| estimate_tokens(c)).sum();
    if middle_tokens < MIN_MIDDLE_TOKENS {
        return None;
    }

    Some(Plan {
        head,
        middle_chunks,
        tail,
    })
}

// ── splice ──────────────────────────────────────────────────

fn summary_message(summary: &str) -> ChatMessage {
    ChatMessage {
        role: "user".to_string(),
        content: Some(Value::String(format!(
            "以下是本次对话早前历史的摘要(为控制长度已由网关压缩,细节可能有损):\n\n{summary}"
        ))),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }
}

fn splice(head: &[ChatMessage], summary: &str, tail: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(head.len() + 1 + tail.len());
    out.extend(head.iter().cloned());
    out.push(summary_message(summary));
    out.extend(tail.iter().cloned());
    out
}

// ── 上游摘要调用 ────────────────────────────────────────────

const SUMMARY_SYSTEM: &str = "你是对话历史压缩器。把给定的对话片段压成精炼摘要,务必保留:\
关键决策与结论、代码改动与文件路径、已完成与未完成的任务、重要约束与参数。\
丢弃寒暄与冗余过程。只输出摘要正文,不要前后缀。";

/// 构造摘要请求。摘要不需要 reasoning(多产 item / 吃预算),但 `thinking` 是
/// MiMo / Kimi / DeepSeek 的请求方言;其余上游(OpenAI 官方等)对未知字段会 400,
/// 不能带。摘要调用直走 adapter、不经过 per-provider transform,所以在这里收口。
fn build_summary_request(
    config: &ProviderConfig,
    model: &str,
    transcript: &str,
) -> ChatCompletionsRequest {
    ChatCompletionsRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: Some(Value::String(SUMMARY_SYSTEM.to_string())),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: Some(Value::String(transcript.to_string())),
                reasoning_content: None,
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ],
        tools: None,
        tool_choice: None,
        stream: false,
        temperature: Some(0.2),
        top_p: None,
        max_tokens: Some(SUMMARY_MAX_TOKENS),
        max_completion_tokens: Some(SUMMARY_MAX_TOKENS),
        thinking: thinking_disabled_for(&config.provider_type),
        stream_options: None,
        response_format: None,
        reasoning_effort: None,
        seed: None,
        stop: None,
        frequency_penalty: None,
        presence_penalty: None,
        parallel_tool_calls: None,
        diagnostic_events: Vec::new(),
    }
}

/// 认识 `thinking` 字段的上游返回 `{"type":"disabled"}`,其余返回 None(不发)。
/// 名单与 transform/providers 的方言处理保持一致:MiMo / Kimi(moonshot)/ DeepSeek。
/// 漏列的代价只是摘要调用按上游默认跑(可能开思考),不会 400;错列才会 400,从严。
/// messages 路由处理 Claude Code 压缩请求时也用它关思考。
pub(crate) fn thinking_disabled_for(provider_type: &str) -> Option<Value> {
    let pt = provider_type.to_ascii_lowercase();
    let knows_thinking = pt == "deepseek"
        || pt == "kimi"
        || pt.contains("moonshot")
        || pt == "xiaomi"
        || pt.contains("mimo");
    knows_thinking.then(|| json!({"type": "disabled"}))
}

/// 调上游做一次摘要。`codex_compact` 模块借这个做 Codex v2 compaction 的本地实现。
pub(crate) async fn summarize_chunk(
    client: &Client,
    config: &ProviderConfig,
    model: &str,
    transcript: &str,
) -> Result<String, crate::errors::AppError> {
    let key = content_hash(transcript);
    if let Some(cached) = cache_get(&key) {
        return Ok(cached);
    }

    let mut req = build_summary_request(config, model, transcript);

    let resp = adapter::send_non_stream(client, config, &mut req).await?;
    let summary = resp
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            crate::errors::AppError::new(
                crate::errors::codes::UPSTREAM_NON_STREAM_ERROR,
                "auto_compact: 摘要调用返回空内容",
            )
        })?;

    cache_put(key, summary.clone());
    Ok(summary)
}

// ── 入口 ────────────────────────────────────────────────────

/// 判断该请求是否启用自压缩,并返回触发阈值(token 数)。
/// 默认开启;返回 None 表示本次不压缩。
fn threshold_for(config: &ProviderConfig, model: &str) -> Option<usize> {
    if auto_compact_disabled() || !provider_allowed(config) {
        return None;
    }
    Some(resolve_threshold(config, model))
}

/// `AGENTGATE_AUTO_COMPACT=off|0|false|no` 显式全关。
fn auto_compact_disabled() -> bool {
    std::env::var("AGENTGATE_AUTO_COMPACT")
        .ok()
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "off" | "0" | "false" | "no"
            )
        })
        .unwrap_or(false)
}

/// 未设 `AGENTGATE_AUTO_COMPACT_PROVIDERS` → 全部 provider 允许(默认开);
/// 设置后只允许名单内的 provider name / type。
fn provider_allowed(config: &ProviderConfig) -> bool {
    let Ok(list) = std::env::var("AGENTGATE_AUTO_COMPACT_PROVIDERS") else {
        return true;
    };
    if list.trim().is_empty() {
        return true;
    }
    list.split(',').any(|s| {
        let s = s.trim();
        !s.is_empty()
            && (s.eq_ignore_ascii_case(&config.name)
                || s.eq_ignore_ascii_case(&config.provider_type))
    })
}

/// 阈值优先级:`AGENTGATE_AUTO_COMPACT_TOKENS` 显式覆盖 > 用户配置的模型窗口 ×85%
/// > catalog 内置窗口 ×85% > 默认 110K。
fn resolve_threshold(config: &ProviderConfig, model: &str) -> usize {
    if let Some(explicit) = std::env::var("AGENTGATE_AUTO_COMPACT_TOKENS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|v| *v > 0)
    {
        return explicit;
    }
    config
        .model_context_windows
        .get(model)
        .copied()
        .or_else(|| capabilities::context_window_for(&config.provider_type, model))
        .map(|window| (window as usize) * CONTEXT_WINDOW_USAGE_PERCENT / 100)
        .unwrap_or(DEFAULT_THRESHOLD_TOKENS)
}

/// 若 chat_req 历史超阈值,就地压缩 messages。失败时降级为可观测的硬截断(保留 head+tail
/// 并插入显式标记),不静默吞错、不假成功。压缩/降级都会写 diagnostic_events 供日志追踪。
pub async fn maybe_compact(
    client: &Client,
    config: &ProviderConfig,
    chat_req: &mut ChatCompletionsRequest,
) {
    let Some(threshold) = threshold_for(config, &chat_req.model) else {
        return;
    };

    let original_tokens = estimate_tokens(&chat_req.messages);
    if original_tokens <= threshold {
        return;
    }

    // 小窗口 provider 阈值低于 tail 预算时,tail 会比整个阈值还大导致压缩失效;
    // 让 tail 不超过阈值的一半。
    let tail_budget = TAIL_BUDGET_TOKENS.min(threshold / 2);
    let Some(plan) = plan_compaction(&chat_req.messages, tail_budget) else {
        return;
    };

    let model = chat_req.model.clone();
    // 分块摘要并发调用(join_all 保序):长对话首压可能有 3-4 块,串行要 30s+,
    // 并发后墙钟时间 ≈ 最慢一块。
    let transcripts: Vec<String> = plan
        .middle_chunks
        .iter()
        .map(|chunk| render_transcript(chunk))
        .filter(|t| !t.is_empty())
        .collect();
    let chunk_count = transcripts.len();
    let results = futures::future::join_all(
        transcripts
            .iter()
            .map(|t| summarize_chunk(client, config, &model, t)),
    )
    .await;

    let mut summaries: Vec<String> = Vec::with_capacity(chunk_count);
    let mut failed = false;
    for r in results {
        match r {
            Ok(s) => summaries.push(s),
            Err(e) => {
                tracing::warn!(
                    provider = %config.name,
                    error = %e.message,
                    "auto_compact: 摘要调用失败,降级为硬截断"
                );
                failed = true;
                break;
            }
        }
    }

    let (summary_text, kind, message) = if failed {
        (
            "[摘要生成失败,早前历史已省略]".to_string(),
            "auto_compact_truncated",
            format!("auto_compact 摘要失败,硬截断 ~{original_tokens} tok 历史(保留 head+tail)"),
        )
    } else {
        (
            summaries.join("\n\n---\n\n"),
            "auto_compact",
            format!("auto_compact 压缩 {chunk_count} 段中段历史,原始 ~{original_tokens} tok"),
        )
    };

    chat_req.messages = splice(&plan.head, &summary_text, &plan.tail);

    let compacted_tokens = estimate_tokens(&chat_req.messages);
    chat_req.diagnostic_events.push(CapabilityDegradationEvent {
        kind: kind.to_string(),
        capability: "context".to_string(),
        source: "auto_compact".to_string(),
        provider: Some(config.name.clone()),
        model: Some(model),
        message,
        count: Some(compacted_tokens),
        reason: Some(format!("threshold={threshold}")),
    });

    tracing::info!(
        provider = %config.name,
        original_tokens,
        compacted_tokens,
        chunks = chunk_count,
        "auto_compact: 历史已压缩"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: Some(Value::String(content.to_string())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    fn asst_tool_call(id: &str, name: &str, args: &str) -> ChatMessage {
        use crate::protocol::chat_completions::{ToolCall, ToolCallFunction};
        ChatMessage {
            role: "assistant".to_string(),
            content: None,
            reasoning_content: None,
            tool_calls: Some(vec![ToolCall {
                id: id.to_string(),
                call_type: "function".to_string(),
                function: ToolCallFunction {
                    name: name.to_string(),
                    arguments: args.to_string(),
                },
            }]),
            tool_call_id: None,
            name: None,
        }
    }

    fn tool_result(id: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: "tool".to_string(),
            content: Some(Value::String(content.to_string())),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(id.to_string()),
            name: None,
        }
    }

    #[test]
    fn estimate_counts_cjk_as_one_token_per_char() {
        // 复现 bug:chars/4 对中文低估 2.5~4 倍(中文 1 字 ≈ 1 token),
        // 中文重的长对话估不到阈值、压缩不触发,上游 400 照样出现。
        let m = msg("user", &"中".repeat(400));
        let t = estimate_msg_tokens(&m);
        assert!(
            (400..=410).contains(&t),
            "400 个中文字应估 ~400 token,实际 {t}"
        );

        // 混合文本:200 中文 + 400 英文字符 → ~200 + ~100
        let m2 = msg("user", &format!("{}{}", "字".repeat(200), "a".repeat(400)));
        let t2 = estimate_msg_tokens(&m2);
        assert!((300..=315).contains(&t2), "混合文本应分开累计,实际 {t2}");
    }

    #[test]
    fn estimate_counts_text_and_images() {
        let mut m = msg("user", &"a".repeat(400)); // 400 chars → 100 tok + 4 overhead
        let t1 = estimate_msg_tokens(&m);
        assert!((100..=110).contains(&t1));

        m.content = Some(json!([
            {"type": "input_image", "image_url": "x"},
            {"type": "text", "text": "hi"}
        ]));
        let t2 = estimate_msg_tokens(&m);
        assert!(t2 >= IMAGE_TOKENS);
    }

    #[test]
    fn group_blocks_keeps_tool_pairing_intact() {
        let messages = vec![
            msg("system", "sys"),
            msg("user", "hi"),
            asst_tool_call("c1", "read", "{}"),
            tool_result("c1", "file contents"),
            msg("assistant", "done"),
        ];
        let blocks = group_blocks(&messages);
        // system / user / [asst+tool] / assistant = 4 块
        assert_eq!(blocks.len(), 4);
        // 第三块 = assistant tool_call + 其 tool 结果,绑在一起
        assert_eq!(blocks[2].len(), 2);
        assert_eq!(blocks[2][0].role, "assistant");
        assert_eq!(blocks[2][1].role, "tool");
    }

    #[test]
    fn plan_never_splits_tool_call_from_result() {
        // 构造:system + 多轮,中间含 tool_call/tool 配对,确保 plan 后无孤儿 tool。
        let mut messages = vec![msg("system", "sys")];
        for i in 0..40 {
            messages.push(msg("user", &format!("user turn {i} {}", "x".repeat(2000))));
            messages.push(asst_tool_call(&format!("c{i}"), "read", "{}"));
            messages.push(tool_result(&format!("c{i}"), &"r".repeat(2000)));
            messages.push(msg("assistant", &format!("reply {i} {}", "y".repeat(2000))));
        }

        let plan = plan_compaction(&messages, TAIL_BUDGET_TOKENS).expect("should compact");
        let spliced = splice(&plan.head, "SUMMARY", &plan.tail);

        // 校验:任何 role=="tool" 的消息,其前一条必是 assistant 且带 tool_calls。
        for (i, m) in spliced.iter().enumerate() {
            if m.role == "tool" {
                assert!(i > 0, "tool 不能是首条");
                let prev = &spliced[i - 1];
                assert_eq!(prev.role, "assistant", "tool 前必须是 assistant");
                assert!(
                    prev.tool_calls.as_ref().is_some_and(|t| !t.is_empty()),
                    "tool 前的 assistant 必须带 tool_calls"
                );
            }
        }
    }

    #[test]
    fn plan_preserves_head_and_tail_verbatim() {
        let mut messages = vec![msg("system", "SYSTEM_PROMPT")];
        for i in 0..50 {
            messages.push(msg("user", &format!("u{i} {}", "x".repeat(2000))));
            messages.push(msg("assistant", &format!("a{i} {}", "y".repeat(2000))));
        }
        let last_user = messages[messages.len() - 2].clone();
        let last_asst = messages[messages.len() - 1].clone();

        let plan = plan_compaction(&messages, TAIL_BUDGET_TOKENS).expect("should compact");
        let spliced = splice(&plan.head, "SUMMARY", &plan.tail);

        // head 第一条仍是原 system
        assert_eq!(spliced[0].role, "system");
        assert_eq!(
            spliced[0].content.as_ref().unwrap().as_str().unwrap(),
            "SYSTEM_PROMPT"
        );
        // 第二条是摘要消息
        assert_eq!(spliced[1].role, "user");
        assert!(spliced[1]
            .content
            .as_ref()
            .unwrap()
            .as_str()
            .unwrap()
            .contains("SUMMARY"));
        // 末尾两条仍是原始最近轮(verbatim)
        let n = spliced.len();
        assert_eq!(
            spliced[n - 2].content.as_ref().unwrap().as_str().unwrap(),
            last_user.content.as_ref().unwrap().as_str().unwrap()
        );
        assert_eq!(
            spliced[n - 1].content.as_ref().unwrap().as_str().unwrap(),
            last_asst.content.as_ref().unwrap().as_str().unwrap()
        );
    }

    #[test]
    fn plan_returns_none_when_short() {
        let messages = vec![
            msg("system", "sys"),
            msg("user", "hi"),
            msg("assistant", "yo"),
        ];
        assert!(plan_compaction(&messages, TAIL_BUDGET_TOKENS).is_none());
    }

    #[test]
    fn plan_skips_when_middle_too_small() {
        // middle 只剩一小块(~200 token),低于值得摘要的下限 → 应跳过,
        // 不为这点历史白发一次摘要 round-trip(省一次上游调用/成本/延迟)。
        let messages = vec![
            msg("system", "sys"),
            msg("user", &"旧".repeat(200)),      // middle ~200 tok
            msg("user", &"近".repeat(200)),      // tail
            msg("assistant", &"答".repeat(200)), // tail
        ];
        // tail_budget=500:tail 收下最近两块(~408 tok),只留 middle 一小块(~204)。
        assert!(
            plan_compaction(&messages, 500).is_none(),
            "middle 过小应跳过摘要"
        );
    }

    #[test]
    fn large_middle_splits_into_multiple_chunks() {
        // 中段远超 CHUNK_BUDGET_TOKENS,应分多块。
        let mut messages = vec![msg("system", "sys")];
        // 每条 ~5000 tok(20000 chars),200 条 → ~1M tok 中段
        for i in 0..200 {
            messages.push(msg("user", &format!("turn {i} {}", "x".repeat(20000))));
        }
        let plan = plan_compaction(&messages, TAIL_BUDGET_TOKENS).expect("should compact");
        assert!(
            plan.middle_chunks.len() > 1,
            "大中段应分多块,实际 {}",
            plan.middle_chunks.len()
        );
        // 每块不超预算(单块单条除外)
        for chunk in &plan.middle_chunks {
            let t: usize = chunk.iter().map(estimate_msg_tokens).sum();
            assert!(t <= CHUNK_BUDGET_TOKENS + 5000, "块 {t} 超预算过多");
        }
    }

    #[test]
    fn render_transcript_shows_tool_calls_and_results() {
        let messages = vec![
            msg("user", "请读文件"),
            asst_tool_call("c1", "read_file", r#"{"path":"a.rs"}"#),
            tool_result("c1", "fn main() {}"),
        ];
        let t = render_transcript(&messages);
        assert!(t.contains("用户: 请读文件"));
        assert!(t.contains("[调用工具 read_file"));
        assert!(t.contains("工具结果: fn main()"));
    }

    #[test]
    fn summary_request_gates_thinking_by_provider() {
        // 复现 bug:摘要请求曾硬编码 thinking:{"type":"disabled"},OpenAI 官方等
        // 不认识该字段的上游会 400 → 摘要必失败 → 长对话全部降级硬截断。
        let mimo = test_config("m", "mimo");
        let req = build_summary_request(&mimo, "mimo-v2.5-pro", "t");
        assert_eq!(
            req.thinking,
            Some(json!({"type": "disabled"})),
            "MiMo 认识 thinking 字段,摘要应显式关思考"
        );

        let deepseek = test_config("d", "deepseek");
        assert!(build_summary_request(&deepseek, "deepseek-chat", "t")
            .thinking
            .is_some());

        let openai = test_config("o", "openai");
        let req = build_summary_request(&openai, "gpt-5", "t");
        assert!(
            req.thinking.is_none(),
            "OpenAI 兼容上游不认识 thinking 字段,带上会 400"
        );

        let custom = test_config("c", "openai_compatible");
        assert!(
            build_summary_request(&custom, "m", "t").thinking.is_none(),
            "未知类型从严:漏列只是按上游默认跑,错列才会 400"
        );
    }

    #[test]
    fn cache_roundtrip_and_eviction() {
        let key = content_hash("unique-transcript-xyz");
        assert!(cache_get(&key).is_none());
        cache_put(key.clone(), "summary-A".to_string());
        assert_eq!(cache_get(&key), Some("summary-A".to_string()));
    }

    // env 是进程全局,用 #[serial(env)] 与其他动 env 的测试互斥。
    #[test]
    #[serial_test::serial(env)]
    fn threshold_env_scenarios() {
        let config = test_config("mimo", "openai");

        // 1. 默认(未设任何 env)→ 开启;未收录窗口的模型退回默认阈值
        std::env::remove_var("AGENTGATE_AUTO_COMPACT");
        std::env::remove_var("AGENTGATE_AUTO_COMPACT_PROVIDERS");
        std::env::remove_var("AGENTGATE_AUTO_COMPACT_TOKENS");
        assert_eq!(threshold_for(&config, "m"), Some(DEFAULT_THRESHOLD_TOKENS));

        // 2. 显式全关
        std::env::set_var("AGENTGATE_AUTO_COMPACT", "off");
        assert!(threshold_for(&config, "m").is_none());
        std::env::remove_var("AGENTGATE_AUTO_COMPACT");

        // 3. 白名单收窄:在名单内 → 开启
        std::env::set_var("AGENTGATE_AUTO_COMPACT_PROVIDERS", "openai");
        assert_eq!(threshold_for(&config, "m"), Some(DEFAULT_THRESHOLD_TOKENS));

        // 4. 白名单收窄:不在名单内 → 关闭
        std::env::set_var("AGENTGATE_AUTO_COMPACT_PROVIDERS", "deepseek");
        assert!(threshold_for(&config, "m").is_none());
        std::env::remove_var("AGENTGATE_AUTO_COMPACT_PROVIDERS");

        // 5. 显式 tokens 覆盖
        std::env::set_var("AGENTGATE_AUTO_COMPACT_TOKENS", "50000");
        assert_eq!(threshold_for(&config, "m"), Some(50000));
        std::env::remove_var("AGENTGATE_AUTO_COMPACT_TOKENS");

        // 6. 窗口自适应:catalog 收录的模型按窗口 ×85%
        let mimo = test_config("mimo", "mimo");
        assert_eq!(
            threshold_for(&mimo, "mimo-v2.5-pro"),
            Some(128_000 * CONTEXT_WINDOW_USAGE_PERCENT / 100)
        );

        // 7. 用户配置覆盖 catalog(256K > catalog 的 128K)
        let mut custom = test_config("x", "mimo");
        custom
            .model_context_windows
            .insert("mimo-v2.5-pro".to_string(), 256_000);
        assert_eq!(
            threshold_for(&custom, "mimo-v2.5-pro"),
            Some(256_000 * CONTEXT_WINDOW_USAGE_PERCENT / 100)
        );
    }

    fn test_config(name: &str, ptype: &str) -> ProviderConfig {
        ProviderConfig {
            name: name.to_string(),
            provider_type: ptype.to_string(),
            base_url: "http://localhost".to_string(),
            api_keys: vec!["k".to_string()],
            default_model: "m".to_string(),
            reasoning_model: None,
            timeout_seconds: 60,
            extra_headers: Default::default(),
            anthropic_base_url: None,
            responses_base_url: None,
            model_context_windows: Default::default(),
        }
    }
}
