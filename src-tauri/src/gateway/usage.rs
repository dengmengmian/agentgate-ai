//! 从上游响应里提取 (input_tokens, output_tokens)。
//!
//! 三种上游协议的 usage 字段形态不同,这里收敛为单一来源,供各协议 handler 共用,
//! 避免在 routes.rs 里散落多份字段映射、改一处漏一处。

use serde_json::Value;

/// 一次请求的 token 计数。把原先 `log_request_success` 尾部 4 个相邻
/// `Option<i64>`(input/output/cache_write/cache_read)收敛成具名字段,
/// 消除"传参写反顺序"这一类隐患——4 个同型 `Option<i64>` 排在一起最易出错。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TokenUsage {
    pub input: Option<i64>,
    pub output: Option<i64>,
    pub cache_write: Option<i64>,
    pub cache_read: Option<i64>,
}

/// 按一组候选 key 从 usage 对象里读第一个命中的 i64。
fn read_first(usage: Option<&Value>, keys: &[&str]) -> Option<i64> {
    let usage = usage?;
    keys.iter()
        .find_map(|k| usage.get(*k).and_then(|v| v.as_i64()))
}

/// OpenAI Chat / Responses 形态:`usage.{prompt_tokens|input_tokens}` /
/// `usage.{completion_tokens|output_tokens}`。兼容两套字段名。
pub fn extract_chat(upstream: &Value) -> (Option<i64>, Option<i64>) {
    let usage = upstream.get("usage");
    let input = read_first(usage, &["prompt_tokens", "input_tokens"]);
    let output = read_first(usage, &["completion_tokens", "output_tokens"]);
    (input, output)
}

/// Anthropic Messages 形态:`usage.input_tokens` / `usage.output_tokens`。
pub fn extract_anthropic(upstream: &Value) -> (Option<i64>, Option<i64>) {
    let usage = upstream.get("usage");
    let input = read_first(usage, &["input_tokens"]);
    let output = read_first(usage, &["output_tokens"]);
    (input, output)
}

/// Gemini 形态:`usageMetadata.promptTokenCount` / `usageMetadata.candidatesTokenCount`。
pub fn extract_gemini(upstream: &Value) -> (Option<i64>, Option<i64>) {
    let usage = upstream.get("usageMetadata");
    let input = read_first(usage, &["promptTokenCount"]);
    let output = read_first(usage, &["candidatesTokenCount"]);
    (input, output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn chat_reads_prompt_completion_tokens() {
        let v = json!({"usage": {"prompt_tokens": 12, "completion_tokens": 34}});
        assert_eq!(extract_chat(&v), (Some(12), Some(34)));
    }

    #[test]
    fn chat_falls_back_to_input_output_tokens() {
        let v = json!({"usage": {"input_tokens": 5, "output_tokens": 7}});
        assert_eq!(extract_chat(&v), (Some(5), Some(7)));
    }

    #[test]
    fn anthropic_reads_input_output_tokens() {
        let v = json!({"usage": {"input_tokens": 100, "output_tokens": 200}});
        assert_eq!(extract_anthropic(&v), (Some(100), Some(200)));
    }

    #[test]
    fn gemini_reads_usage_metadata() {
        let v = json!({"usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 11}});
        assert_eq!(extract_gemini(&v), (Some(9), Some(11)));
    }

    #[test]
    fn missing_usage_yields_none() {
        let v = json!({});
        assert_eq!(extract_chat(&v), (None, None));
        assert_eq!(extract_anthropic(&v), (None, None));
        assert_eq!(extract_gemini(&v), (None, None));
    }
}
