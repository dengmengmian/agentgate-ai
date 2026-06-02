//! Line-level surgical edits to TOML files.
//!
//! Preserves user comments, blank lines, key style choices, and unrelated
//! keys/sections byte-for-byte. The cost is that we only support two narrow
//! operations:
//!
//!   - [`upsert_top_level_key`] — replace or insert a top-level scalar key
//!     (top-level = appearing before any `[section]` header).
//!   - [`upsert_section`] — replace or insert a `[header]` section and its
//!     keys; the section runs from its header line to the next `[...]` /
//!     `[[...]]` header or EOF.
//!
//! Why not use a real TOML parser like `toml_edit`? Codex's `config.toml`
//! carries `[projects]` trust levels, `[mcp_servers]` configs, `[notice]`
//! migration tables, inline comments documenting non-obvious flags, and
//! `model_reasoning_effort` set on top by the user. Round-tripping through
//! any TOML library reorders the file (sections by alpha, keys by encounter
//! order), drops comments, and normalises string quoting. Users complain.
//! `toml_edit` does preserve comments but still reorders and re-quotes —
//! good for editors, bad for "I diff this file with my notes".
//!
//! AtomCode has the same issue with `[providers.<name>]`: a user can have
//! their own `[providers.deepseek]` next to ours.
//!
//! Line-based editing dodges the round-trip cost. Limitations:
//!   - **No inline tables**: `key = { a = 1, b = 2 }` is just one line and
//!     gets replaced wholesale if the key matches. The keys we manage
//!     (`model_provider`, `default_provider`) are scalar strings; the values
//!     we write are scalar; so this is fine for our two callers.
//!   - **Trailing line comments on replaced lines are dropped.** Same as any
//!     "find the key and rewrite the line" approach. Acceptable trade-off
//!     since the affected keys are AgentGate-owned.
//!   - **CRLF input → LF output.** `str::lines()` handles \r\n on the way in;
//!     we emit `\n`. Tauri write paths always use LF anyway.

/// Insert or replace a top-level scalar key. `raw_value` is the TOML literal
/// — caller is responsible for quoting strings (`"\"OpenAI\""`) and using
/// the right bare form for numbers / bools.
///
/// Semantics:
///   - If `key` exists as a top-level key (before any `[section]`), the line
///     is replaced in place. Any comments / blank lines before it are kept.
///   - If `key` does not exist, the new line is inserted just before the
///     first section header; or appended to the end if the file has no
///     sections.
///   - Same-named keys *inside* sections are not touched.
pub fn upsert_top_level_key(content: &str, key: &str, raw_value: &str) -> String {
    let new_line = format!("{key} = {raw_value}");
    let mut out = String::new();
    let mut written = false;
    let mut hit_section = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        // First section header: if we still haven't written, insert just before it.
        if !hit_section && is_section_header(trimmed) {
            if !written {
                out.push_str(&new_line);
                out.push('\n');
                written = true;
            }
            hit_section = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // Top-level region, key-matching line → replace.
        if !hit_section && !written {
            if let Some(name) = top_level_key_name(line) {
                if name == key {
                    out.push_str(&new_line);
                    out.push('\n');
                    written = true;
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }

    // No section header and key not found → append.
    if !written {
        out.push_str(&new_line);
        out.push('\n');
    }

    preserve_final_newline(content, out)
}

/// Insert or replace a `[header]` section. `header` is the bare section path
/// (e.g. `model_providers.OpenAI`) — no surrounding brackets. `body` is the
/// section's key=value lines (one per line, no trailing newline required;
/// no `[header]` line — we add it).
///
/// Semantics:
///   - The first occurrence of `[header]` is replaced; the section's content
///     (until the next `[...]` / `[[...]]` header or EOF) is dropped and
///     swapped for `body`.
///   - Subsequent same-named occurrences (rare; usually malformed configs)
///     are removed.
///   - If `[header]` doesn't appear, the section is appended to the end of
///     the file with one blank line of separation from preceding content.
pub fn upsert_section(content: &str, header: &str, body: &str) -> String {
    let target = format!("[{header}]");
    let mut out = String::new();
    let mut inserted = false;
    let mut skipping = false;

    for line in content.lines() {
        let trimmed = line.trim_start();

        // Inside the section we're replacing: drop lines until the next header.
        if skipping {
            if is_section_header(trimmed) {
                skipping = false;
                // fall through and process this line normally
            } else {
                continue;
            }
        }

        if header_matches(trimmed, &target) {
            if !inserted {
                out.push_str(&target);
                out.push('\n');
                out.push_str(body);
                if !body.ends_with('\n') {
                    out.push('\n');
                }
                inserted = true;
            }
            // skip the rest of this section even on duplicate occurrences
            skipping = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }

    if !inserted {
        // Append, with one blank line of separation if the file isn't empty.
        if !out.is_empty() && !out.ends_with("\n\n") {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }
        out.push_str(&target);
        out.push('\n');
        out.push_str(body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }

    preserve_final_newline(content, out)
}

// ── helpers ─────────────────────────────────────────────────────

/// True if `trimmed` (already start-trimmed) begins a TOML section.
/// Covers both `[section]` and `[[array_of_tables]]`.
fn is_section_header(trimmed: &str) -> bool {
    trimmed.starts_with('[')
}

/// Does `trimmed` (start-trimmed) name the section `target` (a string like
/// `"[model_providers.OpenAI]"`)? Tolerates trailing whitespace and `#`
/// comments after the closing `]`.
fn header_matches(trimmed: &str, target: &str) -> bool {
    let no_comment = trimmed.split('#').next().unwrap_or(trimmed).trim_end();
    no_comment == target
}

/// Extract the bare key name of a top-level `key = value` line. Returns
/// `None` for blanks, comments, section headers, and lines we don't recognise
/// as a simple bare-key assignment.
fn top_level_key_name(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('[') {
        return None;
    }
    let eq = trimmed.find('=')?;
    let key_part = trimmed[..eq].trim();
    if key_part.is_empty() {
        return None;
    }
    // TOML bare keys are `[A-Za-z0-9_-]+`. Quoted keys like `"a.b" = 1` we
    // don't support — none of our callers write or want them.
    if !key_part.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return None;
    }
    Some(key_part)
}

/// If the input didn't end with a newline, neither does the output. This
/// keeps the diff against the user's file minimal — most editors preserve
/// the trailing-newline convention of the file they opened.
fn preserve_final_newline(input: &str, mut output: String) -> String {
    if input.is_empty() {
        return output;
    }
    let input_terminated = input.ends_with('\n');
    let output_terminated = output.ends_with('\n');
    if !input_terminated && output_terminated {
        output.pop();
    } else if input_terminated && !output_terminated {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── upsert_top_level_key ──

    #[test]
    fn top_level_inserts_into_empty_file() {
        let out = upsert_top_level_key("", "model_provider", "\"OpenAI\"");
        assert_eq!(out, "model_provider = \"OpenAI\"\n");
    }

    #[test]
    fn top_level_replaces_existing_key_in_place() {
        let input = "# my header comment\nmodel = \"gpt-5\"\nmodel_provider = \"old\"\nother = 1\n";
        let out = upsert_top_level_key(input, "model_provider", "\"OpenAI\"");
        assert_eq!(out, "# my header comment\nmodel = \"gpt-5\"\nmodel_provider = \"OpenAI\"\nother = 1\n");
    }

    #[test]
    fn top_level_inserts_before_first_section() {
        let input = "[mcp_servers]\nfoo = 1\n";
        let out = upsert_top_level_key(input, "model_provider", "\"OpenAI\"");
        assert_eq!(out, "model_provider = \"OpenAI\"\n[mcp_servers]\nfoo = 1\n");
    }

    #[test]
    fn top_level_does_not_touch_key_inside_section() {
        let input = "[mcp_servers]\nmodel_provider = \"nope\"\n";
        let out = upsert_top_level_key(input, "model_provider", "\"OpenAI\"");
        // Inserted before [mcp_servers], the in-section key is untouched.
        assert_eq!(out, "model_provider = \"OpenAI\"\n[mcp_servers]\nmodel_provider = \"nope\"\n");
    }

    #[test]
    fn top_level_appends_when_no_section_and_no_existing_key() {
        let input = "model = \"gpt-5\"\nother = 1\n";
        let out = upsert_top_level_key(input, "model_provider", "\"OpenAI\"");
        assert_eq!(out, "model = \"gpt-5\"\nother = 1\nmodel_provider = \"OpenAI\"\n");
    }

    #[test]
    fn top_level_preserves_blank_lines_and_comments_above() {
        let input = "# Codex config\n\n# managed by me\napproval_policy = \"on-request\"\n";
        let out = upsert_top_level_key(input, "model_provider", "\"OpenAI\"");
        assert_eq!(
            out,
            "# Codex config\n\n# managed by me\napproval_policy = \"on-request\"\nmodel_provider = \"OpenAI\"\n"
        );
    }

    #[test]
    fn top_level_drops_trailing_line_comment_on_replaced_line_intentionally() {
        // 已知折衷：替换整行 → 丢掉行尾注释。AgentGate-managed key 才会被替换，
        // 用户也不该往这个 key 上加注释。
        let input = "model_provider = \"old\"  # was the old name\n";
        let out = upsert_top_level_key(input, "model_provider", "\"OpenAI\"");
        assert_eq!(out, "model_provider = \"OpenAI\"\n");
    }

    // ── upsert_section ──

    #[test]
    fn section_inserts_into_empty_file() {
        let out = upsert_section("", "model_providers.OpenAI", "name = \"OpenAI\"\n");
        assert_eq!(out, "[model_providers.OpenAI]\nname = \"OpenAI\"\n");
    }

    #[test]
    fn section_replaces_existing_body_keeps_header_position() {
        let input = "# pre\n[model_providers.OpenAI]\nold_key = 1\nstill_old = 2\n[mcp_servers]\nfoo = 1\n";
        let out = upsert_section(
            input,
            "model_providers.OpenAI",
            "name = \"OpenAI\"\nbase_url = \"http://x/v1\"\n",
        );
        assert_eq!(
            out,
            "# pre\n[model_providers.OpenAI]\nname = \"OpenAI\"\nbase_url = \"http://x/v1\"\n[mcp_servers]\nfoo = 1\n"
        );
    }

    #[test]
    fn section_replaces_at_end_of_file() {
        let input = "[model_providers.OpenAI]\nold = 1\nstill_old = 2\n";
        let out = upsert_section(input, "model_providers.OpenAI", "x = 1\n");
        assert_eq!(out, "[model_providers.OpenAI]\nx = 1\n");
    }

    #[test]
    fn section_appended_with_blank_line_when_missing() {
        let input = "model_provider = \"OpenAI\"\n";
        let out = upsert_section(input, "model_providers.OpenAI", "name = \"x\"\n");
        assert_eq!(out, "model_provider = \"OpenAI\"\n\n[model_providers.OpenAI]\nname = \"x\"\n");
    }

    #[test]
    fn section_dedupes_duplicate_occurrences() {
        // pathological config with two copies of the same section
        let input = "[x]\na=1\n[x]\nb=2\n[y]\nc=3\n";
        let out = upsert_section(input, "x", "z=9\n");
        assert_eq!(out, "[x]\nz=9\n[y]\nc=3\n");
    }

    #[test]
    fn section_preserves_unrelated_sections() {
        let input = "[providers.deepseek]\nkey = \"sk-real\"\n[providers.kimi]\nkey = \"sk-kimi\"\n";
        let out = upsert_section(input, "providers.agentgate", "type = \"openai\"\n");
        assert_eq!(
            out,
            "[providers.deepseek]\nkey = \"sk-real\"\n[providers.kimi]\nkey = \"sk-kimi\"\n\n[providers.agentgate]\ntype = \"openai\"\n"
        );
    }

    #[test]
    fn section_does_not_match_when_name_is_only_a_prefix() {
        // `[model_providers.OpenAI]` should not be matched by header
        // `model_providers.Open` — header_matches checks full equality.
        let input = "[model_providers.OpenAI]\na = 1\n[model_providers.OpenAI2]\nb = 2\n";
        let out = upsert_section(input, "model_providers.Open", "c = 3\n");
        assert!(out.contains("[model_providers.OpenAI]"));
        assert!(out.contains("[model_providers.OpenAI2]"));
        assert!(out.contains("[model_providers.Open]"));
    }

    #[test]
    fn section_tolerates_trailing_comment_on_header_line() {
        let input = "[model_providers.OpenAI]  # AgentGate's hijack\nold = 1\n";
        let out = upsert_section(input, "model_providers.OpenAI", "new = 1\n");
        assert_eq!(out, "[model_providers.OpenAI]\nnew = 1\n");
    }

    #[test]
    fn section_array_of_tables_acts_as_boundary() {
        // `[[arr]]` is its own boundary — replacing `[x]` should stop at it.
        let input = "[x]\na = 1\nold = 2\n[[arr]]\nname = \"first\"\n";
        let out = upsert_section(input, "x", "a = 1\n");
        assert_eq!(out, "[x]\na = 1\n[[arr]]\nname = \"first\"\n");
    }

    // ── combo (mirrors the Codex apply path) ──

    #[test]
    fn combo_codex_apply_preserves_user_config() {
        let user_config = "\
# User's notes
approval_policy = \"on-request\"
model_reasoning_effort = \"high\"

[projects.\"/home/me/repo\"]
trust_level = \"trusted\"

[mcp_servers.local]
command = \"my-mcp-server\"
";
        let host = "127.0.0.1";
        let port = 9090;
        let token = "ag_local_xxx";

        // Mirror tools/codex.rs::apply: write model_provider + the [model_providers.OpenAI] section.
        let mut c = user_config.to_string();
        c = upsert_top_level_key(&c, "model_provider", "\"OpenAI\"");
        let body = format!(
            "name = \"OpenAI\"\nbase_url = \"http://{host}:{port}/v1\"\nwire_api = \"responses\"\nexperimental_bearer_token = \"{token}\"\nrequires_openai_auth = true\n"
        );
        c = upsert_section(&c, "model_providers.OpenAI", &body);

        // User's stuff survives.
        assert!(c.contains("approval_policy = \"on-request\""));
        assert!(c.contains("model_reasoning_effort = \"high\""));
        assert!(c.contains("[projects.\"/home/me/repo\"]"));
        assert!(c.contains("trust_level = \"trusted\""));
        assert!(c.contains("[mcp_servers.local]"));
        // Our managed bits land.
        assert!(c.contains("model_provider = \"OpenAI\""));
        assert!(c.contains("[model_providers.OpenAI]"));
        assert!(c.contains("base_url = \"http://127.0.0.1:9090/v1\""));
        assert!(c.contains("experimental_bearer_token = \"ag_local_xxx\""));
    }

    #[test]
    fn combo_codex_apply_idempotent() {
        // 第二次 apply 应该和第一次结果一致——line-level edit 必须幂等。
        let host = "127.0.0.1";
        let port = 9090;
        let token = "ag_local_xxx";
        let body = format!(
            "name = \"OpenAI\"\nbase_url = \"http://{host}:{port}/v1\"\nwire_api = \"responses\"\nexperimental_bearer_token = \"{token}\"\nrequires_openai_auth = true\n"
        );

        let mut once = String::new();
        once = upsert_top_level_key(&once, "model_provider", "\"OpenAI\"");
        once = upsert_section(&once, "model_providers.OpenAI", &body);

        let mut twice = once.clone();
        twice = upsert_top_level_key(&twice, "model_provider", "\"OpenAI\"");
        twice = upsert_section(&twice, "model_providers.OpenAI", &body);

        assert_eq!(once, twice, "second apply must be a no-op");
    }

    #[test]
    fn combo_atomcode_preserves_other_providers() {
        let user_config = "\
default_provider = \"deepseek\"

[providers.deepseek]
type = \"openai\"
api_key = \"sk-user-key\"
model = \"deepseek-chat\"
base_url = \"https://api.deepseek.com/v1\"

[providers.kimi]
type = \"openai\"
api_key = \"sk-kimi\"
";
        let mut c = user_config.to_string();
        c = upsert_top_level_key(&c, "default_provider", "\"agentgate\"");
        c = upsert_section(
            &c,
            "providers.agentgate",
            "type = \"openai\"\napi_key = \"ag_local_x\"\nmodel = \"agentgate\"\nbase_url = \"http://127.0.0.1:9090/v1\"\ncontext_window = 1000000\n",
        );

        assert!(c.contains("default_provider = \"agentgate\""));
        assert!(c.contains("[providers.agentgate]"));
        assert!(c.contains("[providers.deepseek]"));
        assert!(c.contains("api_key = \"sk-user-key\""));
        assert!(c.contains("[providers.kimi]"));
        assert!(c.contains("api_key = \"sk-kimi\""));
    }

    // ── final newline preservation ──

    #[test]
    fn preserves_no_trailing_newline() {
        let input = "model = \"gpt-5\""; // no trailing \n
        let out = upsert_top_level_key(input, "model_provider", "\"OpenAI\"");
        // 我们插入的是另一个顶级 key，原 key 之后 → 输出仍不以 \n 结尾。
        assert!(!out.ends_with('\n'));
        assert!(out.contains("model = \"gpt-5\""));
        assert!(out.contains("model_provider = \"OpenAI\""));
    }

    #[test]
    fn preserves_trailing_newline() {
        let input = "model = \"gpt-5\"\n";
        let out = upsert_top_level_key(input, "model_provider", "\"OpenAI\"");
        assert!(out.ends_with('\n'));
    }

}
