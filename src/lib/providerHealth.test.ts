import { describe, expect, it } from "vitest";
import { classifyProviderErrorStatus } from "./providerHealth";

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
