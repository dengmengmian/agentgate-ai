import { describe, expect, it } from "vitest";
import { classifyProviderErrorStatus, getProviderCooldownSummary, getProviderSignalSummary } from "./providerHealth";

describe("classifyProviderErrorStatus", () => {
  it("classifies plain 429 as rate limited", () => {
    expect(classifyProviderErrorStatus({ status_code: 429, message: "Too Many Requests" })).toBe("rate_limited");
  });

  it("classifies quota and balance errors before generic rate limits", () => {
    expect(classifyProviderErrorStatus({ status_code: 429, message: "insufficient_quota" })).toBe("quota_exhausted");
    expect(classifyProviderErrorStatus({ status_code: 402, message: "insufficient_balance" })).toBe("insufficient_balance");
  });

  it("classifies auth failures from status or message", () => {
    expect(classifyProviderErrorStatus({ status_code: 401, message: "Unauthorized" })).toBe("auth_failed");
    expect(classifyProviderErrorStatus({ status_code: 400, message: "invalid api key" })).toBe("auth_failed");
  });
});

describe("getProviderCooldownSummary", () => {
  it("returns remaining seconds, recovery time, and reason for active cooldown", () => {
    expect(
      getProviderCooldownSummary(
        {
          cooldown_until: "2026-06-04T12:01:30.000Z",
          last_error: "rate limit exceeded",
          last_error_code: "RATE_LIMIT",
        },
        Date.parse("2026-06-04T12:00:00.000Z"),
      ),
    ).toEqual({
      remainingSeconds: 90,
      recoverAt: "2026-06-04T12:01:30.000Z",
      reason: "RATE_LIMIT",
    });
  });

  it("ignores expired or invalid cooldown values", () => {
    const now = Date.parse("2026-06-04T12:00:00.000Z");

    expect(getProviderCooldownSummary({ cooldown_until: "2026-06-04T11:59:59.000Z", last_error: "boom", last_error_code: null }, now)).toBeNull();
    expect(getProviderCooldownSummary({ cooldown_until: "not-a-date", last_error: "boom", last_error_code: null }, now)).toBeNull();
  });

  it("falls back to last_error when error code is missing", () => {
    expect(
      getProviderCooldownSummary(
        {
          cooldown_until: "2026-06-04T12:00:05.000Z",
          last_error: "upstream timeout",
          last_error_code: null,
        },
        Date.parse("2026-06-04T12:00:00.000Z"),
      )?.reason,
    ).toBe("upstream timeout");
  });
});

describe("getProviderSignalSummary", () => {
  it("normalizes runtime cooldown, probe, and traffic into one summary", () => {
    const summary = getProviderSignalSummary(
      {
        cooldown_until: "2026-06-04T12:01:30.000Z",
        last_error: "rate limit exceeded",
        last_error_code: "RATE_LIMIT",
        quota_exhausted: false,
        available: false,
        consecutive_failures: 2,
        last_probe_ok: true,
        last_probe_latency_ms: 320,
        last_probe_error: null,
      },
      {
        h24_total: 10,
        h24_success_rate: 90,
      },
      Date.parse("2026-06-04T12:00:00.000Z"),
    );

    expect(summary.runtime).toEqual({ status: "cooldown", variant: "warning" });
    expect(summary.probe).toEqual({ status: "ok", variant: "success", latencyMs: 320, error: null });
    expect(summary.traffic).toEqual({ status: "degraded", variant: "warning" });
  });

  it("prioritizes quota exhaustion before cooldown", () => {
    const summary = getProviderSignalSummary(
      {
        cooldown_until: "2026-06-04T12:01:30.000Z",
        last_error: "quota",
        last_error_code: "INSUFFICIENT_QUOTA",
        quota_exhausted: true,
        available: false,
        consecutive_failures: 1,
        last_probe_ok: false,
        last_probe_latency_ms: null,
        last_probe_error: "quota",
      },
      { h24_total: 0, h24_success_rate: 0 },
      Date.parse("2026-06-04T12:00:00.000Z"),
    );

    expect(summary.runtime).toEqual({ status: "quota_exhausted", variant: "error" });
    expect(summary.probe).toEqual({ status: "failed", variant: "error", latencyMs: null, error: "quota" });
    expect(summary.traffic).toEqual({ status: "no_traffic", variant: "muted" });
  });
});
