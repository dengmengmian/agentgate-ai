import { describe, expect, it } from "vitest";
import { PROVIDER_TYPES } from "@/types/provider";
import {
  firstApiKey,
  isMimoProviderType,
  isKnownMimoEndpointUrl,
  getMimoEndpointsForKey,
  PROVIDER_PRESETS,
  resolveKnownProviderEndpoints,
  resolveProviderPresetForKey,
} from "./providerPresets";

describe("MiMo provider endpoints", () => {
  it("uses token-plan hosts for tp keys", () => {
    const preset = resolveProviderPresetForKey("mimo", "tp-xxxxx");
    expect(preset?.baseUrl).toBe("https://token-plan-cn.xiaomimimo.com/v1");
    expect(preset?.anthropicBaseUrl).toBe(
      "https://token-plan-cn.xiaomimimo.com/anthropic"
    );
    expect(preset?.protocols).toContain("anthropic_messages");
  });

  it("uses regular hosts for MiMo sk keys", () => {
    const endpoints = getMimoEndpointsForKey("sk-xxxxx");
    expect(endpoints?.baseUrl).toBe("https://api.xiaomimimo.com/v1");
    expect(endpoints?.anthropicBaseUrl).toBe(
      "https://api.xiaomimimo.com/anthropic"
    );
  });

  it("preserves the token-plan region when the user pasted a subscription URL", () => {
    const endpoints = resolveKnownProviderEndpoints(
      "mimo",
      "tp-xxxxx",
      "https://token-plan-sgp.xiaomimimo.com/v1"
    );
    expect(endpoints?.baseUrl).toBe("https://token-plan-sgp.xiaomimimo.com/v1");
    expect(endpoints?.anthropicBaseUrl).toBe(
      "https://token-plan-sgp.xiaomimimo.com/anthropic"
    );
  });

  it("derives the token-plan region from the Anthropic URL too", () => {
    const endpoints = resolveKnownProviderEndpoints(
      "mimo",
      "tp-xxxxx",
      "https://token-plan-cn.xiaomimimo.com/v1",
      "https://token-plan-ams.xiaomimimo.com/anthropic"
    );
    expect(endpoints?.baseUrl).toBe("https://token-plan-ams.xiaomimimo.com/v1");
    expect(endpoints?.anthropicBaseUrl).toBe(
      "https://token-plan-ams.xiaomimimo.com/anthropic"
    );
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

  it("exposes all catalog providers as quick setup presets", () => {
    expect(Object.keys(PROVIDER_PRESETS)).toHaveLength(27);
    expect(PROVIDER_PRESETS.openai.responsesBaseUrl).toBe(
      "https://api.openai.com"
    );
    expect(PROVIDER_PRESETS.kimi.extraHeaders).toContain("KimiCLI");
  });

  it("keeps provider type options aligned with catalog presets", () => {
    const optionTypes = PROVIDER_TYPES.map((type) => type.value).sort();
    const presetTypes = Object.keys(PROVIDER_PRESETS).sort();
    expect(optionTypes).toEqual(presetTypes);
  });
});

describe("isMimoProviderType", () => {
  it("matches mimo, xiaomi, and case-insensitive variants", () => {
    expect(isMimoProviderType("mimo")).toBe(true);
    expect(isMimoProviderType("xiaomi")).toBe(true);
    expect(isMimoProviderType("MIMO")).toBe(true);
    expect(isMimoProviderType("  mimo  ")).toBe(true);
    expect(isMimoProviderType("something-mimo-extra")).toBe(true);
  });

  it("does not match unrelated types", () => {
    expect(isMimoProviderType("openai")).toBe(false);
    expect(isMimoProviderType("deepseek")).toBe(false);
    expect(isMimoProviderType("")).toBe(false);
  });
});

describe("firstApiKey", () => {
  it("returns empty string for null/undefined/empty", () => {
    expect(firstApiKey(null)).toBe("");
    expect(firstApiKey(undefined)).toBe("");
    expect(firstApiKey("")).toBe("");
    expect(firstApiKey("   ")).toBe("");
  });

  it("returns plain key as-is", () => {
    expect(firstApiKey("sk-abc123")).toBe("sk-abc123");
  });

  it("trims whitespace from plain key", () => {
    expect(firstApiKey("  sk-abc123  ")).toBe("sk-abc123");
  });

  it("parses JSON array and returns first non-empty string", () => {
    expect(firstApiKey('["key-a","key-b"]')).toBe("key-a");
  });

  it("skips empty strings in JSON array", () => {
    expect(firstApiKey('["","key-b"]')).toBe("key-b");
  });

  it("returns raw value when JSON parse fails", () => {
    expect(firstApiKey("[invalid json")).toBe("[invalid json");
  });

  it("returns raw value when JSON is not an array", () => {
    expect(firstApiKey('{"a":1}')).toBe('{"a":1}');
  });
});

describe("isKnownMimoEndpointUrl", () => {
  it("returns true for known MiMo endpoints", () => {
    expect(isKnownMimoEndpointUrl("https://api.xiaomimimo.com/v1")).toBe(true);
  });

  it("returns false for unknown URLs", () => {
    expect(isKnownMimoEndpointUrl("https://api.openai.com/v1")).toBe(false);
  });

  it("returns false for null/undefined", () => {
    expect(isKnownMimoEndpointUrl(null)).toBe(false);
    expect(isKnownMimoEndpointUrl(undefined)).toBe(false);
  });
});
