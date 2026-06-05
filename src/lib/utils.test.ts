import { describe, it, expect } from "vitest";
import { cn, formatTimestamp, formatDate, formatLatency, formatOptionalLatency, formatUptime } from "./utils";

describe("cn", () => {
  it("merges class names", () => {
    expect(cn("a", "b")).toBe("a b");
  });

  it("handles conditional classes", () => {
    expect(cn("a", false && "b", "c")).toBe("a c");
  });

  it("handles undefined and null", () => {
    expect(cn("a", undefined, null, "b")).toBe("a b");
  });
});

describe("formatTimestamp", () => {
  it("formats ISO string to MM-DD HH:MM:SS", () => {
    const result = formatTimestamp("2024-01-15T08:30:45.000Z");
    expect(result).toMatch(/01-15\s+\d{2}:\d{2}:\d{2}/);
  });

  it("supports zh locale", () => {
    const result = formatTimestamp("2024-01-15T08:30:45.000Z", "zh");
    expect(result).toMatch(/01-15\s+\d{2}:\d{2}:\d{2}/);
  });
});

describe("formatDate", () => {
  it("formats ISO string to locale date", () => {
    const result = formatDate("2024-01-15T08:30:45.000Z");
    expect(result).toContain("2024");
  });

  it("supports zh locale", () => {
    const result = formatDate("2024-01-15T08:30:45.000Z", "zh");
    // zh-CN format includes year numeric
    expect(result).toContain("2024");
  });
});

describe("formatLatency", () => {
  it("shows ms for values under 1000", () => {
    expect(formatLatency(500)).toBe("500ms");
    expect(formatLatency(0)).toBe("0ms");
    expect(formatLatency(999)).toBe("999ms");
  });

  it("shows seconds for values >= 1000", () => {
    expect(formatLatency(1000)).toBe("1.0s");
    expect(formatLatency(1500)).toBe("1.5s");
    expect(formatLatency(20000)).toBe("20.0s");
  });
});

describe("formatOptionalLatency", () => {
  it("shows dash for missing or unrecorded latency", () => {
    expect(formatOptionalLatency(null)).toBe("—");
    expect(formatOptionalLatency(0)).toBe("—");
  });

  it("formats positive latency", () => {
    expect(formatOptionalLatency(500)).toBe("500ms");
    expect(formatOptionalLatency(1500)).toBe("1.5s");
  });
});

describe("formatUptime", () => {
  it("shows hours and minutes when >= 1 hour", () => {
    expect(formatUptime(3600)).toBe("1h 0m");
    expect(formatUptime(3660)).toBe("1h 1m");
    expect(formatUptime(7200)).toBe("2h 0m");
  });

  it("shows only minutes when < 1 hour", () => {
    expect(formatUptime(0)).toBe("0m");
    expect(formatUptime(60)).toBe("1m");
    expect(formatUptime(3540)).toBe("59m");
  });
});
