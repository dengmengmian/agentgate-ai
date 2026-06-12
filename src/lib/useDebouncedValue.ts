import { useEffect, useState } from "react";

/// 防抖值：value 连续变化时，只有停止变化 delayMs 后才更新返回值。
/// 用于把搜索框输入和真正触发查询的值解耦——每个键击都查一次数据库太浪费。
export function useDebouncedValue<T>(value: T, delayMs = 300): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);
  return debounced;
}
