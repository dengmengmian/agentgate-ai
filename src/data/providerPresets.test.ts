import { describe, expect, it } from "vitest";
import {
  firstApiKey,
  getMimoEndpointsForKey,
  resolveKnownProviderEndpoints,
  resolveProviderPresetForKey,
} from "./providerPresets";

describe("MiMo provider endpoints", () => {
  it("uses token-plan hosts for tp keys", () => {
    const preset = resolveProviderPresetForKey("mimo", "tp-xxxxx");
    expect(preset?.baseUrl).toBe("https://token-plan-cn.xiaomimimo.com/v1");
    expect(preset?.anthropicBaseUrl).toBe("https://token-plan-cn.xiaomimimo.com/anthropic");
    expect(preset?.protocols).toContain("anthropic_messages");
  });

  it("uses regular hosts for MiMo sk keys", () => {
    const endpoints = getMimoEndpointsForKey("sk-xxxxx");
    expect(endpoints?.baseUrl).toBe("https://api.xiaomimimo.com/v1");
    expect(endpoints?.anthropicBaseUrl).toBe("https://api.xiaomimimo.com/anthropic");
  });

  it("preserves the token-plan region when the user pasted a subscription URL", () => {
    const endpoints = resolveKnownProviderEndpoints(
      "mimo",
      "tp-xxxxx",
      "https://token-plan-sgp.xiaomimimo.com/v1",
    );
    expect(endpoints?.baseUrl).toBe("https://token-plan-sgp.xiaomimimo.com/v1");
    expect(endpoints?.anthropicBaseUrl).toBe("https://token-plan-sgp.xiaomimimo.com/anthropic");
  });

  it("derives the token-plan region from the Anthropic URL too", () => {
    const endpoints = resolveKnownProviderEndpoints(
      "mimo",
      "tp-xxxxx",
      "https://token-plan-cn.xiaomimimo.com/v1",
      "https://token-plan-ams.xiaomimimo.com/anthropic",
    );
    expect(endpoints?.baseUrl).toBe("https://token-plan-ams.xiaomimimo.com/v1");
    expect(endpoints?.anthropicBaseUrl).toBe("https://token-plan-ams.xiaomimimo.com/anthropic");
  });

  it("uses the first key in a JSON key list", () => {
    expect(firstApiKey('["tp-first","sk-second"]')).toBe("tp-first");
  });

  it("does not change non-MiMo presets", () => {
    const preset = resolveProviderPresetForKey("deepseek", "tp-xxxxx");
    expect(preset?.baseUrl).toBe("https://api.deepseek.com");
  });

  it("enables Claude Code native protocol for DeepSeek", () => {
    const preset = resolveProviderPresetForKey("deepseek", "deepseek-xxxxx");
    expect(preset?.protocols).toContain("anthropic_messages");
    expect(preset?.defaultModel).toBe("deepseek-v4-flash");
    expect(preset?.reasoningModel).toBe("deepseek-v4-pro");
    expect(preset?.anthropicBaseUrl).toBe("https://api.deepseek.com/anthropic");
  });
});
