//! effort 覆盖辅助:按 env 配置对 reasoning_effort 做 fill / floor 两层兜底。

/// 应用两层 effort env 覆盖。floor 优先（最激进，覆盖客户端）；fill 次之
/// （仅 None 时补）。两层都不命中则返回原值。
///
/// env 配置示例：
///   AGENTGATE_FORCE_HIGH_EFFORT_PROVIDERS=mimo,deepseek  # None → high
///   AGENTGATE_EFFORT_FLOOR_PROVIDERS=mimo                # low/medium → high
pub(super) fn apply_effort_overrides(provider_type: &str, current: Option<String>) -> Option<String> {
    // 1. floor 覆盖（先看，因为它对 Some/None 都生效）
    if provider_in_env_list(provider_type, "AGENTGATE_EFFORT_FLOOR_PROVIDERS") {
        let needs_lift = current
            .as_deref()
            .map(|e| effort_rank(e) < effort_rank("high"))
            .unwrap_or(true); // None 也升
        if needs_lift {
            return Some("high".to_string());
        }
    }
    // 2. fill 兜底（仅 None 时）
    if current.is_none()
        && provider_in_env_list(provider_type, "AGENTGATE_FORCE_HIGH_EFFORT_PROVIDERS")
    {
        return Some("high".to_string());
    }
    current
}

fn provider_in_env_list(provider_type: &str, env_var: &str) -> bool {
    std::env::var(env_var)
        .ok()
        .map(|raw| {
            raw.split(',')
                .any(|s| s.trim().eq_ignore_ascii_case(provider_type))
        })
        .unwrap_or(false)
}

fn effort_rank(effort: &str) -> u8 {
    match effort.to_ascii_lowercase().as_str() {
        "minimal" | "low" => 0,
        "medium" => 1,
        "high" => 2,
        "max" | "xhigh" | "highest" => 3,
        _ => 0,
    }
}
