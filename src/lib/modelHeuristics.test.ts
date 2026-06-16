import { describe, expect, it } from "vitest";
import {
  normalizeModelsForProvider,
  pickModelsForProvider,
} from "./modelHeuristics";

describe("DeepSeek model heuristics", () => {
  it("keeps only v4 models for deepseek", () => {
    expect(
      normalizeModelsForProvider("deepseek", [
        "deepseek-chat",
        "deepseek-v4-flash",
        "deepseek-reasoner",
        "deepseek-v4-pro",
      ])
    ).toEqual(["deepseek-v4-flash", "deepseek-v4-pro"]);
  });

  it("defaults deepseek to flash and reasoning to pro", () => {
    expect(
      pickModelsForProvider("deepseek", [
        "deepseek-chat",
        "deepseek-v4-flash",
        "deepseek-reasoner",
        "deepseek-v4-pro",
      ])
    ).toEqual({ default: "deepseek-v4-flash", reasoning: "deepseek-v4-pro" });
  });
});
