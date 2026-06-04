import { describe, expect, it } from "vitest";
import { classifyProviderErrorStatus, getProviderCooldownSummary } from "./providerHealth";

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
