import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, act, waitFor, screen, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/lib/api");

import * as api from "@/lib/api";
import { Tools } from "./Tools";
import { __resetGlobalStoresForTest } from "@/store/global";

afterEach(() => cleanup());

function gatewayStatus(): any {
  return {
    running: true,
    host: "127.0.0.1",
    port: 4141,
  };
}

describe("Tools", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.mocked(api.detectCodexConfig).mockResolvedValue({
      exists: true,
      has_agentgate: false,
    } as any);
    vi.mocked(api.detectClaudeCodeEnv).mockResolvedValue({
      settings_exists: false,
      has_api_key: false,
      has_auth_token: false,
      has_agentgate: false,
    } as any);
    vi.mocked(api.detectOpenCodeConfig).mockResolvedValue({
      exists: false,
      has_agentgate: false,
    } as any);
    vi.mocked(api.detectGeminiConfig).mockResolvedValue({
      exists: false,
      has_agentgate: false,
    } as any);
    vi.mocked(api.detectAtomCodeConfig).mockResolvedValue({
      exists: false,
      has_agentgate: false,
    } as any);
    vi.mocked(api.detectClaudeDesktop).mockResolvedValue({
      installed: false,
      supported: false,
      has_agentgate_profile: false,
    } as any);
    vi.mocked(api.getGatewayStatus).mockResolvedValue(gatewayStatus());
    vi.mocked(api.clientsWithApplyHistory).mockResolvedValue([]);
    vi.mocked(api.generateCodexConfig).mockResolvedValue("{}");
    vi.mocked(api.testToolConnection).mockResolvedValue({
      config_ok: true,
      gateway_ok: true,
      provider_ok: true,
    } as any);
  });

  it("renders client list and loads statuses", async () => {
    render(
      <MemoryRouter>
        <Tools />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(api.detectCodexConfig).toHaveBeenCalled();
      expect(api.getGatewayStatus).toHaveBeenCalled();
    });

    expect(screen.getByText("tools.clients")).toBeInTheDocument();
  });

  it("runs connection test", async () => {
    render(
      <MemoryRouter>
        <Tools />
      </MemoryRouter>
    );

    await screen.findByText("tools.test_connection");

    const testBtn = screen.getByText("tools.test_connection");
    await act(async () => testBtn.click());

    await waitFor(() => expect(api.testToolConnection).toHaveBeenCalled());
  });
});
