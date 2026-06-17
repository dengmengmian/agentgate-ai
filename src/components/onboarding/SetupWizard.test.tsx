import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, fireEvent, act, waitFor } from "@testing-library/react";
import { SetupWizard } from "./SetupWizard";
import { renderWithProviders } from "@/components/test-utils";
import * as api from "@/lib/api";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    createProvider: vi.fn().mockResolvedValue({ id: "p1" }),
    setActiveProvider: vi.fn().mockResolvedValue(undefined),
    startGateway: vi.fn().mockResolvedValue(undefined),
    detectCodexConfig: vi.fn().mockResolvedValue({ exists: false }),
    detectClaudeCodeEnv: vi.fn().mockResolvedValue({ settings_exists: false }),
    detectOpenCodeConfig: vi.fn().mockResolvedValue({ exists: false }),
    detectGeminiConfig: vi.fn().mockResolvedValue({ exists: false }),
    detectAtomCodeConfig: vi.fn().mockResolvedValue({ exists: false }),
    applyCodexConfig: vi.fn().mockResolvedValue({ success: true }),
    applyClaudeCodeConfig: vi.fn().mockResolvedValue({ success: true }),
    applyOpenCodeConfig: vi.fn().mockResolvedValue({ success: true }),
    applyGeminiConfig: vi.fn().mockResolvedValue({ success: true }),
    applyAtomCodeConfig: vi.fn().mockResolvedValue({ success: true }),
    testToolConnection: vi.fn().mockResolvedValue({ provider_ok: true }),
  };
});

vi.mock("@/lib/providerAutoSetup", () => ({
  fetchDetectAndPersistProviderModels: vi.fn().mockResolvedValue({
    models: ["deepseek-v4-flash"],
    capabilitiesDetected: true,
  }),
}));

describe("SetupWizard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("detects provider from key and completes setup", async () => {
    const onComplete = vi.fn();
    renderWithProviders(<SetupWizard onComplete={onComplete} />);

    const keyInput = screen.getByPlaceholderText(/sk-xxx/);
    await act(async () => {
      fireEvent.change(keyInput, { target: { value: "deepseek-testkey" } });
    });

    expect(await screen.findByText(/Detected:/i)).toBeInTheDocument();

    const nextBtn = screen.getByRole("button", { name: /Next/i });
    await waitFor(() => expect(nextBtn).toBeEnabled());
    await act(async () => fireEvent.click(nextBtn));

    expect(screen.getByText(/Select Tools/i)).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Start Setup/i }));
    });

    await waitFor(() =>
      expect(api.createProvider).toHaveBeenCalledWith(
        expect.objectContaining({
          provider_type: "deepseek",
          api_key: "deepseek-testkey",
        })
      )
    );

    expect(await screen.findByText(/All set!/i)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /View Clients/i })
    ).toBeInTheDocument();
  });

  it("skip button calls onComplete", async () => {
    const onComplete = vi.fn();
    renderWithProviders(<SetupWizard onComplete={onComplete} />);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Skip/i }));
    });

    expect(onComplete).toHaveBeenCalled();
  });
});
