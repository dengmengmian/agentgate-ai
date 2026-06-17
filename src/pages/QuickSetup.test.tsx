import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  render,
  act,
  waitFor,
  screen,
  fireEvent,
  cleanup,
} from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/lib/api");
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  readText: vi.fn().mockResolvedValue(""),
}));
vi.mock("@/lib/providerAutoSetup", () => ({
  fetchDetectAndPersistProviderModels: vi
    .fn()
    .mockResolvedValue({ models: [] }),
}));

import * as api from "@/lib/api";
import { QuickSetup } from "./QuickSetup";

afterEach(() => cleanup());

describe("QuickSetup", () => {
  beforeEach(() => {
    vi.mocked(api.detectCodexConfig).mockResolvedValue({
      exists: false,
    } as any);
    vi.mocked(api.detectClaudeCodeEnv).mockResolvedValue({
      settings_exists: false,
      has_api_key: false,
      has_auth_token: false,
      has_agentgate: false,
    } as any);
    vi.mocked(api.detectOpenCodeConfig).mockResolvedValue({
      exists: false,
    } as any);
    vi.mocked(api.detectGeminiConfig).mockResolvedValue({
      exists: false,
    } as any);
    vi.mocked(api.detectAtomCodeConfig).mockResolvedValue({
      exists: false,
    } as any);
    vi.mocked(api.createProvider).mockResolvedValue({
      id: "p1",
      name: "OpenAI",
      provider_type: "openai",
    } as any);
    vi.mocked(api.setActiveProvider).mockResolvedValue({} as any);
    vi.mocked(api.startGateway).mockResolvedValue({ running: true } as any);
    vi.mocked(api.applyCodexConfig).mockResolvedValue({ success: true } as any);
    vi.mocked(api.applyClaudeCodeConfig).mockResolvedValue({
      success: true,
    } as any);
    vi.mocked(api.applyOpenCodeConfig).mockResolvedValue({
      success: true,
    } as any);
    vi.mocked(api.applyGeminiConfig).mockResolvedValue({
      success: true,
    } as any);
    vi.mocked(api.applyAtomCodeConfig).mockResolvedValue({
      success: true,
    } as any);
    vi.mocked(api.testToolConnection).mockResolvedValue({
      config_ok: true,
      gateway_ok: true,
      provider_ok: true,
    } as any);
  });

  it("renders key step and detects provider", async () => {
    render(
      <MemoryRouter>
        <QuickSetup />
      </MemoryRouter>
    );

    expect(await screen.findByText("onboarding.welcome")).toBeInTheDocument();

    const input = screen.getByPlaceholderText(/sk-xxx/);
    fireEvent.change(input, { target: { value: "sk-testkey" } });

    const select = await screen.findByRole("combobox");
    expect(select).toHaveValue("openai");
  });

  it("advances to tools step and runs setup", async () => {
    render(
      <MemoryRouter>
        <QuickSetup />
      </MemoryRouter>
    );

    const input = screen.getByPlaceholderText(/sk-xxx/);
    fireEvent.change(input, { target: { value: "sk-testkey" } });
    await screen.findByRole("combobox");

    const next = screen.getByText("onboarding.next");
    await act(async () => next.click());

    expect(
      await screen.findByRole("heading", { name: "onboarding.select_tools" })
    ).toBeInTheDocument();

    const setup = screen.getAllByText("onboarding.start_setup").pop()!;
    await act(async () => setup.click());

    await waitFor(() => expect(api.createProvider).toHaveBeenCalled());
  });
});
