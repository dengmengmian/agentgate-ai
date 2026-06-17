import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  render,
  act,
  waitFor,
  screen,
  cleanup,
  fireEvent,
} from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/lib/api");

import * as api from "@/lib/api";
import { Routes } from "./Routes";
import { __resetGlobalStoresForTest } from "@/store/global";

afterEach(() => cleanup());

function profile(id: string): any {
  return {
    id,
    name: "Default Route",
    input_protocol: "openai_responses",
    mode: "manual",
    selection_strategy: "priority",
    is_default: true,
    active_provider_name: "OpenAI",
    providers_count: 1,
  };
}

function profileDetail(id: string): any {
  return {
    profile: profile(id),
    providers: [
      {
        id: "rp1",
        provider_id: "p1",
        provider_name: "OpenAI",
        provider_type: "openai",
        provider_protocol: JSON.stringify(["openai_responses"]),
        priority: 1,
        model_override: null,
        routing_conditions: null,
        supports_vision: false,
        supports_cache: null,
        model_capabilities: null,
        has_anthropic_url: false,
        consecutive_failures: 0,
        cooldown_until: null,
      },
    ],
  };
}

describe("Routes", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.mocked(api.listRouteProfiles).mockResolvedValue([]);
    vi.mocked(api.listProviders).mockResolvedValue([]);
    vi.mocked(api.aggregateRouteProfileStats).mockResolvedValue([]);
    vi.mocked(api.getRouteProfile).mockResolvedValue(profileDetail("r1"));
    vi.mocked(api.setRouteProfileMode).mockResolvedValue(true);
    vi.mocked(api.setDefaultRouteProfile).mockResolvedValue(true);
    vi.mocked(api.createRouteProfile).mockResolvedValue(profile("new"));
    vi.mocked(api.deleteRouteProfile).mockResolvedValue(true);
    vi.mocked(api.updateRouteProfile).mockResolvedValue(profile("r1"));
    vi.mocked(api.setRouteActiveProvider).mockResolvedValue(true);
    vi.mocked(api.addProviderToRoute).mockResolvedValue(true);
    vi.mocked(api.removeProviderFromRoute).mockResolvedValue(true);
    vi.mocked(api.reorderRouteProviders).mockResolvedValue(true);
    vi.mocked(api.resetProviderRuntimeStatus).mockResolvedValue({
      provider_id: "p1",
      available: true,
      consecutive_failures: 0,
      cooldown_until: null,
      last_failure_code: null,
      last_failure_message: null,
      last_probe_at: null,
      last_probe_success: null,
      last_probe_latency_ms: null,
      last_probe_error: null,
    } as any);
  });

  it("renders empty state when no route profiles", async () => {
    render(
      <MemoryRouter>
        <Routes />
      </MemoryRouter>
    );

    expect(await screen.findByText("routes.no_profiles")).toBeInTheDocument();
  });

  it("loads first profile detail and toggles mode", async () => {
    vi.mocked(api.listRouteProfiles).mockResolvedValue([profile("r1")]);

    render(
      <MemoryRouter>
        <Routes />
      </MemoryRouter>
    );

    await waitFor(() => expect(api.getRouteProfile).toHaveBeenCalledWith("r1"));
    expect(screen.getAllByText("Default Route").length).toBeGreaterThanOrEqual(
      1
    );

    const failover = screen.getByText("routes.mode_failover");
    await act(async () => failover.click());

    await waitFor(() =>
      expect(api.setRouteProfileMode).toHaveBeenCalledWith("r1", "failover")
    );
  });

  it("creates and renames a route profile", async () => {
    vi.mocked(api.listRouteProfiles).mockResolvedValue([profile("r1")]);

    render(
      <MemoryRouter>
        <Routes />
      </MemoryRouter>
    );

    expect(
      (await screen.findAllByText("Default Route")).length
    ).toBeGreaterThan(0);
    await act(async () => screen.getByText("routes.create_profile").click());

    const nameInput = screen.getByPlaceholderText("My Route");
    await act(async () => {
      fireEvent.change(nameInput, { target: { value: "Custom Route" } });
    });
    await act(async () => screen.getByText("common.save").click());

    await waitFor(() =>
      expect(api.createRouteProfile).toHaveBeenCalledWith(
        expect.objectContaining({ name: "Custom Route" })
      )
    );

    await act(async () => screen.getByTitle("routes.rename_profile").click());
    const renameInput = screen.getByDisplayValue("Default Route");
    await act(async () => {
      fireEvent.change(renameInput, { target: { value: "Renamed Route" } });
    });
    const saveRename = renameInput.parentElement!.querySelector("button")!;
    await act(async () => saveRename.click());

    await waitFor(() =>
      expect(api.updateRouteProfile).toHaveBeenCalledWith("r1", {
        name: "Renamed Route",
      })
    );
  });

  it("adds, removes, and deletes route providers with confirmation for profile delete", async () => {
    vi.mocked(api.listRouteProfiles).mockResolvedValue([profile("r1")]);
    vi.mocked(api.listProviders).mockResolvedValue([
      {
        id: "p2",
        name: "Fallback Provider",
        provider_type: "openai",
        protocol: JSON.stringify(["openai_responses"]),
      },
    ] as any);

    render(
      <MemoryRouter>
        <Routes />
      </MemoryRouter>
    );

    expect((await screen.findAllByText("OpenAI")).length).toBeGreaterThan(0);
    const addSelect = screen.getByDisplayValue(
      "routes.add_provider"
    ) as HTMLSelectElement;
    await act(async () => {
      fireEvent.change(addSelect, { target: { value: "p2" } });
    });
    await act(async () => screen.getByText("routes.add").click());
    await waitFor(() =>
      expect(api.addProviderToRoute).toHaveBeenCalledWith("r1", "p2", {})
    );

    await act(async () => screen.getByTitle("common.delete").click());
    await waitFor(() =>
      expect(api.removeProviderFromRoute).toHaveBeenCalledWith("r1", "p1")
    );

    await act(async () => screen.getByTitle("routes.delete_profile").click());
    expect(await screen.findByText("routes.delete_title")).toBeInTheDocument();
    const deleteButtons = screen.getAllByRole("button", {
      name: "common.delete",
    });
    const confirmDelete = deleteButtons[deleteButtons.length - 1];
    await act(async () => confirmDelete.click());
    await waitFor(() =>
      expect(api.deleteRouteProfile).toHaveBeenCalledWith("r1")
    );
  });
});
