//! Anthropic thinking 块 + 签名链的捕获与回放。
//!
//! 问题：Claude extended thinking 模式响应里每个 `thinking` 块带 `signature`
//! 字段——Claude 的"我生成了这段 thinking"密码学证明。下一轮对话必须把
//! 上一轮的 thinking 块**原样回传**（含签名），否则 Anthropic 400 拒绝
//! （sig-chain 校验失败）。`redacted_thinking` 同理：安全系统判定该段
//! thinking 不能明文回传，给一个加密 blob `data`，下次回传时把这个 blob
//! 原样塞回去。
//!
//! AgentGate 的角色：把 Anthropic 响应翻译成 OpenAI Responses 格式（client
//! 用 Codex/Claude Code 走 Responses API）。Responses 协议里 reasoning 项
//! 有 `encrypted_content` 字段——本质上等价于 Anthropic 的签名 blob 的位置。
//! 我们把所有 thinking 块编码进 `encrypted_content`，下一轮 client 把整个
//! reasoning 项回传，AgentGate 解码后还原出原始 Anthropic 块（含签名）。
//!
//! 编码方式：直接 JSON 字符串化 ThinkingBlock 数组并塞入 encrypted_content。
//! 不做 base64——免新依赖，serde_json 转义本身就能处理换行/引号。client
//! 把 encrypted_content 当 opaque 字符串原样 round-trip，不会窥探内部。
//!
//! 兼容性：解码失败（旧版本生成的 encrypted_content 是纯文本、或第三方
//! 数据）时返回空数组，调用方按"无 thinking 块"处理——不致命，最多丢失
//! 签名链让后续 thinking-mode 多轮调用降级到普通模式。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 单个 thinking 块——可能是普通 thinking 或 redacted_thinking。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThinkingBlock {
    /// "thinking" 或 "redacted_thinking"
    pub kind: String,
    /// 普通 thinking 的文本内容；redacted 时为空
    #[serde(default)]
    pub text: String,
    /// Anthropic 给的签名 blob——必须原样回传
    #[serde(default)]
    pub signature: String,
    /// redacted_thinking 的加密 data；普通 thinking 时为空
    #[serde(default)]
    pub data: String,
}

impl ThinkingBlock {
    pub fn thinking(text: impl Into<String>) -> Self {
        Self {
            kind: "thinking".into(),
            text: text.into(),
            ..Default::default()
        }
    }
    pub fn redacted(data: impl Into<String>) -> Self {
        Self {
            kind: "redacted_thinking".into(),
            data: data.into(),
            ..Default::default()
        }
    }
}

/// 把一组 thinking 块编码成 `encrypted_content` 字段值。空数组返回 None
/// （上游不是 Anthropic 时 reasoning 没有签名信息，encrypted_content 保持
/// "纯文本"传统行为）。
pub fn encode_for_encrypted_content(blocks: &[ThinkingBlock]) -> Option<String> {
    if blocks.is_empty() {
        return None;
    }
    // 用一个唯一前缀做版本标记 + 区分纯文本格式。serde_json 自动处理引号
    // / 换行 / 反斜杠转义，结果是合法 JSON 单行字符串。
    let payload = json!({ "v": 1, "blocks": blocks });
    Some(format!("agentgate-thinking-v1:{}", payload))
}

/// 解析 `encrypted_content` 字段，提取 thinking 块。
///
/// 仅识别 `agentgate-thinking-v1:` 前缀；其它格式（纯文本、第三方生成）
/// 返回空数组——调用方应按"无签名信息"处理。
pub fn decode_from_encrypted_content(s: &str) -> Vec<ThinkingBlock> {
    let Some(rest) = s.strip_prefix("agentgate-thinking-v1:") else {
        return Vec::new();
    };
    let Ok(payload): Result<Value, _> = serde_json::from_str(rest) else {
        return Vec::new();
    };
    payload
        .get("blocks")
        .and_then(|b| serde_json::from_value::<Vec<ThinkingBlock>>(b.clone()).ok())
        .unwrap_or_default()
}

/// 把一组 thinking 块转成可直接拼进 Anthropic `content` 数组的 JSON 块。
pub fn to_anthropic_content_blocks(blocks: &[ThinkingBlock]) -> Vec<Value> {
    blocks
        .iter()
        .filter_map(|b| match b.kind.as_str() {
            "thinking" => {
                // 没签名也照样发——Anthropic 接受但下一轮就再没签名链了。
                // 把无签名的 thinking 整段省掉更稳，避免 Anthropic 因签名
                // 缺失而 400（thinking_required_signature）。
                if b.signature.is_empty() {
                    None
                } else {
                    Some(json!({
                        "type": "thinking",
                        "thinking": b.text,
                        "signature": b.signature,
                    }))
                }
            }
            "redacted_thinking" => {
                if b.data.is_empty() {
                    None
                } else {
                    Some(json!({ "type": "redacted_thinking", "data": b.data }))
                }
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_thinking_with_signature() {
        let blocks = vec![ThinkingBlock {
            kind: "thinking".into(),
            text: "Let me think about this carefully.\n\nFirst, ...".into(),
            signature: "sig_abc123==".into(),
            data: String::new(),
        }];
        let encoded = encode_for_encrypted_content(&blocks).expect("non-empty");
        assert!(encoded.starts_with("agentgate-thinking-v1:"));
        let decoded = decode_from_encrypted_content(&encoded);
        assert_eq!(decoded, blocks);
    }

    #[test]
    fn round_trip_redacted_thinking() {
        let blocks = vec![ThinkingBlock::redacted("ENCRYPTED_BLOB_BASE64=")];
        let encoded = encode_for_encrypted_content(&blocks).expect("non-empty");
        let decoded = decode_from_encrypted_content(&encoded);
        assert_eq!(decoded, blocks);
    }

    #[test]
    fn round_trip_mixed_blocks() {
        let blocks = vec![
            ThinkingBlock {
                kind: "thinking".into(),
                text: "t1".into(),
                signature: "s1".into(),
                data: String::new(),
            },
            ThinkingBlock::redacted("REDACT1"),
            ThinkingBlock {
                kind: "thinking".into(),
                text: "t2".into(),
                signature: "s2".into(),
                data: String::new(),
            },
        ];
        let encoded = encode_for_encrypted_content(&blocks).unwrap();
        assert_eq!(decode_from_encrypted_content(&encoded), blocks);
    }

    #[test]
    fn encode_empty_returns_none() {
        assert!(encode_for_encrypted_content(&[]).is_none());
    }

    #[test]
    fn decode_plain_text_yields_empty() {
        // 旧版本 / 第三方上游生成的 encrypted_content 是纯文本——调用方
        // 拿到空数组，按"无签名"降级处理。
        assert!(decode_from_encrypted_content("just plain text").is_empty());
        assert!(decode_from_encrypted_content("").is_empty());
    }

    #[test]
    fn decode_corrupt_payload_yields_empty() {
        assert!(decode_from_encrypted_content("agentgate-thinking-v1:not-json").is_empty());
    }

    #[test]
    fn to_anthropic_skips_unsigned_thinking() {
        // 无签名的 thinking 块不能发给 Anthropic（会 400），过滤掉。
        let blocks = vec![
            ThinkingBlock::thinking("orphan without signature"),
            ThinkingBlock {
                kind: "thinking".into(),
                text: "valid".into(),
                signature: "sig".into(),
                data: String::new(),
            },
        ];
        let anth = to_anthropic_content_blocks(&blocks);
        assert_eq!(anth.len(), 1);
        assert_eq!(anth[0]["type"], "thinking");
        assert_eq!(anth[0]["thinking"], "valid");
    }

    #[test]
    fn to_anthropic_keeps_redacted_with_data() {
        let blocks = vec![ThinkingBlock::redacted("BLOB")];
        let anth = to_anthropic_content_blocks(&blocks);
        assert_eq!(anth.len(), 1);
        assert_eq!(anth[0]["type"], "redacted_thinking");
        assert_eq!(anth[0]["data"], "BLOB");
    }

    #[test]
    fn to_anthropic_skips_redacted_without_data() {
        let blocks = vec![ThinkingBlock::redacted("")];
        let anth = to_anthropic_content_blocks(&blocks);
        assert!(anth.is_empty());
    }

    #[test]
    fn encoding_handles_special_chars_in_text() {
        // 签名常含 base64 padding (=)、JSON 转义敏感字符（"\, 换行等）。
        let blocks = vec![ThinkingBlock {
            kind: "thinking".into(),
            text: "Line 1\nLine 2 with \"quotes\" and \\backslash".into(),
            signature: "sig+with/slashes=padding==".into(),
            data: String::new(),
        }];
        let encoded = encode_for_encrypted_content(&blocks).unwrap();
        assert_eq!(decode_from_encrypted_content(&encoded), blocks);
    }
}
