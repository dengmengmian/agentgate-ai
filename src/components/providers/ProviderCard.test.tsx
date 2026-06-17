import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen, fireEvent, waitFor, act } from "@testing-library/react";
import { ProviderCard } from "./ProviderCard";
import { renderWithProviders } from "@/components/test-utils";
import * as api from "@/lib/api";
import type { ProviderView } from "@/types/provider";
import type { ProviderHealth } from "@/types/stats";
import { __resetGlobalStoresForTest } from "@/store/global";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getProviderHealth: vi.fn(),
  };
});

function makeProvider(overrides: Partial<ProviderView> = {}): ProviderView {
  return {
    id: "p1",
    name: "DeepSeek",
    provider_type: "deepseek",
    base_url: "https://api.deepseek.com",
    api_key: "sk-***",
    masked_api_key: "sk-***",
    default_model: "deepseek-v4-flash",
    reasoning_model: null,
    supported_models: null,
    model_capabilities: null,
    model_context_windows: null,
    model_mapping: null,
    extra_headers: null,
    anthropic_base_url: null,
    responses_base_url: null,
    auto_cache_control: true,
    protocol: JSON.stringify(["openai_chat_completions"]),
    timeout_seconds: 120,
    enabled: true,
    is_active: false,
    status: "not_tested",
    supports_vision: false,
    supports_cache: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    ...overrides,
  } as ProviderView;
}

describe("ProviderCard", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.mocked(api.getProviderHealth).mockResolvedValue(
      null as unknown as ProviderHealth
    );
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("renders provider details and active badge", async () => {
    const provider = makeProvider({ is_active: true, status: "connected" });
    renderWithProviders(
      <ProviderCard
        provider={provider}
        onEdit={() => {}}
        onDelete={() => {}}
        onSetActive={() => {}}
        onTest={() => {}}
      />
    );

    expect(screen.getByText("DeepSeek")).toBeInTheDocument();
    expect(screen.getByText("https://api.deepseek.com")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();

    await waitFor(() =>
      expect(api.getProviderHealth).toHaveBeenCalledWith("DeepSeek")
    );
  });

  it("fires edit, test, set active and delete callbacks", () => {
    const provider = makeProvider({ is_active: false });
    const onEdit = vi.fn();
    const onDelete = vi.fn();
    const onSetActive = vi.fn();
    const onTest = vi.fn();

    renderWithProviders(
      <ProviderCard
        provider={provider}
        onEdit={onEdit}
        onDelete={onDelete}
        onSetActive={onSetActive}
        onTest={onTest}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: /Edit/i }));
    expect(onEdit).toHaveBeenCalledWith(provider);

    fireEvent.click(screen.getByRole("button", { name: /Test/i }));
    expect(onTest).toHaveBeenCalledWith(provider);

    fireEvent.click(screen.getByRole("button", { name: /Set Active/i }));
    expect(onSetActive).toHaveBeenCalledWith(provider);

    fireEvent.click(screen.getByRole("button", { name: /Delete/i }));
    expect(onDelete).toHaveBeenCalledWith(provider);
  });

  it("disables test button while testing", () => {
    const provider = makeProvider();
    renderWithProviders(
      <ProviderCard
        provider={provider}
        onEdit={() => {}}
        onDelete={() => {}}
        onSetActive={() => {}}
        onTest={() => {}}
        testing
      />
    );

    const testBtn = screen.getByRole("button", { name: /Test/i });
    expect(testBtn).toBeDisabled();
  });

  it("toggles detail section", async () => {
    const provider = makeProvider();
    renderWithProviders(
      <ProviderCard
        provider={provider}
        onEdit={() => {}}
        onDelete={() => {}}
        onSetActive={() => {}}
        onTest={() => {}}
      />
    );

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Details/i }));
    });
    expect(screen.getByText(/Type/i)).toBeInTheDocument();
    expect(screen.getByText("deepseek")).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Details/i }));
    });
    expect(screen.queryByText(/Type/i)).not.toBeInTheDocument();
  });
});
