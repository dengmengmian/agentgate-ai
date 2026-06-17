import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import { Activity } from "lucide-react";
import { MetricCard } from "./MetricCard";
import { renderWithProviders } from "@/components/test-utils";

describe("MetricCard", () => {
  it("renders label, value and trend", () => {
    renderWithProviders(
      <MetricCard label="Requests" value={1234} icon={Activity} trend="+12%" />
    );

    expect(screen.getByText("Requests")).toBeInTheDocument();
    expect(screen.getByText("1234")).toBeInTheDocument();
    expect(screen.getByText("+12%")).toBeInTheDocument();
  });

  it("renders without a trend when omitted", () => {
    renderWithProviders(
      <MetricCard label="Latency" value="45ms" icon={Activity} />
    );

    expect(screen.getByText("Latency")).toBeInTheDocument();
    expect(screen.getByText("45ms")).toBeInTheDocument();
    expect(screen.queryByText("+12%")).not.toBeInTheDocument();
  });
});
