import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, fireEvent, act } from "@testing-library/react";
import { Sidebar } from "./Sidebar";
import { renderWithProviders } from "@/components/test-utils";
import { useProviders, __resetGlobalStoresForTest } from "@/store/global";
import type { ProviderView } from "@/types/provider";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("1.4.4"),
}));

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    listProviders: vi.fn().mockResolvedValue([]),
  };
});

const sampleProvider: ProviderView = {
  id: "p1",
  name: "DeepSeek",
  provider_type: "deepseek",
} as ProviderView;

describe("Sidebar", () => {
  beforeEach(() => {
    __resetGlobalStoresForTest();
    localStorage.clear();
    vi.clearAllMocks();
  });

  it("renders navigation links and app version", async () => {
    useProviders.setState({
      items: [sampleProvider],
      loading: false,
      error: null,
    });

    renderWithProviders(<Sidebar />);

    expect(screen.getByText("Overview")).toBeInTheDocument();
    expect(screen.getByText("Providers")).toBeInTheDocument();
    expect(screen.getByText("Clients")).toBeInTheDocument();
    expect(await screen.findByText("v1.4.4")).toBeInTheDocument();
  });

  it("toggles collapsed state", async () => {
    useProviders.setState({
      items: [sampleProvider],
      loading: false,
      error: null,
    });

    renderWithProviders(<Sidebar />);

    const collapseBtn = screen.getByTitle(/Collapse sidebar/i);
    await act(async () => {
      fireEvent.click(collapseBtn);
    });

    expect(screen.queryByText("Overview")).not.toBeInTheDocument();
    expect(screen.getByTitle(/Expand sidebar/i)).toBeInTheDocument();
    expect(localStorage.getItem("agentgate_sidebar_collapsed")).toBe("1");

    const expandBtn = screen.getByTitle(/Expand sidebar/i);
    await act(async () => {
      fireEvent.click(expandBtn);
    });

    expect(screen.getByText("Overview")).toBeInTheDocument();
    expect(screen.getByTitle(/Collapse sidebar/i)).toBeInTheDocument();
    expect(localStorage.getItem("agentgate_sidebar_collapsed")).toBe("0");
  });

  it("shows quick setup banner when no providers are configured", async () => {
    localStorage.setItem("agentgate_show_quick_setup", "1");
    useProviders.setState({ items: [], loading: true, error: null });

    await act(async () => {
      renderWithProviders(<Sidebar />);
    });

    expect(screen.getByText(/Quick Setup/i)).toBeInTheDocument();
  });
});
