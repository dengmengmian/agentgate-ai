import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import { Outlet } from "react-router-dom";

vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual("react-router-dom");
  return {
    ...(actual as object),
    BrowserRouter: (actual as { MemoryRouter: typeof Outlet }).MemoryRouter,
  };
});

const eventListeners: Record<string, (payload?: unknown) => void> = {};

vi.mock("@/lib/bindings", () => ({
  events: Object.fromEntries(
    ["petOpenSettings", "petOpenGateway", "petOpenLogs"].map((key) => [
      key,
      {
        listen: vi.fn((cb: (e: { payload: unknown }) => void) => {
          eventListeners[key] = (payload?: unknown) => cb({ payload });
          return Promise.resolve(() => {});
        }),
      },
    ])
  ),
}));

vi.mock("@/components/layout/AppShell", () => ({
  AppShell: () => (
    <div data-testid="app-shell">
      <Outlet />
    </div>
  ),
}));

vi.mock("@/components/common/Toast", () => ({
  ToastContainer: () => <div data-testid="toast-container" />,
}));

vi.mock("@/components/common/UpdateChecker", () => ({
  UpdateChecker: () => <div data-testid="update-checker" />,
}));

vi.mock("@/pages/Dashboard", () => ({
  Dashboard: () => <div>Dashboard Page</div>,
}));

vi.mock("@/pages/Settings", () => ({
  Settings: () => <div>Settings Page</div>,
}));

vi.mock("@/pages/Gateway", () => ({
  Gateway: () => <div>Gateway Page</div>,
}));

import { App } from "./App";

describe("App", () => {
  it("renders the app shell and default dashboard route", () => {
    render(<App />);
    expect(screen.getByTestId("app-shell")).toBeInTheDocument();
    expect(screen.getByText("Dashboard Page")).toBeInTheDocument();
    expect(screen.getByTestId("toast-container")).toBeInTheDocument();
    expect(screen.getByTestId("update-checker")).toBeInTheDocument();
  });

  it("navigates to settings when petOpenSettings event fires", async () => {
    render(<App />);
    act(() => eventListeners.petOpenSettings?.());
    await waitFor(() => {
      expect(screen.getByText("Settings Page")).toBeInTheDocument();
    });
  });

  it("navigates to gateway when petOpenGateway event fires", async () => {
    render(<App />);
    act(() => eventListeners.petOpenGateway?.());
    await waitFor(() => {
      expect(screen.getByText("Gateway Page")).toBeInTheDocument();
    });
  });
});
