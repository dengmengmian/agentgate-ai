import { describe, it, expect, vi } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import { GeneralTab } from "./GeneralTab";
import { renderWithProviders } from "@/components/test-utils";

// 锁住成本预警 UI:开关触发 + 阈值 onBlur 的保存边界(>0、变化才存)。
// ToggleSwitch/ThemePicker 用最简 stub,避免引入无关依赖。

const ToggleSwitch = ({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) => (
  <input
    type="checkbox"
    checked={checked}
    onChange={(e) => onChange(e.target.checked)}
  />
);
const ThemePicker = () => <div data-testid="theme-picker" />;

function baseSettings(over: Record<string, unknown> = {}) {
  return {
    host: "127.0.0.1",
    port: 9090,
    active_provider_id: null,
    input_protocol: "openai_responses",
    output_protocol: "openai_chat_completions",
    auto_start: false,
    log_retention_days: 14,
    body_filter_global: false,
    thinking_rectifier_global: false,
    error_mapper_global: false,
    health_probe_enabled: false,
    codex_compact_enabled: true,
    codex_compact_summary_max_tokens: 1500,
    cost_alert_enabled: false,
    cost_alert_threshold: null,
    updated_at: "",
    ...over,
  };
}

function setup(over: Record<string, unknown> = {}) {
  const handleUpdateCostAlert = vi.fn();
  renderWithProviders(
    <GeneralTab
      settings={baseSettings(over) as any}
      locale="zh"
      setLocale={vi.fn()}
      theme="dark"
      setTheme={vi.fn()}
      handleUpdateAutoStart={vi.fn()}
      handleUpdateRefinerGlobal={vi.fn()}
      handleUpdateCostAlert={handleUpdateCostAlert}
      t={(k: string) => k}
      ToggleSwitch={ToggleSwitch}
      ThemePicker={ThemePicker}
    />
  );
  return { handleUpdateCostAlert };
}

describe("GeneralTab 成本预警", () => {
  it("开启开关 → 保存 cost_alert_enabled:true(cost_alert 是最后一个 toggle)", () => {
    const { handleUpdateCostAlert } = setup();
    const boxes = screen.getAllByRole("checkbox");
    fireEvent.click(boxes[boxes.length - 1]);
    expect(handleUpdateCostAlert).toHaveBeenCalledWith({
      cost_alert_enabled: true,
    });
  });

  it("未开启时不显示阈值输入", () => {
    setup({ cost_alert_enabled: false });
    expect(screen.queryByPlaceholderText("10")).toBeNull();
  });

  it("开启时有效阈值 onBlur 保存", () => {
    const { handleUpdateCostAlert } = setup({ cost_alert_enabled: true });
    fireEvent.blur(screen.getByPlaceholderText("10"), {
      target: { value: "20" },
    });
    expect(handleUpdateCostAlert).toHaveBeenCalledWith({
      cost_alert_threshold: 20,
    });
  });

  it("阈值为 0 / 非法 / 不变时不保存", () => {
    const { handleUpdateCostAlert } = setup({
      cost_alert_enabled: true,
      cost_alert_threshold: 5,
    });
    const input = screen.getByPlaceholderText("10");
    fireEvent.blur(input, { target: { value: "0" } }); // <=0
    fireEvent.blur(input, { target: { value: "abc" } }); // NaN
    fireEvent.blur(input, { target: { value: "5" } }); // 与当前值相同
    expect(handleUpdateCostAlert).not.toHaveBeenCalled();
  });
});
