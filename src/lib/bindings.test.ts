import { describe, it, expect, vi, beforeAll } from "vitest";

const invokeMock = vi.fn();
const eventListenMock = vi.fn(() => Promise.resolve(() => {}));
const eventOnceMock = vi.fn(() => Promise.resolve(() => {}));
const eventEmitMock = vi.fn(() => Promise.resolve());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: eventListenMock,
  once: eventOnceMock,
  emit: eventEmitMock,
}));

let commands: typeof import("./bindings").commands;
let events: typeof import("./bindings").events;

beforeAll(async () => {
  const mod = await import("./bindings");
  commands = mod.commands;
  events = mod.events;
});

describe("generated bindings", () => {
  it("exports a commands object that invokes the backend", async () => {
    invokeMock.mockResolvedValue({ running: true });

    const result = await commands.getGatewayStatus();

    expect(invokeMock).toHaveBeenCalledWith("get_gateway_status");
    expect(result).toEqual({ status: "ok", data: { running: true } });
  });

  it("exports typed event helpers with listen methods", () => {
    expect(events.petBubble).toBeDefined();
    expect(events.petSettingsChanged).toBeDefined();
    expect(events.petGatewayStateChanged).toBeDefined();
    expect(typeof events.petOpenSettings.listen).toBe("function");
  });

  it("event listen calls the tauri event listener", () => {
    const handler = vi.fn();
    events.petBubble.listen(handler);
    expect(eventListenMock).toHaveBeenCalledWith("pet-bubble", handler);
  });
});
