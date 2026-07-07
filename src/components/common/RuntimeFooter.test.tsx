import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { RuntimeFooter } from "./RuntimeFooter";
import { renderWithProviders } from "@/components/test-utils";
import * as api from "@/lib/api";

vi.mock("@/lib/api");

describe("RuntimeFooter", () => {
  beforeEach(() => {
    vi.mocked(api.getRuntimeKpis).mockResolvedValue({
      active_requests: 0,
      uptime_seconds: 7500,
      total_requests: 25,
      total_tokens: 1377200000,
      total_cost: 4954.83,
      success_rate_lifetime: 56,
      gateway_running: true,
    } as any);
  });

  it("renders runtime strip", async () => {
    renderWithProviders(<RuntimeFooter />);

    await waitFor(() => expect(api.getRuntimeKpis).toHaveBeenCalled());
    expect(screen.getByText("Runtime Strip")).toBeInTheDocument();
    expect(screen.getByText("25")).toBeInTheDocument();
  });
});
