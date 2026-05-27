import { describe, it, expect, vi } from "vitest";

// We test the error extraction logic indirectly by mocking invoke.
// Since api.ts imports @tauri-apps/api/core, we mock it.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import {
  listProviders,
  getProvider,
  createProvider,
  updateProvider,
  deleteProvider,
  AppError,
} from "./api";

describe("API error handling", () => {
  it("listProviders extracts object error", async () => {
    const err = { code: "DB_ERROR", message: "db down" };
    vi.mocked(invoke).mockRejectedValueOnce(err);
    await expect(listProviders()).rejects.toEqual(err);
  });

  it("getProvider extracts string error", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("network timeout");
    await expect(getProvider("p1")).rejects.toEqual({
      code: "UNKNOWN",
      message: "network timeout",
    });
  });

  it("createProvider falls back to generic message", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(404);
    await expect(createProvider({} as any)).rejects.toEqual({
      code: "UNKNOWN",
      message: "An unexpected error occurred",
    });
  });

  it("updateProvider passes correct args", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ id: "p1" });
    const result = await updateProvider("p1", { name: "New" });
    expect(invoke).toHaveBeenCalledWith("update_provider", {
      id: "p1",
      input: { name: "New" },
    });
    expect(result).toEqual({ id: "p1" });
  });

  it("deleteProvider passes correct args", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(true);
    const result = await deleteProvider("p1");
    expect(invoke).toHaveBeenCalledWith("delete_provider", { id: "p1" });
    expect(result).toBe(true);
  });
});
