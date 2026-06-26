/// Unified redaction utility for sensitive values.

/// Redact a string that looks like an API key or token.
pub fn redact_value(val: &str) -> String {
    if val.len() <= 8 {
        return "*".repeat(val.len());
    }
    let prefix_len = if val.starts_with("ag_local_") {
        9
    } else if val.starts_with("sk-") {
        3
    } else {
        4
    };
    let suffix_len = 4;
    let prefix = &val[..prefix_len.min(val.len())];
    let suffix = &val[val.len().saturating_sub(suffix_len)..];
    format!("{prefix}••••••••{suffix}")
}

/// Redact all sensitive patterns in a text block.
pub fn redact_text(text: &str) -> String {
    let mut result = text.to_string();

    // Redact ag_local_ tokens
    result = redact_pattern(&result, "ag_local_");
    // Redact sk- keys
    result = redact_pattern(&result, "sk-");
    // Redact Bearer tokens in headers
    result = redact_bearer(&result);
    // 命名键值:header 行 / JSON 字段两种形态(大小写不敏感)。
    // 注意顺序:x-api-key 先于 api-key,配合词边界避免后者重复命中前者的后缀。
    for name in [
        "x-goog-api-key",
        "x-api-key",
        "api-key",
        "api_key",
        "apikey",
        "access_token",
    ] {
        result = redact_named_value(&result, name);
    }
    // URL 查询参数 ?key= / &key=(Gemini 风格)。"key" 太泛,只在查询参数位置处理。
    result = redact_query_param(&result, "key");

    result
}

/// 大小写不敏感地查找 ASCII `needle`。命中位置必为 char 边界(ASCII 字节
/// 不会是多字节字符的延续字节),后续按字节切片安全。
fn find_ci(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < from + n.len() {
        return None;
    }
    (from..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// 脱敏 `name` 键的值,覆盖 `name: value`(header 行)和 `"name": "value"`
/// (JSON 字段)两种形态。要求:name 前是词边界;name 与值之间出现 `:` 或 `=`;
/// 值长度 >8 才脱敏(短值不像密钥)。
fn redact_named_value(text: &str, name: &str) -> String {
    let mut result = String::new();
    let mut pos = 0;
    while let Some(start) = find_ci(text, name, pos) {
        let after_name = start + name.len();
        // 词边界:避免 "monkey" 命中 "key"、"x-api-key" 命中 "api-key"。
        if start > 0 && is_word_byte(text.as_bytes()[start - 1]) {
            result.push_str(&text[pos..after_name]);
            pos = after_name;
            continue;
        }
        result.push_str(&text[pos..after_name]);
        pos = after_name;

        let rest = &text[after_name..];
        let val_start = rest
            .find(|c: char| !matches!(c, '"' | '\'' | ':' | '=' | ' ' | '\t'))
            .unwrap_or(rest.len());
        let sep = &rest[..val_start];
        // 必须是键值对(有 : 或 =),否则只是普通文本里提到了这个词。
        if !(sep.contains(':') || sep.contains('=')) {
            continue;
        }
        let val_rest = &rest[val_start..];
        let val_end = val_rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | '}' | ')' | '&'))
            .unwrap_or(val_rest.len());
        result.push_str(sep);
        if val_end > 8 {
            result.push_str(&redact_value(&val_rest[..val_end]));
        } else {
            result.push_str(&val_rest[..val_end]);
        }
        pos = after_name + val_start + val_end;
    }
    result.push_str(&text[pos..]);
    result
}

/// 脱敏 URL 查询参数 `?name=value` / `&name=value` 的值。
fn redact_query_param(text: &str, name: &str) -> String {
    let mut result = String::new();
    let mut pos = 0;
    let bytes = text.as_bytes();
    while let Some(start) = find_ci(text, name, pos) {
        let after_name = start + name.len();
        let prev_is_qmark_or_amp = start > 0 && matches!(bytes[start - 1], b'?' | b'&');
        let next_is_eq = bytes.get(after_name) == Some(&b'=');
        result.push_str(&text[pos..after_name]);
        pos = after_name;
        if !(prev_is_qmark_or_amp && next_is_eq) {
            continue;
        }
        let val_rest = &text[after_name + 1..];
        let val_end = val_rest
            .find(|c: char| c.is_whitespace() || matches!(c, '&' | '"' | '\''))
            .unwrap_or(val_rest.len());
        result.push('=');
        if val_end > 8 {
            result.push_str(&redact_value(&val_rest[..val_end]));
        } else {
            result.push_str(&val_rest[..val_end]);
        }
        pos = after_name + 1 + val_end;
    }
    result.push_str(&text[pos..]);
    result
}

fn redact_pattern(text: &str, prefix: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;

    while let Some(start) = remaining.find(prefix) {
        result.push_str(&remaining[..start]);
        let after = &remaining[start..];
        // Find end of token (whitespace, quote, comma, brace, or end)
        let end = after
            .find(|c: char| {
                c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}' || c == ')'
            })
            .unwrap_or(after.len());
        if end > 8 {
            let token = &after[..end];
            result.push_str(&redact_value(token));
        } else {
            result.push_str(&after[..end]);
        }
        remaining = &after[end..];
    }
    result.push_str(remaining);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redact_short_value() {
        assert_eq!(redact_value("abc"), "***");
        assert_eq!(redact_value("abcdefgh"), "********");
    }

    #[test]
    fn test_redact_ag_local_token() {
        let token = "ag_local_abcdefghijklmnopqrstuvwxyz1234";
        let redacted = redact_value(token);
        assert!(redacted.starts_with("ag_local_"));
        assert!(redacted.contains("••••••••"));
        assert!(redacted.ends_with("1234"));
    }

    #[test]
    fn test_redact_sk_key() {
        let key = "sk-abcdefghijklmnopqrstuvwxyz1234567890";
        let redacted = redact_value(key);
        assert!(redacted.starts_with("sk-"));
        assert!(redacted.contains("••••••••"));
        assert!(redacted.ends_with("7890"));
    }

    #[test]
    fn test_redact_generic_long_value() {
        let val = "mysecretkey1234567890abcdef";
        let redacted = redact_value(val);
        assert_eq!(redacted, "myse••••••••cdef");
    }

    #[test]
    fn test_redact_text_ag_local() {
        let text = "token is ag_local_abc123xyz789 and more";
        let result = redact_text(text);
        assert!(!result.contains("ag_local_abc123xyz789"));
        assert!(result.contains("ag_local_"));
        assert!(result.contains("••••••••"));
    }

    #[test]
    fn test_redact_text_sk_key() {
        let text = "key: sk-live-12345abcdef, ok";
        let result = redact_text(text);
        assert!(!result.contains("sk-live-12345abcdef"));
        assert!(result.contains("sk-"));
    }

    #[test]
    fn test_redact_text_bearer() {
        let text = "Authorization: Bearer supersecrettoken12345";
        let result = redact_text(text);
        assert!(!result.contains("supersecrettoken12345"));
        assert!(result.contains("Bearer "));
        assert!(result.contains("••••••••"));
    }

    #[test]
    fn test_redact_text_multiple_tokens() {
        let text = "tokens: ag_local_abc123 and sk-xyz789";
        let result = redact_text(text);
        assert!(!result.contains("ag_local_abc123"));
        assert!(!result.contains("sk-xyz789"));
    }

    #[test]
    fn test_redact_text_no_match() {
        let text = "hello world, no secrets here";
        assert_eq!(redact_text(text), text);
    }

    // ── server 端脱敏加固:x-api-key / api_key 字段 / ?key= 查询参数 ──

    #[test]
    fn test_redact_x_api_key_header_line_and_json() {
        // header 行形态
        let t1 = redact_text("x-api-key: supersecretvalue123");
        assert!(!t1.contains("supersecretvalue123"));
        // JSON header map 形态(大小写不敏感)
        let t2 = redact_text(r#"{"X-Api-Key": "supersecretvalue123"}"#);
        assert!(!t2.contains("supersecretvalue123"));
    }

    #[test]
    fn test_redact_api_key_json_field() {
        let t = redact_text(r#"{"api_key": "longsecret1234567890", "model": "m"}"#);
        assert!(!t.contains("longsecret1234567890"));
        assert!(t.contains(r#""model": "m""#), "其他字段不受影响");
    }

    #[test]
    fn test_redact_gemini_query_key_param() {
        let t = redact_text("POST https://generativelanguage.googleapis.com/v1beta/models/g:streamGenerateContent?key=AIzaSyABCDEF1234567890&alt=sse");
        assert!(!t.contains("AIzaSyABCDEF1234567890"));
        assert!(t.contains("&alt=sse"), "后续参数保留");
    }

    #[test]
    fn test_named_value_word_boundary_no_false_positive() {
        // "monkey=..." 不能因为含 "key" 被误脱敏;短值也不动
        let text = "monkey=12345678901234 and donkey: abcdefghijkl";
        assert_eq!(redact_text(text), text);
    }

    #[test]
    fn test_redact_access_token_field() {
        let t = redact_text(r#"{"access_token": "tok_1234567890abcdef"}"#);
        assert!(!t.contains("tok_1234567890abcdef"));
    }
}

fn redact_bearer(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;
    let pattern = "Bearer ";

    while let Some(start) = remaining.find(pattern) {
        result.push_str(&remaining[..start]);
        result.push_str("Bearer ");
        let after = &remaining[start + pattern.len()..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(after.len());
        if end > 4 {
            result.push_str(&redact_value(&after[..end]));
        } else {
            result.push_str(&after[..end]);
        }
        remaining = &after[end..];
    }
    result.push_str(remaining);
    result
}
