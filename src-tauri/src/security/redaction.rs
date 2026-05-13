/// Unified redaction utility for sensitive values.

/// Redact a string that looks like an API key or token.
pub fn redact_value(val: &str) -> String {
    if val.len() <= 8 {
        return "*".repeat(val.len());
    }
    let prefix_len = if val.starts_with("ag_local_") { 9 } else if val.starts_with("sk-") { 3 } else { 4 };
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

    result
}

fn redact_pattern(text: &str, prefix: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;

    while let Some(start) = remaining.find(prefix) {
        result.push_str(&remaining[..start]);
        let after = &remaining[start..];
        // Find end of token (whitespace, quote, comma, brace, or end)
        let end = after.find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == '}' || c == ')').unwrap_or(after.len());
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
}

fn redact_bearer(text: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;
    let pattern = "Bearer ";

    while let Some(start) = remaining.find(pattern) {
        result.push_str(&remaining[..start]);
        result.push_str("Bearer ");
        let after = &remaining[start + pattern.len()..];
        let end = after.find(|c: char| c.is_whitespace() || c == '"' || c == '\'').unwrap_or(after.len());
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
