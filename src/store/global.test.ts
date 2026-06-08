import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@/lib/api", () => ({
  listProviders: vi.fn(),
  getGatewaySettings: vi.fn(),
  listModelPricing: vi.fn(),
  listRouteProfiles: vi.fn(),
}));

import * as api from "@/lib/api";
import {
  useProviders,
  useGatewaySettings,
  usePricing,
  useRouteProfiles,
  __resetGlobalStoresForTest,
} from "./global";

describe("global store", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.mocked(api.listProviders).mockReset();
    vi.mocked(api.getGatewaySettings).mockReset();
    vi.mocked(api.listModelPricing).mockReset();
    vi.mocked(api.listRouteProfiles).mockReset();
  });

  afterEach(() => {
    __resetGlobalStoresForTest();
  });

  describe("useProviders", () => {
    it("fetch loads items and clears loading", async () => {
      const fake = [{ id: "p1", name: "OpenAI" }] as any;
      vi.mocked(api.listProviders).mockResolvedValue(fake);
      await useProviders.getState().fetch();
      const s = useProviders.getState();
      expect(s.items).toEqual(fake);
      expect(s.loading).toBe(false);
      expect(s.error).toBeNull();
      expect(api.listProviders).toHaveBeenCalledTimes(1);
    });

    it("concurrent fetch coalesces into single invoke", async () => {
      let resolve!: (v: any) => void;
      vi.mocked(api.listProviders).mockReturnValue(
        new Promise((r) => { resolve = r; })
      );
      const p1 = useProviders.getState().fetch();
      const p2 = useProviders.getState().fetch();
      resolve([{ id: "p1" }]);
      await Promise.all([p1, p2]);
      expect(api.listProviders).toHaveBeenCalledTimes(1);
      expect(useProviders.getState().items).toEqual([{ id: "p1" }]);
    });

    it("fetch records error message on failure", async () => {
      vi.mocked(api.listProviders).mockRejectedValue({ message: "boom" });
      await useProviders.getState().fetch();
      const s = useProviders.getState();
      expect(s.error).toBe("boom");
      expect(s.loading).toBe(false);
      expect(s.items).toEqual([]);
    });

    it("refetch re-invokes api even after success", async () => {
      vi.mocked(api.listProviders)
        .mockResolvedValueOnce([{ id: "a" }] as any)
        .mockResolvedValueOnce([{ id: "a" }, { id: "b" }] as any);
      await useProviders.getState().fetch();
      expect(useProviders.getState().items).toHaveLength(1);
      await useProviders.getState().refetch();
      expect(useProviders.getState().items).toHaveLength(2);
      expect(api.listProviders).toHaveBeenCalledTimes(2);
    });
  });

  describe("useGatewaySettings", () => {
    it("fetch stores value as single object", async () => {
      const settings = { id: 1, host: "127.0.0.1", port: 11451 } as any;
      vi.mocked(api.getGatewaySettings).mockResolvedValue(settings);
      await useGatewaySettings.getState().fetch();
      expect(useGatewaySettings.getState().value).toEqual(settings);
    });

    it("concurrent fetch coalesces", async () => {
      let resolve!: (v: any) => void;
      vi.mocked(api.getGatewaySettings).mockReturnValue(
        new Promise((r) => { resolve = r; })
      );
      const p1 = useGatewaySettings.getState().fetch();
      const p2 = useGatewaySettings.getState().fetch();
      resolve({ id: 1 });
      await Promise.all([p1, p2]);
      expect(api.getGatewaySettings).toHaveBeenCalledTimes(1);
    });
  });

  describe("usePricing", () => {
    it("fetch + setItems both update items", async () => {
      vi.mocked(api.listModelPricing).mockResolvedValue([
        { id: "a", provider: "openai", model_pattern: "gpt-4" },
      ] as any);
      await usePricing.getState().fetch();
      expect(usePricing.getState().items).toHaveLength(1);
      usePricing.getState().setItems([
        { id: "a", provider: "openai", model_pattern: "gpt-4" } as any,
        { id: "b", provider: "anthropic", model_pattern: "claude" } as any,
      ]);
      expect(usePricing.getState().items).toHaveLength(2);
    });
  });

  describe("useRouteProfiles", () => {
    it("fetch loads profile list", async () => {
      const profiles = [
        { id: "default", name: "Default" },
        { id: "p2", name: "Secondary" },
      ] as any;
      vi.mocked(api.listRouteProfiles).mockResolvedValue(profiles);
      await useRouteProfiles.getState().fetch();
      expect(useRouteProfiles.getState().items).toEqual(profiles);
    });

    it("error message captured", async () => {
      vi.mocked(api.listRouteProfiles).mockRejectedValue({ message: "rpc err" });
      await useRouteProfiles.getState().fetch();
      const s = useRouteProfiles.getState();
      expect(s.error).toBe("rpc err");
      expect(s.items).toEqual([]);
    });
  });
});
