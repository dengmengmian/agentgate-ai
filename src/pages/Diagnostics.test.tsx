import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, act, waitFor, screen, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/lib/api");

import * as api from "@/lib/api";
import { Diagnostics } from "./Diagnostics";

afterEach(() => cleanup());

describe("Diagnostics", () => {
  beforeEach(() => {
    vi.mocked(api.runFullSelfTest).mockResolvedValue({
      overall_status: "ok",
      summary: "All checks passed",
      created_at: "2026-06-16T00:00:00Z",
      reports: [
        {
          name: "Gateway",
          status: "ok",
          summary: "Gateway is healthy",
          checks: [],
        },
      ],
    } as any);
    vi.mocked(api.exportDiagnosticBundle).mockResolvedValue({
      success: true,
      path: "/tmp/diagnostics.zip",
    } as any);
    vi.mocked(api.openAppDataDir).mockResolvedValue(true);
  });

  it("runs self test and displays report", async () => {
    render(
      <MemoryRouter>
        <Diagnostics />
      </MemoryRouter>
    );

    const run = screen.getByText("diag.run_self_test");
    await act(async () => run.click());

    await waitFor(() => expect(api.runFullSelfTest).toHaveBeenCalled());
    expect(screen.getByText("All checks passed")).toBeInTheDocument();
  });

  it("exports diagnostic bundle", async () => {
    render(
      <MemoryRouter>
        <Diagnostics />
      </MemoryRouter>
    );

    const exportBtn = screen.getByText("diag.export_bundle");
    await act(async () => exportBtn.click());

    await waitFor(() =>
      expect(api.exportDiagnosticBundle).toHaveBeenCalledWith(true, 50)
    );
  });
});
