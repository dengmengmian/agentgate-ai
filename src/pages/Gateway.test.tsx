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
vi.mock("@/components/common/Toast", () => ({
  toast: vi.fn(),
}));

import * as api from "@/lib/api";
import { toast } from "@/components/common/Toast";
import { Gateway } from "./Gateway";
import { __resetGlobalStoresForTest } from "@/store/global";

afterEach(() => cleanup());

function gatewayStatus(): any {
  return {
    running: true,
    host: "127.0.0.1",
    port: 4141,
    input_protocol: "openai_responses",
    output_protocol: "openai_chat_completions",
    active_provider: "OpenAI",
    started_at: "2026-06-16T00:00:00Z",
  };
}

function gatewaySettings(): any {
  return {
    host: "127.0.0.1",
    port: 4141,
    input_protocol: "openai_responses",
    output_protocol: "openai_chat_completions",
    auto_start: true,
    log_retention_days: 30,
  };
}

describe("Gateway", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.mocked(api.getGatewayStatus).mockResolvedValue(gatewayStatus());
    vi.mocked(api.getGatewaySettings).mockResolvedValue(gatewaySettings());
    vi.mocked(api.updateGatewaySettings).mockResolvedValue(gatewaySettings());
    vi.mocked(api.startGateway).mockResolvedValue(gatewayStatus());
    vi.mocked(api.stopGateway).mockResolvedValue({
      ...gatewayStatus(),
      running: false,
    });
    vi.mocked(api.restartGateway).mockResolvedValue(gatewayStatus());
  });

  it("renders gateway status and settings", async () => {
    render(
      <MemoryRouter>
        <Gateway />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(api.getGatewayStatus).toHaveBeenCalled();
      expect(api.getGatewaySettings).toHaveBeenCalled();
    });

    expect(screen.getByText("gateway.stop")).toBeInTheDocument();
  });

  it("saves settings when save is clicked", async () => {
    render(
      <MemoryRouter>
        <Gateway />
      </MemoryRouter>
    );

    await screen.findByText("gateway.configuration");

    const portInput = screen.getByDisplayValue("4141") as HTMLInputElement;
    fireEvent.change(portInput, { target: { value: "8080" } });

    const save = screen.getByText("gateway.save");
    await act(async () => save.click());

    await waitFor(() =>
      expect(api.updateGatewaySettings).toHaveBeenCalledWith(
        expect.objectContaining({ port: 8080 })
      )
    );
  });

  it("starts, stops, and restarts the gateway", async () => {
    render(
      <MemoryRouter>
        <Gateway />
      </MemoryRouter>
    );

    await screen.findByText("gateway.stop");
    await act(async () => screen.getByText("gateway.stop").click());
    await waitFor(() => expect(api.stopGateway).toHaveBeenCalled());

    await screen.findByText("gateway.start");
    await act(async () => screen.getByText("gateway.start").click());
    await waitFor(() => expect(api.startGateway).toHaveBeenCalled());

    await screen.findByText("gateway.restart");
    await act(async () => screen.getByText("gateway.restart").click());
    await waitFor(() => expect(api.restartGateway).toHaveBeenCalled());
  });

  it("shows an error when a gateway action fails", async () => {
    vi.mocked(api.stopGateway).mockRejectedValue(new Error("stop failed"));

    render(
      <MemoryRouter>
        <Gateway />
      </MemoryRouter>
    );

    await screen.findByText("gateway.stop");
    await act(async () => screen.getByText("gateway.stop").click());

    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith("error", "stop failed")
    );
  });
});
