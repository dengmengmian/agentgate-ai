import { describe, it, expect } from "vitest";
import {
  buildGreetingInstruction,
  buildStatsInstruction,
  buildErrorInstruction,
  buildAmbientInstruction,
  buildMemoryExtractionInstruction,
  parseExtractedMemory,
} from "./petGenerate";

describe("buildGreetingInstruction", () => {
  it("带上时间段、网关状态、语言约束", () => {
    const zh = buildGreetingInstruction("zh", 2, "running");
    expect(zh).toContain("running");
    expect(zh).toContain("凌晨");
    expect(zh).toContain("中文");
    const en = buildGreetingInstruction("en", 14, "stopped");
    expect(en).toContain("English");
  });
});

describe("buildStatsInstruction", () => {
  it("含请求数,有花费/错误时带上", () => {
    const s = buildStatsInstruction("zh", {
      requests: 230,
      errors: 3,
      cost: 1.2345,
    });
    expect(s).toContain("230");
    expect(s).toContain("3个错误");
    expect(s).toContain("$1.23");
  });
  it("零花费零错误时不硬凑", () => {
    const s = buildStatsInstruction("zh", { requests: 5 });
    expect(s).toContain("5");
    expect(s).not.toContain("错误");
    expect(s).not.toContain("$");
  });
});

describe("buildErrorInstruction", () => {
  it("含供应商与截断的原始错误", () => {
    const s = buildErrorInstruction("zh", "Kimi", "HTTP 502 bad gateway");
    expect(s).toContain("Kimi");
    expect(s).toContain("502");
  });
  it("超长错误被截断", () => {
    const s = buildErrorInstruction("zh", "", "x".repeat(500));
    expect(s.length).toBeLessThan(400);
  });
});

describe("buildAmbientInstruction", () => {
  it("结合情境事实", () => {
    const s = buildAmbientInstruction("zh", {
      hour: 2,
      gwState: "active",
      today: { requests: 100, errors: 2 },
      topic: "宠物功能",
    });
    expect(s).toContain("凌晨");
    expect(s).toContain("100");
    expect(s).toContain("宠物功能");
  });
});

describe("parseExtractedMemory", () => {
  it("NONE / 空 → 空对象", () => {
    expect(parseExtractedMemory("NONE")).toEqual({});
    expect(parseExtractedMemory("  none  ")).toEqual({});
    expect(parseExtractedMemory("")).toEqual({});
  });
  it("合法 JSON 提取", () => {
    expect(parseExtractedMemory('{"job":"程序员","likes":"咖啡"}')).toEqual({
      job: "程序员",
      likes: "咖啡",
    });
  });
  it("夹带解释文字也能抠出 JSON", () => {
    const raw = '好的,记住了:{"pet":"养了只猫"} 就这些';
    expect(parseExtractedMemory(raw)).toEqual({ pet: "养了只猫" });
  });
  it("过滤内部 _ 前缀、非法 key、超长 value", () => {
    const raw = JSON.stringify({
      _evil: "x",
      "bad key": "y",
      job: "z".repeat(50),
      ok: "good",
    });
    expect(parseExtractedMemory(raw)).toEqual({ ok: "good" });
  });
  it("最多保留 3 条", () => {
    const raw = JSON.stringify({ a: "1", b: "2", c: "3", d: "4" });
    expect(Object.keys(parseExtractedMemory(raw)).length).toBe(3);
  });
  it("非法输入健壮兜底", () => {
    expect(parseExtractedMemory("not json at all")).toEqual({});
    expect(parseExtractedMemory("{broken")).toEqual({});
  });
});

describe("buildMemoryExtractionInstruction", () => {
  it("含对话内容,要求输出 JSON 或 NONE", () => {
    const s = buildMemoryExtractionInstruction("zh", "我是个后端", "你好");
    expect(s).toContain("后端");
    expect(s).toContain("NONE");
    expect(s).toContain("JSON");
  });
});
