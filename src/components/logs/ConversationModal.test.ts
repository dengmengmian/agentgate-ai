import { describe, expect, it } from "vitest";
import { getConversationMessageKind } from "@/components/logs/ConversationModal";

describe("getConversationMessageKind", () => {
  it("detects tool calls", () => {
    expect(getConversationMessageKind("[Tool: Bash]")).toBe("tool_call");
    expect(getConversationMessageKind(" [Tool: Read] ")).toBe("tool_call");
  });

  it("detects tool results", () => {
    expect(getConversationMessageKind("[Tool result] total 536")).toBe(
      "tool_result"
    );
  });

  it("keeps normal messages as chat", () => {
    expect(getConversationMessageKind("我先了解一下项目结构和状态。")).toBe(
      "chat"
    );
  });
});
