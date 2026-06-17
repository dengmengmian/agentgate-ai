import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import { ToolCard } from "./ToolCard";
import { renderWithProviders } from "@/components/test-utils";
import type { ToolConfigView } from "@/types/tool";

function makeTool(overrides: Partial<ToolConfigView> = {}): ToolConfigView {
  return {
    id: "codex",
    name: "Codex",
    description: "OpenAI CLI coding agent",
    icon: "terminal",
    config_exists: true,
    config_path: "~/.codex/config.toml",
    ...overrides,
  } as ToolConfigView;
}

describe("ToolCard", () => {
  it("renders configured tool info", () => {
    renderWithProviders(<ToolCard tool={makeTool()} />);

    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(screen.getByText("OpenAI CLI coding agent")).toBeInTheDocument();
    expect(screen.getByText("Config found")).toBeInTheDocument();
    expect(screen.getByText("~/.codex/config.toml")).toBeInTheDocument();
  });

  it("renders missing config state and falls back icon", () => {
    renderWithProviders(
      <ToolCard tool={makeTool({ config_exists: false, icon: "unknown" })} />
    );

    expect(screen.getByText("No config")).toBeInTheDocument();
  });
});
