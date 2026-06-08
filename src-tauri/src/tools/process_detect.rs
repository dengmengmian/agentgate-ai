//! Look up running processes by basename so the UI can warn users that the
//! client they just (re)configured is still alive in some terminal and needs
//! a restart to pick up the new config.
//!
//! Unix shells out to `pgrep -i -l <name>` (substring + case-insensitive,
//! basename-only). Windows path is a TODO: returns an empty list so the
//! caller treats it as "couldn't detect" rather than failing the apply.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, specta::Type)]
pub struct RunningProcess {
    pub pid: u32,
    pub command: String,
}

/// Find processes whose basename matches any of `needles`. Each needle is
/// passed to `pgrep` separately; results are merged and deduplicated by
/// PID. Returns an empty list on Windows or whenever `pgrep` is missing.
pub fn find_running(needles: &[&str]) -> Vec<RunningProcess> {
    #[cfg(unix)]
    {
        find_unix(needles)
    }
    #[cfg(not(unix))]
    {
        let _ = needles;
        Vec::new()
    }
}

#[cfg(unix)]
fn find_unix(needles: &[&str]) -> Vec<RunningProcess> {
    use std::process::Command;

    let mut all: Vec<RunningProcess> = Vec::new();
    for name in needles {
        let needle = name.trim();
        if needle.is_empty() {
            continue;
        }
        let Ok(output) = Command::new("pgrep").args(["-i", "-l", needle]).output() else {
            continue;
        };
        if !output.status.success() {
            // pgrep returns 1 when nothing matches, which is the common case.
            // Anything else (binary missing, permission denied) we just skip.
            continue;
        }
        all.extend(parse_pgrep(&String::from_utf8_lossy(&output.stdout)));
    }

    // Drop the AgentGate process itself in case a needle accidentally
    // matched it (e.g. searching for "claude" near a misnamed binary).
    all.retain(|p| !is_self_process(&p.command));

    all.sort_by_key(|p| p.pid);
    all.dedup_by_key(|p| p.pid);
    all
}

fn parse_pgrep(output: &str) -> Vec<RunningProcess> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.splitn(2, char::is_whitespace);
            let pid_str = parts.next()?;
            let command = parts.next()?.trim();
            let pid = pid_str.parse::<u32>().ok()?;
            Some(RunningProcess {
                pid,
                command: command.to_string(),
            })
        })
        .collect()
}

fn is_self_process(command: &str) -> bool {
    let lc = command.to_ascii_lowercase();
    lc.contains("agentgate")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pgrep_typical_output() {
        let out = "12345 codex\n67890 Codex Helper\n";
        let parsed = parse_pgrep(out);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].pid, 12345);
        assert_eq!(parsed[0].command, "codex");
        assert_eq!(parsed[1].pid, 67890);
        assert_eq!(parsed[1].command, "Codex Helper");
    }

    #[test]
    fn parse_pgrep_handles_blank_lines() {
        let out = "\n12345 claude\n\n";
        let parsed = parse_pgrep(out);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].command, "claude");
    }

    #[test]
    fn parse_pgrep_rejects_non_numeric_pid() {
        let out = "abc claude\n12345 codex\n";
        let parsed = parse_pgrep(out);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].pid, 12345);
    }

    #[test]
    fn is_self_filters_agentgate_basename() {
        assert!(is_self_process("agentgate"));
        assert!(is_self_process("AgentGate"));
        assert!(!is_self_process("claude"));
        assert!(!is_self_process("codex"));
    }
}
