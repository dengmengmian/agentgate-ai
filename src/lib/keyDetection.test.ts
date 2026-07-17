import { describe, it, expect } from "vitest";
import { detectProvider, detectProviderType } from "./keyDetection";

describe("detectProviderType", () => {
  it("returns null for empty input", () => {
    expect(detectProviderType("")).toBeNull();
    expect(detectProviderType("   ")).toBeNull();
  });

  it("recognizes Anthropic by sk-ant- prefix", () => {
    expect(detectProviderType("sk-ant-abc123")).toBe("anthropic");
    expect(detectProviderType("sk-ant-api03-XXXXX")).toBe("anthropic");
  });

  it("recognizes GitHub Copilot by gho_/ghu_ prefix", () => {
    expect(detectProviderType("gho_16C7e42F292c6912E7710c838347Ae178")).toBe(
      "copilot"
    );
    expect(detectProviderType("ghu_16C7e42F292c6912E7710c838347Ae178")).toBe(
      "copilot"
    );
  });

  it("recognizes OpenRouter by sk-or- prefix", () => {
    expect(detectProviderType("sk-or-v1-abcdef")).toBe("openrouter");
  });

  it("recognizes Groq by gsk_ prefix", () => {
    expect(detectProviderType("gsk_abc123")).toBe("groq");
  });

  it("recognizes xAI by xai- prefix", () => {
    expect(detectProviderType("xai-abc123")).toBe("xai");
  });

  it("recognizes Perplexity by pplx- prefix", () => {
    expect(detectProviderType("pplx-abc123")).toBe("perplexity");
  });

  it("recognizes MiMo Token Plan by tp- prefix", () => {
    expect(detectProviderType("tp-abc123def")).toBe("mimo");
  });

  it("recognizes DeepSeek by deepseek- prefix (legacy)", () => {
    expect(detectProviderType("deepseek-abc123")).toBe("deepseek");
  });

  it("recognizes DeepSeek by exact sk- + 32 hex shape", () => {
    // Real DeepSeek key shape.
    expect(detectProviderType("sk-abcdef0123456789abcdef0123456789")).toBe(
      "deepseek"
    );
    expect(detectProviderType("sk-0123456789abcdef0123456789abcdef")).toBe(
      "deepseek"
    );
  });

  it("does NOT mistake long OpenAI keys for DeepSeek", () => {
    // OpenAI legacy key: 48 mixed-case chars after sk-. Should NOT match
    // DeepSeek's 32-hex anchor.
    expect(
      detectProviderType("sk-A1B2C3D4E5F6abcdef0123456789ABCDEFabcdef01234567")
    ).not.toBe("deepseek");
  });

  it("recognizes Kimi-style 48-char base64 keys", () => {
    // Moonshot/Kimi pattern: 48 alphanumerics.
    expect(
      detectProviderType("sk-AbCdEf0123456789AbCdEf0123456789AbCdEf0123456789")
    ).toBe("kimi");
  });

  it("recognizes Kimi Code keys by sk-kimi- prefix", () => {
    // Membership console keys look like sk-kimi-… (hyphen after kimi).
    // Synthetic fixtures only — never paste live keys into the suite.
    expect(detectProviderType("sk-kimi-test-code-key")).toBe("kimi");
    expect(detectProviderType("sk-kimi-abc123")).toBe("kimi");
  });

  it("recognizes OpenAI structured prefixes", () => {
    expect(detectProviderType("sk-proj-XXXXXXX")).toBe("openai");
    expect(detectProviderType("sk-svcacct-XXXXXX")).toBe("openai");
    expect(detectProviderType("sk-admin-XXXXXX")).toBe("openai");
  });

  it("falls back to OpenAI for unrecognized sk- shapes", () => {
    // Some random sk- key that doesn't match the DeepSeek hex shape nor
    // any explicit prefix → OpenAI is the safest default.
    expect(detectProviderType("sk-randomBlobThatDoesntMatchAnything")).toBe(
      "openai"
    );
  });

  it("returns null for non-sk keys without known prefix", () => {
    expect(detectProviderType("some-random-string")).toBeNull();
    expect(detectProviderType("Bearer xyz")).toBeNull();
  });

  it("trims surrounding whitespace before matching", () => {
    expect(detectProviderType("  sk-ant-abc  ")).toBe("anthropic");
    expect(
      detectProviderType("\n sk-abcdef0123456789abcdef0123456789 \n")
    ).toBe("deepseek");
  });
});

describe("detectProvider", () => {
  it("returns label alongside type", () => {
    expect(detectProvider("sk-ant-abc")).toEqual({
      type: "anthropic",
      label: "Anthropic",
    });
    expect(detectProvider("sk-abcdef0123456789abcdef0123456789")).toEqual({
      type: "deepseek",
      label: "DeepSeek",
    });
  });

  it("returns null when type is null", () => {
    expect(detectProvider("")).toBeNull();
  });

  it("labels MiMo with Chinese disambiguator", () => {
    expect(detectProvider("tp-abc123")?.label).toBe("MiMo (小米)");
  });
});
