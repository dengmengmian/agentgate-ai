import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@/lib/api", () => ({
  fetchProviderModels: vi.fn(),
  seedModelCapabilities: vi.fn(),
  updateProvider: vi.fn(),
}));

import * as api from "@/lib/api";
import { fetchDetectAndPersistProviderModels } from "./providerAutoSetup";

describe("fetchDetectAndPersistProviderModels", () => {
  beforeEach(() => {
    vi.mocked(api.fetchProviderModels).mockReset();
    vi.mocked(api.seedModelCapabilities).mockReset();
    vi.mocked(api.updateProvider).mockReset();
  });

  it("returns empty result when provider returns no models", async () => {
    vi.mocked(api.fetchProviderModels).mockResolvedValue([]);

    const result = await fetchDetectAndPersistProviderModels("p1", "openai");

    expect(result).toEqual({ models: [], capabilitiesDetected: false });
    expect(api.updateProvider).not.toHaveBeenCalled();
  });

  it("persists models and capabilities when seed succeeds", async () => {
    vi.mocked(api.fetchProviderModels).mockResolvedValue(["gpt-4", "gpt-3.5"]);
    vi.mocked(api.seedModelCapabilities).mockResolvedValue({
      "gpt-4": ["vision"],
    });

    const result = await fetchDetectAndPersistProviderModels("p1", "openai");

    expect(result.models).toEqual(["gpt-4", "gpt-3.5"]);
    expect(result.capabilitiesDetected).toBe(true);
    expect(api.updateProvider).toHaveBeenCalledWith(
      "p1",
      expect.objectContaining({
        supported_models: JSON.stringify(["gpt-4", "gpt-3.5"]),
        default_model: "gpt-4",
        model_capabilities: JSON.stringify({ "gpt-4": ["vision"] }),
      })
    );
  });

  it("still persists models when capability seeding fails", async () => {
    vi.mocked(api.fetchProviderModels).mockResolvedValue(["gpt-4"]);
    vi.mocked(api.seedModelCapabilities).mockRejectedValue(
      new Error("seed error")
    );

    const result = await fetchDetectAndPersistProviderModels("p1", "openai");

    expect(result.capabilitiesDetected).toBe(false);
    expect(api.updateProvider).toHaveBeenCalledWith(
      "p1",
      expect.objectContaining({
        supported_models: JSON.stringify(["gpt-4"]),
        default_model: "gpt-4",
      })
    );
    const callArg = vi.mocked(api.updateProvider).mock.calls[0][1] as Record<
      string,
      unknown
    >;
    expect(callArg.model_capabilities).toBeUndefined();
  });
});
