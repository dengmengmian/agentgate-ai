import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, fireEvent, act, waitFor } from "@testing-library/react";
import { ProviderFormDialog } from "./ProviderFormDialog";
import { renderWithProviders } from "@/components/test-utils";
import * as api from "@/lib/api";
import type { ProviderView } from "@/types/provider";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getProviderKeys: vi.fn(),
  };
});

function makeProvider(overrides: Partial<ProviderView> = {}): ProviderView {
  return {
    id: "p1",
    name: "OpenAI",
    provider_type: "openai",
    base_url: "https://api.openai.com",
    api_key: "sk-***",
    masked_api_key: "sk-***",
    default_model: "gpt-5.5",
    reasoning_model: null,
    supported_models: JSON.stringify(["gpt-5.5"]),
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
    is_active: true,
    status: "connected",
    supports_vision: false,
    supports_cache: null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    ...overrides,
  } as ProviderView;
}

describe("ProviderFormDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getProviderKeys).mockResolvedValue(["sk-real"]);
  });

  it("quick creates a provider from a detected API key", async () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    renderWithProviders(
      <ProviderFormDialog open onSubmit={onSubmit} onClose={onClose} />
    );

    const keyInput = screen.getByPlaceholderText(/sk-xxx/);
    await act(async () => {
      fireEvent.change(keyInput, { target: { value: "deepseek-testkey" } });
    });

    const createBtn = () =>
      screen
        .getAllByRole("button", { name: /Create Provider/i })
        .find((b) => b.getAttribute("type") === "button");

    await waitFor(() => expect(createBtn()).toBeEnabled());

    await act(async () => {
      fireEvent.click(createBtn()!);
    });

    expect(onSubmit).toHaveBeenCalledTimes(1);
    const submitted = onSubmit.mock.calls[0][0];
    expect(submitted.provider_type).toBe("deepseek");
    expect(submitted.api_key).toBe("deepseek-testkey");
    expect(submitted.base_url).toBe("https://api.deepseek.com");
  });

  it("switches to manual mode and submits a new provider", async () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    renderWithProviders(
      <ProviderFormDialog open onSubmit={onSubmit} onClose={onClose} />
    );

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Manual setup/i }));
    });

    const nameInput = screen.getByPlaceholderText(/My Provider/i);
    await act(async () => {
      fireEvent.change(nameInput, { target: { value: "My DeepSeek" } });
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Create Provider/i }));
    });

    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    const submitted = onSubmit.mock.calls[0][0];
    expect(submitted.name).toBe("My DeepSeek");
    expect(submitted.provider_type).toBe("deepseek");
    expect(submitted.base_url).toBe("https://api.deepseek.com");
  });

  it("prefills edit mode and submits an update", async () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();
    const provider = makeProvider();

    renderWithProviders(
      <ProviderFormDialog
        open
        provider={provider}
        onSubmit={onSubmit}
        onClose={onClose}
      />
    );

    await waitFor(() => expect(api.getProviderKeys).toHaveBeenCalledWith("p1"));

    const nameInput = screen.getByPlaceholderText(/My Provider/i);
    await act(async () => {
      fireEvent.change(nameInput, { target: { value: "OpenAI Updated" } });
    });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Save Changes/i }));
    });

    await waitFor(() => expect(onSubmit).toHaveBeenCalled());
    const submitted = onSubmit.mock.calls[0][0];
    expect(submitted.name).toBe("OpenAI Updated");
    expect(submitted.provider_type).toBe("openai");
    expect(submitted.api_key).toBe("sk-real");
  });
});
