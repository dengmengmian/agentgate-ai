import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { StatusBadge } from "./StatusBadge";

describe("StatusBadge", () => {
  it("renders children", () => {
    render(<StatusBadge variant="success">Online</StatusBadge>);
    expect(screen.getByText("Online")).toBeInTheDocument();
  });

  it("applies success styles", () => {
    const { container } = render(
      <StatusBadge variant="success">OK</StatusBadge>
    );
    const badge = container.querySelector("span");
    expect(badge?.className).toContain("bg-success-soft");
    expect(badge?.className).toContain("text-success");
  });

  it("applies error styles", () => {
    const { container } = render(
      <StatusBadge variant="error">Fail</StatusBadge>
    );
    const badge = container.querySelector("span");
    expect(badge?.className).toContain("bg-error-soft");
    expect(badge?.className).toContain("text-error");
  });

  it("applies warning styles", () => {
    const { container } = render(
      <StatusBadge variant="warning">Warn</StatusBadge>
    );
    const badge = container.querySelector("span");
    expect(badge?.className).toContain("bg-warning-soft");
    expect(badge?.className).toContain("text-warning");
  });

  it("applies muted styles", () => {
    const { container } = render(
      <StatusBadge variant="muted">Off</StatusBadge>
    );
    const badge = container.querySelector("span");
    expect(badge?.className).toContain("bg-hover");
    expect(badge?.className).toContain("text-text-muted");
  });

  it("applies accent styles", () => {
    const { container } = render(
      <StatusBadge variant="accent">Info</StatusBadge>
    );
    const badge = container.querySelector("span");
    expect(badge?.className).toContain("bg-accent-soft");
    expect(badge?.className).toContain("text-accent");
  });

  it("merges custom className", () => {
    const { container } = render(
      <StatusBadge variant="success" className="custom-class">
        OK
      </StatusBadge>
    );
    const badge = container.querySelector("span");
    expect(badge?.className).toContain("custom-class");
  });
});
