//! 流式字节 → UTF-8 字符串的跨 chunk 安全转换。
//!
//! 背景:上游 SSE 的网络 chunk 边界可能切在多字节字符(中文/emoji)中间。
//! 逐 chunk `from_utf8_lossy` 会把前半截变成 �,后半截字节也跟着废——中文重度
//! 流式输出受影响最大。参考 cc-switch 的 append_utf8_safe:把残留的不完整
//! 序列(≤3 字节)缓存到 pending,等下个 chunk 补齐;真正非法的字节才 lossy 替换。

/// 把 `chunk` 追加到 `out`,跨 chunk 的不完整 UTF-8 尾部缓存在 `pending`。
/// 调用方为每条流维护一个独立的 `pending: Vec<u8>`;流结束时 pending 里
/// 至多残留 3 字节的半个字符,随流丢弃(与"无尾随换行的残行被丢弃"行为一致)。
pub(crate) fn append_utf8_safe(out: &mut String, pending: &mut Vec<u8>, chunk: &[u8]) {
    // pending(上个 chunk 的不完整尾部)拼上本次 chunk 后再解码。
    let owned: Vec<u8>;
    let mut rest: &[u8] = if pending.is_empty() {
        chunk
    } else {
        pending.extend_from_slice(chunk);
        owned = std::mem::take(pending);
        &owned
    };

    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                return;
            }
            Err(e) => {
                let (valid, after) = rest.split_at(e.valid_up_to());
                // valid_up_to 保证 valid 是合法 UTF-8,from_utf8 不会失败。
                out.push_str(std::str::from_utf8(valid).unwrap_or(""));
                match e.error_len() {
                    // None = 尾部是被切断的多字节序列开头:缓存等下个 chunk。
                    // UTF-8 单字符至多 4 字节,残留必然 ≤3;防御性兜底超长时 lossy。
                    None if after.len() <= 3 => {
                        pending.extend_from_slice(after);
                        return;
                    }
                    None => {
                        out.push_str(&String::from_utf8_lossy(after));
                        return;
                    }
                    // Some(n) = 真正非法的字节序列:替换为 � 后继续解码剩余部分。
                    Some(n) => {
                        out.push('\u{FFFD}');
                        rest = &after[n..];
                    }
                }
            }
        }
    }
}

/// 在不超过 `max` 字节的最近字符边界处截断,避免 `&s[..n]` 切在多字节字符
/// 中间 panic(pass_through 的 sse_log 截断曾有此隐患,中文日志高概率触发)。
pub(crate) fn truncate_at_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(chunks: &[&[u8]]) -> String {
        let mut out = String::new();
        let mut pending = Vec::new();
        for c in chunks {
            append_utf8_safe(&mut out, &mut pending, c);
        }
        out
    }

    #[test]
    fn cjk_split_across_chunks_stays_intact() {
        // 复现 bug:"中"(E4 B8 AD)被 TCP 边界切成 1+2 字节,
        // 旧逻辑两边各自 lossy → "���",正确结果应是 "中"。
        let bytes = "中".as_bytes();
        assert_eq!(feed(&[&bytes[..1], &bytes[1..]]), "中");
        // 2+1 切法
        assert_eq!(feed(&[&bytes[..2], &bytes[2..]]), "中");
    }

    #[test]
    fn emoji_4byte_split_across_chunks_stays_intact() {
        let bytes = "🚀".as_bytes(); // 4 字节
        assert_eq!(feed(&[&bytes[..1], &bytes[1..]]), "🚀");
        assert_eq!(feed(&[&bytes[..3], &bytes[3..]]), "🚀");
    }

    #[test]
    fn mixed_text_with_split_inside_sentence() {
        let s = "data: {\"c\":\"你好世界\"}\n";
        let bytes = s.as_bytes();
        // 在 "好" 的第二个字节处切断
        let cut = s.find("好").unwrap() + 1;
        assert_eq!(feed(&[&bytes[..cut], &bytes[cut..]]), s);
    }

    #[test]
    fn truly_invalid_bytes_are_replaced_not_buffered() {
        // 0xFF 永远不是合法 UTF-8 起始,应立即替换为 �,不能滞留 pending
        assert_eq!(feed(&[b"ab\xFFcd"]), "ab\u{FFFD}cd");
    }

    #[test]
    fn pure_ascii_passes_through() {
        assert_eq!(feed(&[b"hello ", b"world"]), "hello world");
    }

    #[test]
    fn truncate_backs_off_to_char_boundary() {
        // "中文" = 6 字节,在 4 处硬切会落在"文"中间 → 旧逻辑 panic。
        assert_eq!(truncate_at_char_boundary("中文", 4), "中");
        assert_eq!(truncate_at_char_boundary("中文", 3), "中");
        assert_eq!(truncate_at_char_boundary("中文", 2), "");
        // 不需要截断时原样返回
        assert_eq!(truncate_at_char_boundary("中文", 6), "中文");
        assert_eq!(truncate_at_char_boundary("abc", 10), "abc");
    }

    #[test]
    fn pending_does_not_leak_across_complete_chunks() {
        let mut out = String::new();
        let mut pending = Vec::new();
        append_utf8_safe(&mut out, &mut pending, "完整".as_bytes());
        assert!(pending.is_empty(), "完整 chunk 后 pending 应为空");
        assert_eq!(out, "完整");
    }
}
