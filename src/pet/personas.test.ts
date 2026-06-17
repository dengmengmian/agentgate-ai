import { describe, it, expect, vi, afterEach } from "vitest";
import { buildSystemPrompt, pickPokeReaction } from "./personas";
import type { PetType } from "@/types/pet";

const PET_TYPES: PetType[] = [
  "robot",
  "pixel-cat",
  "slime",
  "fox",
  "octopus",
  "ghost",
  "ox",
  "soldier",
  "coder",
];

describe("buildSystemPrompt", () => {
  it("includes base prompt and locale-specific persona for each pet type", () => {
    for (const type of PET_TYPES) {
      const en = buildSystemPrompt(type, "en", "");
      expect(en).toContain("cute desktop pet assistant");
      expect(en.length).toBeGreaterThan(100);

      const zh = buildSystemPrompt(type, "zh", "");
      expect(zh).toContain("你是");
      expect(zh.length).toBeGreaterThan(50);
    }
  });

  it("includes memory when provided", () => {
    const prompt = buildSystemPrompt("robot", "en", "name: Kimi");
    expect(prompt).toContain("You remember about the user: name: Kimi");
  });

  it("does not include memory line when empty", () => {
    const prompt = buildSystemPrompt("robot", "en", "");
    expect(prompt).not.toContain("You remember about the user");
  });
});

describe("pickPokeReaction", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("returns a non-empty reaction for each pet type", () => {
    for (const type of PET_TYPES) {
      const reaction = pickPokeReaction(type, "en");
      expect(typeof reaction).toBe("string");
      expect(reaction.length).toBeGreaterThan(0);
    }
  });

  it("uses locale-specific reactions", () => {
    vi.spyOn(Math, "random").mockReturnValue(0);
    const en = pickPokeReaction("robot", "en");
    const zh = pickPokeReaction("robot", "zh");
    expect(en).not.toBe(zh);
  });
});
