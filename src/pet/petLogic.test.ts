import { describe, it, expect } from "vitest";
import {
  activityTier,
  pokeMood,
  getDateBadge,
  extractTopic,
  isOverBudget,
  topicGreeting,
} from "./petLogic";

describe("activityTier", () => {
  it("0-1 个并发是一档", () => {
    expect(activityTier(0)).toBe(1);
    expect(activityTier(1)).toBe(1);
  });
  it("2 个并发是二档", () => {
    expect(activityTier(2)).toBe(2);
  });
  it("3 个及以上是三档(冒汗)", () => {
    expect(activityTier(3)).toBe(3);
    expect(activityTier(10)).toBe(3);
  });
});

describe("pokeMood", () => {
  it("戳 1-2 下正常", () => {
    expect(pokeMood(1)).toBe("normal");
    expect(pokeMood(2)).toBe("normal");
  });
  it("戳 3-4 下生气", () => {
    expect(pokeMood(3)).toBe("angry");
    expect(pokeMood(4)).toBe("angry");
  });
  it("戳 5 下以上背过身", () => {
    expect(pokeMood(5)).toBe("sulk");
    expect(pokeMood(9)).toBe("sulk");
  });
});

describe("getDateBadge", () => {
  it("圣诞节给圣诞树", () => {
    expect(getDateBadge(new Date(2026, 11, 25, 10))).toBe("🎄");
    expect(getDateBadge(new Date(2026, 11, 24, 10))).toBe("🎄");
  });
  it("万圣节给南瓜", () => {
    expect(getDateBadge(new Date(2026, 9, 31, 10))).toBe("🎃");
  });
  it("元旦给彩带", () => {
    expect(getDateBadge(new Date(2027, 0, 1, 10))).toBe("🎉");
  });
  it("春节给红包(2026-02-17)", () => {
    expect(getDateBadge(new Date(2026, 1, 17, 10))).toBe("🧧");
  });
  it("深夜(0-5 点)给月亮,优先级低于节日", () => {
    // 2026-07-08 是周三
    expect(getDateBadge(new Date(2026, 6, 8, 2))).toBe("🌙");
    expect(getDateBadge(new Date(2026, 11, 25, 2))).toBe("🎄");
  });
  it("周五白天给闪光", () => {
    // 2026-07-10 是周五
    expect(getDateBadge(new Date(2026, 6, 10, 14))).toBe("✨");
  });
  it("普通日子没有彩蛋", () => {
    // 2026-07-08 周三下午
    expect(getDateBadge(new Date(2026, 6, 8, 14))).toBeNull();
  });
});

describe("extractTopic", () => {
  it("中文「在做/在弄/在写」句式", () => {
    expect(extractTopic("我最近在弄一个宠物功能")).toBe("一个宠物功能");
    expect(extractTopic("在写发版脚本,好烦")).toBe("发版脚本");
    expect(extractTopic("我正在修网关的 bug")).toBe("网关的 bug");
  });
  it("英文 working on 句式", () => {
    expect(extractTopic("I'm working on the release pipeline")).toBe(
      "the release pipeline"
    );
  });
  it("闲聊提不出话题", () => {
    expect(extractTopic("你好呀")).toBeNull();
    expect(extractTopic("今天天气不错")).toBeNull();
  });
  it("话题截断到 20 字,尾部标点去掉", () => {
    const long = "在做" + "字".repeat(40);
    const t = extractTopic(long);
    expect(t).not.toBeNull();
    expect(t!.length).toBeLessThanOrEqual(20);
    expect(extractTopic("我在弄发版自动化。")).toBe("发版自动化");
  });
});

describe("isOverBudget", () => {
  it("配置了阈值且启用时按配置判断", () => {
    expect(isOverBudget(6, { enabled: true, threshold: 5 })).toBe(true);
    expect(isOverBudget(4, { enabled: true, threshold: 5 })).toBe(false);
  });
  it("未配置/未启用时用默认 $10", () => {
    expect(isOverBudget(12, null)).toBe(true);
    expect(isOverBudget(9.5, undefined)).toBe(false);
    expect(isOverBudget(12, { enabled: false, threshold: 5 })).toBe(true);
  });
  it("零花费永远不吃撑", () => {
    expect(isOverBudget(0, { enabled: true, threshold: 0 })).toBe(false);
  });
});

describe("topicGreeting", () => {
  it("中文问候带上话题", () => {
    const g = topicGreeting("宠物功能", "zh");
    expect(g).toContain("宠物功能");
  });
  it("英文问候带上话题", () => {
    const g = topicGreeting("the release pipeline", "en");
    expect(g).toContain("the release pipeline");
  });
});
