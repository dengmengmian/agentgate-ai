import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, waitFor, cleanup } from "@testing-library/react";
import * as api from "@/lib/api";

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    startDragging: vi.fn(),
    setIgnoreCursorEvents: vi.fn(() => Promise.resolve()),
    setSize: vi.fn(() => Promise.resolve()),
    setPosition: vi.fn(() => Promise.resolve()),
    scaleFactor: vi.fn(() => Promise.resolve(1)),
    outerSize: vi.fn(() => Promise.resolve({ width: 200, height: 200 })),
    outerPosition: vi.fn(() => Promise.resolve({ x: 0, y: 0 })),
    onMoved: vi.fn(() => Promise.resolve(() => {})),
  })),
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number
    ) {}
  },
  LogicalPosition: class {
    constructor(
      public x: number,
      public y: number
    ) {}
  },
}));

vi.mock("@/lib/api", () => ({
  getPetSettings: vi.fn(() => Promise.resolve({ pet_type: "robot" })),
  updatePetSettings: vi.fn(() => Promise.resolve({})),
  getPetGatewayState: vi.fn(() =>
    Promise.resolve({ state: "stopped", today: { requests: 0, cost: 0 } })
  ),
  getPetGatewayStateLite: vi.fn(() => Promise.resolve({ state: "stopped" })),
  getPetMemory: vi.fn(() => Promise.resolve("{}")),
  savePetMemory: vi.fn(() => Promise.resolve(true)),
  petChat: vi.fn(() => Promise.resolve("hello")),
  getPetClickThrough: vi.fn(() => Promise.resolve(false)),
  showPetContextMenu: vi.fn(() => Promise.resolve(null)),
}));

vi.mock("@/lib/bindings", () => ({
  events: Object.fromEntries(
    [
      "petSettingsChanged",
      "petBubble",
      "petGatewayStateChanged",
      "petMemoryReset",
      "petClickThroughChanged",
    ].map((key) => [key, { listen: vi.fn(() => Promise.resolve(() => {})) }])
  ),
}));

import { PetApp } from "./PetApp";

describe("PetApp", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it("renders the pet container and initial robot pet", async () => {
    const { container } = render(<PetApp />);
    await waitFor(() => {
      expect(container.querySelector(".pet-container")).toBeInTheDocument();
    });
    expect(container.querySelector("svg")).toBeInTheDocument();
  });

  it("loads saved pet settings on mount", async () => {
    vi.mocked(api.getPetSettings).mockResolvedValueOnce({
      pet_type: "fox",
    } as any);
    const { container } = render(<PetApp />);
    await waitFor(() => {
      expect(api.getPetSettings).toHaveBeenCalled();
    });
    expect(container.querySelector(".pet-container")).toBeInTheDocument();
  });
});
