//! Failover 候选排序:把"主 provider 优先 + 失败转移候选 + vision 过滤 + 会话亲和"
//! 这套排序逻辑收敛到单一来源,供各协议 handler 共用,避免在 routes.rs 里各写一份、
//! 改一处漏一处(vision 过滤此前只在 /v1/responses 实现就是这么漏的)。

use crate::gateway::provider_selector::ProviderCandidate;

/// 构建实际尝试顺序:
/// 1. 主 provider 优先(带图请求且主 provider 明确不支持 vision 时跳过);
/// 2. failover 模式下追加其余非 cooldown 候选(同样按 vision 过滤);
/// 3. 若全部被 vision 过滤掉,退回不带 vision 过滤的原始顺序(vision 是提示不是硬约束);
/// 4. 会话亲和:上一轮命中上游 prompt 缓存的 provider 提到队首(命中且不在 cooldown 时)。
pub fn build_attempt_order<'a>(
    candidates: &'a [ProviderCandidate],
    primary_id: &str,
    is_failover: bool,
    request_has_images: bool,
    session_id: Option<&str>,
) -> Vec<&'a ProviderCandidate> {
    let mut attempt_order: Vec<&ProviderCandidate> = Vec::new();

    // 主 provider
    if let Some(primary) = candidates.iter().find(|c| c.provider_id == primary_id) {
        if !request_has_images || primary.supports_vision != Some(false) {
            attempt_order.push(primary);
        }
    }
    // 其余候选(failover)
    if is_failover {
        for c in candidates {
            if c.provider_id != primary_id && !c.in_cooldown {
                if request_has_images && c.supports_vision == Some(false) {
                    continue; // 显式不支持 vision 的 provider 跳过
                }
                attempt_order.push(c);
            }
        }
    }
    // 全部被 vision 过滤掉时,退回原始顺序(不带 vision 过滤)
    if attempt_order.is_empty() {
        if let Some(primary) = candidates.iter().find(|c| c.provider_id == primary_id) {
            attempt_order.push(primary);
        }
        if is_failover {
            for c in candidates {
                if c.provider_id != primary_id && !c.in_cooldown {
                    attempt_order.push(c);
                }
            }
        }
    }

    // 会话亲和:把上一轮命中缓存的 provider 提到队首。亲和是提示,
    // 候选集里没有或在 cooldown 时忽略。
    if let Some(sid) = session_id {
        if let Some(entry) = crate::gateway::session_affinity::lookup(sid) {
            if let Some(pos) = attempt_order
                .iter()
                .position(|c| c.provider_id == entry.provider_id && !c.in_cooldown)
            {
                if pos > 0 {
                    let preferred = attempt_order.remove(pos);
                    attempt_order.insert(0, preferred);
                }
            }
        }
    }

    attempt_order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, in_cooldown: bool, vision: Option<bool>) -> ProviderCandidate {
        ProviderCandidate {
            provider_id: id.to_string(),
            provider_name: id.to_string(),
            priority: 0,
            model: "m".to_string(),
            routing_conditions: None,
            in_cooldown,
            supports_vision: vision,
            cooldown_seconds: 0,
            failover_on_status_codes: vec![],
            failover_on_error_keywords: vec![],
        }
    }

    fn ids(order: &[&ProviderCandidate]) -> Vec<String> {
        order.iter().map(|c| c.provider_id.clone()).collect()
    }

    #[test]
    fn primary_first_then_failover_candidates() {
        let cands = vec![cand("a", false, None), cand("b", false, None)];
        let order = build_attempt_order(&cands, "a", true, false, None);
        assert_eq!(ids(&order), vec!["a", "b"]);
    }

    #[test]
    fn no_failover_keeps_primary_only() {
        let cands = vec![cand("a", false, None), cand("b", false, None)];
        let order = build_attempt_order(&cands, "a", false, false, None);
        assert_eq!(ids(&order), vec!["a"]);
    }

    #[test]
    fn cooldown_candidates_skipped_in_failover() {
        let cands = vec![cand("a", false, None), cand("b", true, None)];
        let order = build_attempt_order(&cands, "a", true, false, None);
        assert_eq!(ids(&order), vec!["a"]);
    }

    #[test]
    fn images_skip_non_vision_providers() {
        let cands = vec![cand("a", false, Some(false)), cand("b", false, Some(true))];
        let order = build_attempt_order(&cands, "a", true, true, None);
        assert_eq!(ids(&order), vec!["b"]);
    }

    #[test]
    fn images_fall_back_to_original_order_when_all_filtered() {
        // 全部 provider 都不支持 vision → 退回原始顺序而非空
        let cands = vec![cand("a", false, Some(false)), cand("b", false, Some(false))];
        let order = build_attempt_order(&cands, "a", true, true, None);
        assert_eq!(ids(&order), vec!["a", "b"]);
    }

    #[test]
    fn unknown_vision_capability_not_filtered() {
        // supports_vision = None(未知)不应被过滤
        let cands = vec![cand("a", false, None), cand("b", false, None)];
        let order = build_attempt_order(&cands, "a", true, true, None);
        assert_eq!(ids(&order), vec!["a", "b"]);
    }
}
