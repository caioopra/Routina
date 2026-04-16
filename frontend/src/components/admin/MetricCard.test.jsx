import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import MetricCard from "./MetricCard";

describe("MetricCard", () => {
  it("renders the label and value", () => {
    render(<MetricCard label="Total Users" value={42} />);
    expect(screen.getByText("Total Users")).toBeInTheDocument();
    expect(screen.getByText("42")).toBeInTheDocument();
  });

  it("renders a string value", () => {
    render(<MetricCard label="Active Provider" value="gemini" />);
    expect(screen.getByText("gemini")).toBeInTheDocument();
  });

  it("renders the optional subtitle when provided", () => {
    render(
      <MetricCard label="Monthly Cost" value="$0.05" subtitle="last 30 days" />,
    );
    expect(screen.getByText("last 30 days")).toBeInTheDocument();
  });

  it("does not render a subtitle element when subtitle is omitted", () => {
    render(<MetricCard label="Chat Status" value="Enabled" />);
    expect(screen.queryByText("last 30 days")).not.toBeInTheDocument();
    // The label and value are still present
    expect(screen.getByText("Chat Status")).toBeInTheDocument();
    expect(screen.getByText("Enabled")).toBeInTheDocument();
  });
});
