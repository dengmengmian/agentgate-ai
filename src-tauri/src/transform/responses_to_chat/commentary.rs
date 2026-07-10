//! `<commentary>` 标签剥离。
//!
//! 问题：Codex 的 system prompt 要求模型把中间进度更新发到 `commentary`
//! channel。GPT-5 走原生 Responses API 时 channel 是协议层概念，不出现在
//! 文本里；但第三方模型（DeepSeek 等）走 Chat Completions 没有 channel，
//! 只能在正文里模仿输出字面 `<commentary>...</commentary>` 标签——Codex
//! 客户端原样渲染，用户看到裸标签。
//!
//! 方案：转换回 Responses 事件时把标签本身删掉，**正文保留**（进度更新
//! 文本对用户有价值；且部分响应整段只有 commentary，若整块丢弃 Codex 会
//! 显示空回复）。流式场景标签可能被 chunk 边界切成半截（`<commen` /
//! `</comm`），[`CommentaryStripper`] 跨 chunk carry 半截标签，机制与
//! [`super::think::ThinkSplitter`] 相同。

use super::think::trailing_partial;

/// Codex channel 假标签集合。观测到的有 commentary（进度更新）与
/// context_addition（gpt-5.6 词表）；新增时同步补测试。
const CHANNEL_TAGS: &[&str] = &[
    "<commentary>",
    "</commentary>",
    "<context_addition>",
    "</context_addition>",
];

/// 无状态整段剥离：删除所有 channel 假标签（`<commentary>` 等），保留正文。
/// 用于非流式响应路径。
pub fn strip_commentary_tags(content: &str) -> String {
    if !content.contains('<') {
        return content.to_string();
    }
    let mut out = content.to_string();
    for tag in CHANNEL_TAGS {
        if out.contains(tag) {
            out = out.replace(tag, "");
        }
    }
    out
}

/// 流式 `<commentary>` 标签剥离器（有状态）。
///
/// chunk 边界可能落在标签中间，末尾疑似半截标签的字节 carry 到下一个
/// chunk 凑齐再判断。流结束时调一次 [`CommentaryStripper::flush`] 把
/// carry 残留按字面文本吐出。对不含标签的上游完全透明。
#[derive(Debug, Default)]
pub struct CommentaryStripper {
    /// 上一个 chunk 末尾残留的"可能是半截标签"的字节。
    carry: String,
}

impl CommentaryStripper {
    pub fn new() -> Self {
        Self::default()
    }

    /// 消费一段 chunk,返回当前能确定的可见文本。半截标签 carry 到下次调用。
    pub fn process_chunk(&mut self, chunk: &str) -> String {
        if chunk.is_empty() && self.carry.is_empty() {
            return String::new();
        }

        // carry + chunk 拼接后删完整标签
        let mut buffer = std::mem::take(&mut self.carry);
        buffer.push_str(chunk);
        let cleaned = strip_commentary_tags(&buffer);

        // 末尾疑似半截标签（所有标签里取最早的匹配起点）→ carry 到下一 chunk
        let carry_at = CHANNEL_TAGS
            .iter()
            .filter_map(|tag| trailing_partial(&cleaned, tag))
            .min();
        match carry_at {
            Some(at) => {
                self.carry = cleaned[at..].to_string();
                cleaned[..at].to_string()
            }
            None => cleaned,
        }
    }

    /// 流结束时调一次,把 carry 残留按字面文本返回(不再当标签)。
    pub fn flush(&mut self) -> String {
        std::mem::take(&mut self.carry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_removes_paired_tags_keeps_inner_text() {
        let input = "<commentary>\n正在使用 Sites 技能搭建网站。\n</commentary>";
        assert_eq!(
            strip_commentary_tags(input),
            "\n正在使用 Sites 技能搭建网站。\n"
        );
    }

    #[test]
    fn strip_handles_commentary_followed_by_final_text() {
        let input = "<commentary>先看目录结构。</commentary>\n\n改完了,共 3 个文件。";
        assert_eq!(
            strip_commentary_tags(input),
            "先看目录结构。\n\n改完了,共 3 个文件。"
        );
    }

    #[test]
    fn strip_passes_through_text_without_tags() {
        let input = "普通回答,不含标签。Vec<String> 也不受影响。";
        assert_eq!(strip_commentary_tags(input), input);
    }

    #[test]
    fn stripper_removes_tag_within_single_chunk() {
        let mut s = CommentaryStripper::new();
        let out = s.process_chunk("<commentary>hello</commentary>");
        assert_eq!(out, "hello");
        assert_eq!(s.flush(), "");
    }

    #[test]
    fn stripper_carries_partial_tag_across_chunks() {
        let mut s = CommentaryStripper::new();
        let mut out = String::new();
        out.push_str(&s.process_chunk("<commen"));
        out.push_str(&s.process_chunk("tary>正在检查</comm"));
        out.push_str(&s.process_chunk("entary> 完成"));
        out.push_str(&s.flush());
        assert_eq!(out, "正在检查 完成");
    }

    #[test]
    fn stripper_flushes_false_positive_partial_as_text() {
        let mut s = CommentaryStripper::new();
        // 末尾 "<comm" 疑似半截标签,先 carry
        let out1 = s.process_chunk("对比 a<comm");
        assert_eq!(out1, "对比 a");
        // 流结束,carry 是假阳性,按字面吐出
        assert_eq!(s.flush(), "<comm");
    }

    #[test]
    fn stripper_emits_carried_text_when_disproven_by_next_chunk() {
        let mut s = CommentaryStripper::new();
        let mut out = String::new();
        out.push_str(&s.process_chunk("Vec<c"));
        out.push_str(&s.process_chunk("har> 类型"));
        out.push_str(&s.flush());
        assert_eq!(out, "Vec<char> 类型");
    }

    #[test]
    fn strip_removes_context_addition_tags() {
        let input = "Let me look.\n\n<context_addition>Reading key files.\n</context_addition>";
        assert_eq!(
            strip_commentary_tags(input),
            "Let me look.\n\nReading key files.\n"
        );
    }

    #[test]
    fn stripper_removes_context_addition_across_chunks() {
        let mut s = CommentaryStripper::new();
        let mut out = String::new();
        out.push_str(&s.process_chunk("<context_ad"));
        out.push_str(&s.process_chunk("dition>看目录</context_addition>"));
        out.push_str(&s.flush());
        assert_eq!(out, "看目录");
    }

    #[test]
    fn stripper_passes_through_plain_text() {
        let mut s = CommentaryStripper::new();
        assert_eq!(s.process_chunk("普通文本流"), "普通文本流");
        assert_eq!(s.flush(), "");
    }
}
