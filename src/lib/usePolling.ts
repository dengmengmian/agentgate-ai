import { useEffect } from "react";

/// 周期性调用 cb——用于让 Providers / Routes / Tools 等"非 dashboard"页面也能
/// 反映后台变化（如 runtime_status 因请求失败被标记 cooldown）。
///
/// 默认 10s 周期——这些页面变化频率远低于 dashboard（5s 太快没必要）。
/// 配合 window focus 监听：用户从 IDE 切回 AgentGate 时立刻刷新一次，
/// 避免依赖周期看到陈旧数据。
///
/// cb 应该是 useCallback 化的稳定引用，否则 useEffect 会反复重建定时器。
export function usePolling(cb: () => void, intervalMs = 10_000) {
  useEffect(() => {
    let mounted = true;
    const onFocus = () => { if (mounted) cb(); };
    window.addEventListener("focus", onFocus);
    const timer = setInterval(() => { if (mounted) cb(); }, intervalMs);
    return () => {
      mounted = false;
      window.removeEventListener("focus", onFocus);
      clearInterval(timer);
    };
  }, [cb, intervalMs]);
}
