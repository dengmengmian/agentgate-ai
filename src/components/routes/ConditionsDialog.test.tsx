import { describe, it, expect, vi } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import { ConditionsDialog } from "./ConditionsDialog";
import { renderWithProviders } from "@/components/test-utils";
import type { RoutingConditions } from "@/types/route-profile";

// 锁住「场景路由」的前端逻辑:预设勾选 ↔ RoutingConditions 的互转
// (detectCheckedPresets / mergePresetConditions / isBackgroundPreset /
// hasCustomOnlyConditions)。定位用 checkbox role + 按钮正则,语言无关。
//
// CONDITION_PRESETS 顺序:background(0) / images(1) / long_text(2) / tools(3)。

function setup(current: RoutingConditions = {}) {
  const onSave = vi.fn();
  const onClose = vi.fn();
  renderWithProviders(
    <ConditionsDialog
      target={{
        providerName: "DeepSeek",
        inputProtocol: "anthropic_messages",
        current,
      }}
      onSave={onSave}
      onClose={onClose}
    />
  );
  return { onSave, onClose };
}

const save = () =>
  fireEvent.click(screen.getByRole("button", { name: /save|保存/i }));

describe("ConditionsDialog 场景路由", () => {
  it("勾选「后台/子任务」预设 → 保存为 model_name_match:[haiku]", () => {
    const { onSave } = setup();
    fireEvent.click(screen.getAllByRole("checkbox")[0]); // background
    save();
    expect(onSave).toHaveBeenCalledWith({ model_name_match: ["haiku"] });
  });

  it("current 含 haiku 匹配时,「后台」预设初始勾选", () => {
    setup({ model_name_match: ["haiku"] });
    expect(screen.getAllByRole("checkbox")[0]).toBeChecked();
  });

  it("后台 + 图片可叠加保存", () => {
    const { onSave } = setup();
    fireEvent.click(screen.getAllByRole("checkbox")[0]); // background
    fireEvent.click(screen.getAllByRole("checkbox")[1]); // images
    save();
    expect(onSave).toHaveBeenCalledWith({
      model_name_match: ["haiku"],
      has_images: true,
    });
  });

  it("自定义模式可输入任意模型名匹配", () => {
    const { onSave } = setup();
    fireEvent.click(screen.getByRole("button", { name: /custom|自定义/i }));
    fireEvent.change(screen.getByPlaceholderText("haiku, flash"), {
      target: { value: "sonnet, opus" },
    });
    save();
    expect(onSave).toHaveBeenCalledWith({
      model_name_match: ["sonnet", "opus"],
    });
  });

  it("current 是非 haiku 的自定义匹配 → 初始进入自定义模式", () => {
    setup({ model_name_match: ["sonnet"] });
    const input = screen.getByPlaceholderText(
      "haiku, flash"
    ) as HTMLInputElement;
    expect(input.value).toBe("sonnet");
  });

  it("清除按钮保存空条件", () => {
    const { onSave } = setup({ model_name_match: ["haiku"] });
    fireEvent.click(
      screen.getByRole("button", { name: /clear all|清除所有/i })
    );
    expect(onSave).toHaveBeenCalledWith({});
  });
});
