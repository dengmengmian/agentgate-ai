import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, act, waitFor, screen, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/lib/api");
vi.stubGlobal("__TAURI_INTERNALS__", {
  invoke: vi.fn().mockResolvedValue(""),
  transformCallback: vi.fn((cb) => cb),
});
vi.stubGlobal("__TAURI_EVENT_PLUGIN_INTERNALS__", {
  unregisterListener: vi.fn(),
});
vi.mock("@tauri-apps/plugin-autostart", () => ({
  isEnabled: vi.fn().mockResolvedValue(false),
  enable: vi.fn().mockResolvedValue(undefined),
  disable: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn().mockResolvedValue(null),
}));
vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.0.0"),
}));

import * as api from "@/lib/api";
import { Settings } from "./Settings";
import { __resetGlobalStoresForTest } from "@/store/global";

afterEach(() => cleanup());

function gatewaySettings(): any {
  return {
    host: "127.0.0.1",
    port: 4141,
    input_protocol: "openai_responses",
    output_protocol: "openai_chat_completions",
    auto_start: true,
    log_retention_days: 30,
    body_filter_global: false,
    thinking_rectifier_global: false,
    error_mapper_global: false,
    health_probe_enabled: false,
  };
}

describe("Settings", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.mocked(api.getGatewaySettings).mockResolvedValue(gatewaySettings());
    vi.mocked(api.getGatewayAuthSettings).mockResolvedValue({
      token_path: "/tmp/token",
    } as any);
    vi.mocked(api.getPetSettings).mockResolvedValue({
      pet_type: "robot",
      visible: true,
    } as any);
    vi.mocked(api.getPetClickThrough).mockResolvedValue(false);
    vi.mocked(api.listModelPricing).mockResolvedValue([]);
    vi.mocked(api.updateGatewaySettings).mockResolvedValue(gatewaySettings());
    vi.mocked(api.updatePetSettings).mockResolvedValue({
      pet_type: "robot",
      visible: true,
    } as any);
    vi.mocked(api.setPetVisible).mockResolvedValue({
      pet_type: "robot",
      visible: false,
    } as any);
    vi.mocked(api.getLocalAccessToken).mockResolvedValue("token");
    vi.mocked(api.regenerateLocalAccessToken).mockResolvedValue({
      token_path: "/tmp/token",
    } as any);
    vi.mocked(api.exportConfigJson).mockResolvedValue("{}");
    vi.mocked(api.importConfigJson).mockResolvedValue({} as any);
  });

  it("renders and loads settings", async () => {
    render(
      <MemoryRouter>
        <Settings />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(api.getGatewaySettings).toHaveBeenCalled();
      expect(api.getGatewayAuthSettings).toHaveBeenCalled();
      expect(api.getPetSettings).toHaveBeenCalled();
    });

    expect(screen.getByText("settings.tab.general")).toBeInTheDocument();
  });

  it("toggles auto start gateway", async () => {
    const { container } = render(
      <MemoryRouter>
        <Settings />
      </MemoryRouter>
    );

    await screen.findByText("settings.auto_start_gateway");

    const autoStart = container.querySelector(
      'input[type="checkbox"]'
    )! as HTMLElement;
    await act(async () => autoStart.click());

    await waitFor(() =>
      expect(api.updateGatewaySettings).toHaveBeenCalledWith(
        expect.objectContaining({ auto_start: false })
      )
    );
  });

  it("switches tabs and regenerates the local token after confirmation", async () => {
    render(
      <MemoryRouter>
        <Settings />
      </MemoryRouter>
    );

    await screen.findByText("settings.tab.security");
    await act(async () => screen.getByText("settings.tab.security").click());

    expect(
      await screen.findByText("settings.gateway_security")
    ).toBeInTheDocument();

    await act(async () => {
      screen.getByText("settings.regenerate_token").click();
    });
    expect(await screen.findByText("settings.regen_title")).toBeInTheDocument();

    const regenButtons = screen.getAllByText("settings.regenerate_token");
    const confirm = regenButtons[regenButtons.length - 1];
    await act(async () => confirm.click());

    await waitFor(() =>
      expect(api.regenerateLocalAccessToken).toHaveBeenCalled()
    );
  });

  it("exports config from the data tab without secrets by default", async () => {
    const objectUrl = "blob:agentgate-test";
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn().mockReturnValue(objectUrl),
      revokeObjectURL: vi.fn(),
    });

    render(
      <MemoryRouter>
        <Settings />
      </MemoryRouter>
    );

    await screen.findByText("settings.tab.data");
    await act(async () => screen.getByText("settings.tab.data").click());

    const exportButton = await screen.findByText("settings.export_config");
    await act(async () => exportButton.click());

    await waitFor(() =>
      expect(api.exportConfigJson).toHaveBeenCalledWith(false)
    );
  });
});
