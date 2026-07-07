import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, act, waitFor, screen, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/lib/api");

import * as api from "@/lib/api";
import { Dashboard } from "./Dashboard";
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

describe("Dashboard", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.mocked(api.listTools).mockResolvedValue([]);
    vi.mocked(api.listRequestLogs).mockResolvedValue([]);
    vi.mocked(api.getRequestStatsRange).mockResolvedValue({ total: 0 } as any);
    vi.mocked(api.aggregateCostByModel).mockResolvedValue([]);
    vi.mocked(api.aggregateCostByClient).mockResolvedValue([]);
    vi.mocked(api.aggregateRouteProfileStats).mockResolvedValue([]);
    vi.mocked(api.getGatewayStatus).mockResolvedValue(gatewayStatus());
    vi.mocked(api.getGatewaySettings).mockResolvedValue(gatewaySettings());
    vi.mocked(api.getRuntimeKpis).mockResolvedValue({
      active_requests: 0,
      uptime_seconds: 0,
      total_requests: 0,
      total_tokens: 0,
      total_cost: 0,
      success_rate_lifetime: 100,
      gateway_running: false,
    } as any);
    vi.mocked(api.listProviders).mockResolvedValue([]);
    vi.mocked(api.listRouteProfiles).mockResolvedValue([]);
    vi.mocked(api.startGateway).mockResolvedValue(gatewayStatus());
    vi.mocked(api.stopGateway).mockResolvedValue({
      ...gatewayStatus(),
      running: false,
    });
    vi.mocked(api.restartGateway).mockResolvedValue(gatewayStatus());
  });

  it("renders and fetches initial data", async () => {
    vi.mocked(api.listProviders).mockResolvedValue([{ id: "p1" }] as any);
    vi.mocked(api.getRequestStatsRange).mockResolvedValue({
      total: 1,
      today_total: 1,
      today_errors: 0,
      today_input_tokens: 100,
      today_output_tokens: 50,
      today_cost: 0.01,
      avg_latency_ms: 1000,
      today_codex_compact: 0,
      today_cache_read_tokens: 0,
      today_cache_write_tokens: 0,
      daily: [
        {
          date: "2026-07-07",
          total: 1,
          errors: 0,
          input_tokens: 100,
          output_tokens: 50,
        },
      ],
      providers: [{ name: "OpenAI", count: 1 }],
    } as any);

    render(
      <MemoryRouter>
        <Dashboard />
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(api.listTools).toHaveBeenCalled();
      expect(api.listRequestLogs).toHaveBeenCalledWith({ limit: 5 });
      expect(api.getRequestStatsRange).toHaveBeenCalledWith(7);
    });
    expect(screen.getByText("dashboard.control_console")).toBeInTheDocument();
    expect(screen.getByText("stats.today_realtime")).toBeInTheDocument();
    expect(screen.getByText("stats.traffic_monitor")).toBeInTheDocument();
  });

  it("stops gateway when stop button is clicked", async () => {
    render(
      <MemoryRouter>
        <Dashboard />
      </MemoryRouter>
    );

    const stop = await screen.findByText("dashboard.stop");
    await act(async () => stop.click());
    await waitFor(() => expect(api.stopGateway).toHaveBeenCalled());
  });
});
