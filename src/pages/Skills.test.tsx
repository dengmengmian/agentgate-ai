import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, screen, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";

vi.mock("@/lib/api");

import * as api from "@/lib/api";
import { Skills } from "./Skills";

afterEach(() => cleanup());

function skill(id: string, source: string): api.Skill {
  return {
    id,
    source,
    name: id,
    description: "Local skill",
    enabled: true,
    path: `/tmp/${source}/${id}`,
  };
}

describe("Skills", () => {
  beforeEach(() => {
    vi.mocked(api.listSkills).mockResolvedValue([
      skill("writer", "claude"),
      skill("reviewer", "codex"),
    ]);
    vi.mocked(api.exportSkills).mockResolvedValue({
      version: 1,
      skills: [],
      skipped_files: [],
    });
    vi.mocked(api.importSkills).mockResolvedValue([]);
    vi.mocked(api.setSkillEnabled).mockImplementation(async (source, id) =>
      skill(id, source)
    );
    vi.mocked(api.deleteSkill).mockResolvedValue(true);
  });

  it("renders skill console and source matrix", async () => {
    render(
      <MemoryRouter>
        <Skills />
      </MemoryRouter>
    );

    await waitFor(() => expect(api.listSkills).toHaveBeenCalled());
    expect(screen.getByText("skills.console")).toBeInTheDocument();
    expect(screen.getByText("skills.source_matrix")).toBeInTheDocument();
    expect(screen.getByText("writer")).toBeInTheDocument();
  });
});
