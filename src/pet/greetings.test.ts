import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { getGreeting } from "./greetings";

describe("getGreeting", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns a string for running state", () => {
    const result = getGreeting("running", "en");
    expect(typeof result).toBe("string");
    expect(result.length).toBeGreaterThan(0);
  });

  it("returns a string for stopped state", () => {
    const result = getGreeting("stopped", "en");
    expect(typeof result).toBe("string");
    expect(result.length).toBeGreaterThan(0);
  });

  it("returns a string for active state", () => {
    const result = getGreeting("active", "en");
    expect(typeof result).toBe("string");
    expect(result.length).toBeGreaterThan(0);
  });

  it("returns a string for zh locale", () => {
    const result = getGreeting("running", "zh");
    expect(typeof result).toBe("string");
    expect(result.length).toBeGreaterThan(0);
  });

  it("uses time-based greeting in morning", () => {
    const morning = new Date("2024-01-01T08:00:00");
    vi.setSystemTime(morning);
    vi.spyOn(Math, "random").mockReturnValue(0.1);
    const result = getGreeting("running", "en");
    expect(result).toMatch(/morning|Rise|Ready/i);
    vi.restoreAllMocks();
  });

  it("uses state-based greeting when roll is mid-range", () => {
    vi.spyOn(Math, "random").mockReturnValue(0.5);
    const result = getGreeting("stopped", "en");
    expect(result).toMatch(/Zzz|Idle|Bored/i);
    vi.restoreAllMocks();
  });

  it("uses fun greeting when roll is high", () => {
    vi.spyOn(Math, "random").mockReturnValue(0.8);
    const result = getGreeting("running", "en");
    expect(result).toMatch(/Bugs|awesome|coffee|Ship|commit|LGTM/i);
    vi.restoreAllMocks();
  });
});
