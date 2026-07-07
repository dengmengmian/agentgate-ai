import { describe, it, expect } from "vitest";
import {
  extractName,
  mapChatError,
  recentContext,
  type ChatMessage,
} from "./chatCore";
import { buildMemoryString } from "./petMemory";

describe("extractName", () => {
  it("中文自我介绍", () => {
    expect(extractName("我叫小明")).toBe("小明");
    expect(extractName("你好,我是麻凡,今天想聊聊")).toBe("麻凡,今天想聊聊");
    expect(extractName("叫我阿强就好")).toBe("阿强就好");
  });
  it("英文自我介绍", () => {
    expect(extractName("my name is Kimi")).toBe("Kimi");
    expect(extractName("I'm Alex")).toBe("Alex");
    expect(extractName("call me Sam")).toBe("Sam");
  });
  it("没有名字返回 null", () => {
    expect(extractName("今天天气不错")).toBeNull();
    expect(extractName("hello there")).toBeNull();
  });
});

describe("mapChatError", () => {
  it("已知错误码给友好提示", () => {
    expect(mapChatError({ code: "GATEWAY_NOT_RUNNING" }, "zh")).toContain(
      "启动网关"
    );
    expect(mapChatError({ code: "ACTIVE_PROVIDER_NOT_FOUND" }, "zh")).toContain(
      "供应商"
    );
    expect(mapChatError({ code: "PROVIDER_API_KEY_MISSING" }, "en")).toContain(
      "API key"
    );
    expect(mapChatError({ code: "GATEWAY_AUTH_INVALID" }, "zh")).toContain(
      "token"
    );
  });
  it("未知错误回落到截断的原始消息", () => {
    const out = mapChatError({ message: "boom".repeat(50) }, "zh");
    expect(out).toContain("调不通");
    expect(out.length).toBeLessThan(80);
  });
  it("完全没有信息也有兜底文案", () => {
    expect(mapChatError(null, "en")).toContain("failed");
  });
});

describe("buildMemoryString", () => {
  it("拼接可见字段,跳过下划线内部字段", () => {
    const s = buildMemoryString({
      name: "小明",
      topic: "宠物功能",
      _topic_at: "2026-07-07",
    });
    expect(s).toContain("name: 小明");
    expect(s).toContain("topic: 宠物功能");
    expect(s).not.toContain("_topic_at");
  });
  it("空记忆返回空串", () => {
    expect(buildMemoryString({})).toBe("");
  });
});

describe("recentContext", () => {
  it("只取最近 N 条并剥掉 ts", () => {
    const history: ChatMessage[] = Array.from({ length: 15 }, (_, i) => ({
      role: i % 2 === 0 ? "user" : "assistant",
      content: `m${i}`,
      ts: i,
    }));
    const ctx = recentContext(history, 10);
    expect(ctx).toHaveLength(10);
    // 最近 10 条是 index 5..14,index 5 是奇数 → assistant
    expect(ctx[0]).toEqual({ role: "assistant", content: "m5" });
    expect((ctx[0] as Record<string, unknown>).ts).toBeUndefined();
  });
  it("历史短于窗口时全取", () => {
    const history: ChatMessage[] = [{ role: "user", content: "hi", ts: 1 }];
    expect(recentContext(history, 10)).toHaveLength(1);
  });
});
