import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { usePolling } from "./usePolling";

function setDocumentHidden(hidden: boolean) {
  Object.defineProperty(document, "hidden", {
    configurable: true,
    get: () => hidden,
  });
}

describe("usePolling", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    setDocumentHidden(false);
  });
  afterEach(() => {
    vi.useRealTimers();
    setDocumentHidden(false);
  });

  it("可见时按周期触发 cb", () => {
    const cb = vi.fn();
    renderHook(() => usePolling(cb, 1000));
    vi.advanceTimersByTime(3000);
    expect(cb).toHaveBeenCalledTimes(3);
  });

  it("document.hidden 时周期不触发", () => {
    const cb = vi.fn();
    renderHook(() => usePolling(cb, 1000));
    setDocumentHidden(true);
    vi.advanceTimersByTime(5000);
    expect(cb).not.toHaveBeenCalled();
  });

  it("从隐藏变回可见时立即刷新一次", () => {
    const cb = vi.fn();
    renderHook(() => usePolling(cb, 1000));
    setDocumentHidden(true);
    vi.advanceTimersByTime(5000);
    expect(cb).not.toHaveBeenCalled();
    setDocumentHidden(false);
    document.dispatchEvent(new Event("visibilitychange"));
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it("window focus 时立即刷新（既有行为）", () => {
    const cb = vi.fn();
    renderHook(() => usePolling(cb, 1000));
    window.dispatchEvent(new Event("focus"));
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it("卸载后不再触发", () => {
    const cb = vi.fn();
    const { unmount } = renderHook(() => usePolling(cb, 1000));
    unmount();
    vi.advanceTimersByTime(3000);
    document.dispatchEvent(new Event("visibilitychange"));
    window.dispatchEvent(new Event("focus"));
    expect(cb).not.toHaveBeenCalled();
  });
});
