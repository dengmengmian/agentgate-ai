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
    request_body_limit_mb: 32,
    cost_alert_enabled: false,
    cost_alert_threshold: null,
    wake_enabled: true,
    wake_request_control: false,
    wake_cooldown_seconds: 900,
    wake_keep_display_awake: false,
    updated_at: "",
    ...over,
  };
}

function setup(over: Record<string, unknown> = {}) {
  const handleUpdateCostAlert = vi.fn();
  const handleUpdateRequestBodyLimit = vi.fn();
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
      handleUpdateRequestBodyLimit={handleUpdateRequestBodyLimit}
      wakeStatus={null}
      handleUpdateWake={vi.fn()}
      t={(k: string) => k}
      ToggleSwitch={ToggleSwitch}
      ThemePicker={ThemePicker}
    />
  );
  return { handleUpdateCostAlert, handleUpdateRequestBodyLimit };
}

function openAdvanced() {
  fireEvent.click(screen.getByText("settings.general.advanced"));
}

describe("GeneralTab 成本预警", () => {
  it("按基础 / 外观 / 高级分组，高级默认折叠", () => {
    setup();

    expect(screen.getByText("settings.general.basic")).toBeInTheDocument();
    expect(screen.getByText("settings.general.appearance")).toBeInTheDocument();
    expect(screen.getByText("settings.general.advanced")).toBeInTheDocument();
    expect(screen.getByTestId("theme-picker")).toBeInTheDocument();
    expect(screen.queryByText("settings.body_filter")).toBeNull();
  });

  it("开启开关 → 保存 cost_alert_enabled:true(cost_alert 是最后一个 toggle)", () => {
    const { handleUpdateCostAlert } = setup();
    openAdvanced();
    const boxes = screen.getAllByRole("checkbox");
    fireEvent.click(boxes[boxes.length - 1]);
    expect(handleUpdateCostAlert).toHaveBeenCalledWith({
      cost_alert_enabled: true,
    });
  });

  it("未开启时不显示阈值输入", () => {
    setup({ cost_alert_enabled: false });
    openAdvanced();
    expect(screen.queryByPlaceholderText("10")).toBeNull();
  });

  it("开启时有效阈值 onBlur 保存", () => {
    const { handleUpdateCostAlert } = setup({ cost_alert_enabled: true });
    openAdvanced();
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
    openAdvanced();
    const input = screen.getByPlaceholderText("10");
    fireEvent.blur(input, { target: { value: "0" } }); // <=0
    fireEvent.blur(input, { target: { value: "abc" } }); // NaN
    fireEvent.blur(input, { target: { value: "5" } }); // 与当前值相同
    expect(handleUpdateCostAlert).not.toHaveBeenCalled();
  });

  it("有效请求体上限 onBlur 保存", () => {
    const { handleUpdateRequestBodyLimit } = setup();
    fireEvent.blur(screen.getByDisplayValue("32"), {
      target: { value: "64" },
    });
    expect(handleUpdateRequestBodyLimit).toHaveBeenCalledWith(64);
  });

  it("请求体上限为 0 / 非法 / 不变时不保存", () => {
    const { handleUpdateRequestBodyLimit } = setup({
      request_body_limit_mb: 32,
    });
    const input = screen.getByDisplayValue("32");
    fireEvent.blur(input, { target: { value: "0" } });
    fireEvent.blur(input, { target: { value: "abc" } });
    fireEvent.blur(input, { target: { value: "32" } });
    expect(handleUpdateRequestBodyLimit).not.toHaveBeenCalled();
  });

  it("请求体上限超过 128MB 时不保存", () => {
    const { handleUpdateRequestBodyLimit } = setup({
      request_body_limit_mb: 32,
    });
    fireEvent.blur(screen.getByDisplayValue("32"), {
      target: { value: "4096" },
    });
    expect(handleUpdateRequestBodyLimit).not.toHaveBeenCalled();
  });
});
