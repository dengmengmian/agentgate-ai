//! `<think>` 标签处理:无状态整段切分(split_think_tags)与
//! 跨 chunk 的流式切分器(ThinkSplitter)。

/// Split `<think>...</think>` tags from content into reasoning_content.
/// Used for MiniMax-like providers that embed thinking in content.
/// Handles multiple `<think>` blocks by extracting all of them.
pub fn split_think_tags(content: &str) -> (String, Option<String>) {
    let mut remaining = String::new();
    let mut thinking_parts = Vec::new();
    let mut search_from = 0;

    while let Some(start) = content[search_from..].find("<think>") {
        let abs_start = search_from + start;
        // Append text before this <think> tag
        remaining.push_str(&content[search_from..abs_start]);

        if let Some(end) = content[abs_start..].find("</think>") {
            let abs_end = abs_start + end;
            let block = content[abs_start + 7..abs_end].trim();
            if !block.is_empty() {
                thinking_parts.push(block.to_string());
            }
            search_from = abs_end + 8;
        } else {
            // No closing tag — keep the rest as-is
            search_from = abs_start;
            break;
        }
    }
    // Append any trailing text
    remaining.push_str(&content[search_from..]);

    // Only trim if think tags were actually found and extracted
    if !thinking_parts.is_empty() {
        let trimmed = remaining.trim().to_string();
        (trimmed, Some(thinking_parts.join("\n\n")))
    } else {
        (remaining, None)
    }
}

/// 流式 `<think>...</think>` 切分器（有状态）。
///
/// 用于：上游用 inline `<think>` 模式（MiMo / GLM-air / Skywork / 部分 Qwen）
/// 流式输出 content 时，chunk 边界可能落在标签中间（`<thi` / `</th`），无状态
/// 的 [`split_think_tags`] 会把半截标签当文本——下个 chunk 来时也认不出剩余。
///
/// 解法：跨 chunk **carry 半截标签**，凑齐才识别。`process_chunk` 返回当前能确
/// 定的 `(visible, reasoning)`，stream 结束时调一次 [`ThinkSplitter::flush`] 把
/// carry 残留按当前状态（in_think? 是 reasoning : 是 visible）emit 出去。
///
/// 对没有 inline `<think>` 的上游（独立 reasoning_content 字段或没 reasoning）
/// 完全透明：所有 content 当 visible 原样输出。
#[derive(Debug, Default)]
pub struct ThinkSplitter {
    /// 上一个 chunk 末尾残留的"可能是半截开始/结束标签"的字节。
    carry: String,
    /// 当前是否在 `<think>...</think>` 内部。
    in_think: bool,
}

impl ThinkSplitter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 消费一段 chunk content，返回当前能确定的 `(visible_text, reasoning_extracted)`。
    /// 半截标签会 carry 到下一次 `process_chunk`，不会泄露到 visible。
    pub fn process_chunk(&mut self, chunk: &str) -> (String, Option<String>) {
        if chunk.is_empty() {
            return (String::new(), None);
        }

        // carry + chunk 拼接为本次工作 buffer
        let mut buffer = std::mem::take(&mut self.carry);
        buffer.push_str(chunk);

        let mut visible = String::new();
        let mut reasoning = String::new();
        let mut i: usize = 0;

        while i < buffer.len() {
            if !self.in_think {
                // 状态：在普通文本里，找 `<think>` 开始标签
                if let Some(rel_start) = buffer[i..].find("<think>") {
                    visible.push_str(&buffer[i..i + rel_start]);
                    i = i + rel_start + 7; // 跳过 "<think>"
                    self.in_think = true;
                    continue;
                }
                // 没找到完整开始标签。检查末尾是不是半截 `<think>` 前缀
                if let Some(carry_offset) = trailing_partial(&buffer[i..], "<think>") {
                    visible.push_str(&buffer[i..i + carry_offset]);
                    self.carry = buffer[i + carry_offset..].to_string();
                } else {
                    visible.push_str(&buffer[i..]);
                }
                break;
            } else {
                // 状态：在 think 内，找 `</think>` 结束标签
                if let Some(rel_end) = buffer[i..].find("</think>") {
                    reasoning.push_str(&buffer[i..i + rel_end]);
                    i = i + rel_end + 8; // 跳过 "</think>"
                    self.in_think = false;
                    continue;
                }
                if let Some(carry_offset) = trailing_partial(&buffer[i..], "</think>") {
                    reasoning.push_str(&buffer[i..i + carry_offset]);
                    self.carry = buffer[i + carry_offset..].to_string();
                } else {
                    reasoning.push_str(&buffer[i..]);
                }
                break;
            }
        }

        let reasoning_opt = if reasoning.is_empty() {
            None
        } else {
            Some(reasoning)
        };
        (visible, reasoning_opt)
    }

    /// 流结束时调一次。carry 残留按当前状态 emit：in_think → reasoning，否则 visible。
    /// 半截标签按字面文本处理（不再当标签 carry）。
    pub fn flush(&mut self) -> (String, Option<String>) {
        let carry = std::mem::take(&mut self.carry);
        if carry.is_empty() {
            return (String::new(), None);
        }
        if self.in_think {
            (String::new(), Some(carry))
        } else {
            (carry, None)
        }
    }
}

/// 检测 `s` 的末尾是否是 `target` 的非空前缀。返回 carry 起点（即末尾匹配前缀的起始 byte）。
/// 仅对 ASCII target 有效（`<think>` / `</think>` 都是 ASCII）。
pub(super) fn trailing_partial(s: &str, target: &str) -> Option<usize> {
    let s_bytes = s.as_bytes();
    let t_bytes = target.as_bytes();
    // target 前缀长度从最长往最短试，找第一个匹配。
    // 最长 = min(target.len()-1, s.len())——完整匹配已在 .find() 阶段处理。
    let max_k = (t_bytes.len() - 1).min(s_bytes.len());
    for k in (1..=max_k).rev() {
        let tail = &s_bytes[s_bytes.len() - k..];
        let head = &t_bytes[..k];
        if tail == head {
            return Some(s_bytes.len() - k);
        }
    }
    None
}
