import { describe, it, expect } from "vitest";
import { visibleEntries, mergeMemory, memoryLabel } from "./petMemory";

describe("visibleEntries", () => {
  it("过滤内部 _ 字段", () => {
    const e = visibleEntries({
      name: "小明",
      topic: "宠物功能",
      _topic_at: "2026-07-07",
    });
    expect(e).toEqual([
      { key: "name", value: "小明" },
      { key: "topic", value: "宠物功能" },
    ]);
  });
  it("空记忆返回空数组", () => {
    expect(visibleEntries({})).toEqual([]);
  });
});

describe("mergeMemory", () => {
  it("保留内部字段,写入编辑项", () => {
    const out = mergeMemory(
      { name: "旧名", _topic_at: "2026-07-07" },
      [{ key: "name", value: "新名" }, { key: "mood", value: "开心" }]
    );
    expect(out).toEqual({
      _topic_at: "2026-07-07",
      name: "新名",
      mood: "开心",
    });
  });
  it("丢弃空 key/value,去掉的项不保留(实现删除)", () => {
    const out = mergeMemory({ name: "小明", topic: "旧话题" }, [
      { key: "name", value: "小明" },
      { key: "topic", value: "" }, // 清空 = 删除
      { key: "", value: "无 key" },
    ]);
    expect(out).toEqual({ name: "小明" });
  });
  it("不让用户注入内部 _ 字段", () => {
    const out = mergeMemory({}, [{ key: "_evil", value: "x" }]);
    expect(out).toEqual({});
  });
  it("trim key/value", () => {
    const out = mergeMemory({}, [{ key: "  name ", value: " 小明 " }]);
    expect(out).toEqual({ name: "小明" });
  });
});

describe("memoryLabel", () => {
  it("已知键给友好名", () => {
    expect(memoryLabel("name", "zh")).toBe("名字");
    expect(memoryLabel("topic", "en")).toBe("Recent topic");
  });
  it("未知键用 key 本身", () => {
    expect(memoryLabel("mood", "zh")).toBe("mood");
  });
});
