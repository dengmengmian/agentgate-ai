import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, fireEvent, act } from "@testing-library/react";
import { Topbar } from "./Topbar";
import { renderWithProviders } from "@/components/test-utils";
import * as api from "@/lib/api";
import { useGatewayStatus, __resetGlobalStoresForTest } from "@/store/global";
import type { GatewayStatus } from "@/types/gateway";

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getGatewayStatus: vi.fn().mockResolvedValue({
      running: true,
      host: "127.0.0.1",
      port: 8080,
    } as GatewayStatus),
  };
});

function makeStatus(overrides: Partial<GatewayStatus> = {}): GatewayStatus {
  return {
    running: true,
    host: "127.0.0.1",
    port: 8080,
    ...overrides,
  } as GatewayStatus;
}

describe("Topbar", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    vi.clearAllMocks();
  });

  it("renders page title and running gateway status", async () => {
    useGatewayStatus.setState({
      value: makeStatus(),
      loading: false,
      error: null,
    });

    renderWithProviders(<Topbar />, { route: "/providers" });

    expect(await screen.findByText("Providers")).toBeInTheDocument();
    expect(screen.getByText("Running")).toBeInTheDocument();
    expect(screen.getByText("127.0.0.1:8080")).toBeInTheDocument();
  });

  it("shows diagnostics shortcut when gateway is stopped", async () => {
    vi.mocked(api.getGatewayStatus).mockResolvedValue(
      makeStatus({ running: false })
    );
    useGatewayStatus.setState({
      value: makeStatus({ running: false }),
      loading: false,
      error: null,
    });

    renderWithProviders(<Topbar />, { route: "/" });

    expect(await screen.findByText("Stopped")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Diagnostics/i })
    ).toBeInTheDocument();
  });

  it("calls onOpenCmdK when the command palette button is clicked", async () => {
    useGatewayStatus.setState({ value: null, loading: false, error: null });
    const onOpenCmdK = vi.fn();

    renderWithProviders(<Topbar onOpenCmdK={onOpenCmdK} />, { route: "/" });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /Ctrl\+K/i }));
    });

    expect(onOpenCmdK).toHaveBeenCalled();
  });
});
