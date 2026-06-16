import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useDebouncedValue } from "./useDebouncedValue";

describe("useDebouncedValue", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("初始值立即可用", () => {
    const { result } = renderHook(() => useDebouncedValue("a", 300));
    expect(result.current).toBe("a");
  });

  it("变化后延迟期内保持旧值，到期才更新", () => {
    const { result, rerender } = renderHook(
      ({ v }) => useDebouncedValue(v, 300),
      { initialProps: { v: "a" } }
    );
    rerender({ v: "ab" });
    expect(result.current).toBe("a");
    act(() => {
      vi.advanceTimersByTime(299);
    });
    expect(result.current).toBe("a");
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(result.current).toBe("ab");
  });

  it("连续变化只取最后一次", () => {
    const { result, rerender } = renderHook(
      ({ v }) => useDebouncedValue(v, 300),
      { initialProps: { v: "c" } }
    );
    rerender({ v: "cl" });
    act(() => {
      vi.advanceTimersByTime(200);
    });
    rerender({ v: "claude" });
    act(() => {
      vi.advanceTimersByTime(200);
    });
    // 距最后一次变化只过了 200ms，不应更新
    expect(result.current).toBe("c");
    act(() => {
      vi.advanceTimersByTime(100);
    });
    expect(result.current).toBe("claude");
  });
});
