import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import Planner from "./Planner";

describe("Planner", () => {
  it("renders the title from rotina.json", () => {
    render(<Planner />);
    expect(screen.getByText("Rotina Semanal")).toBeInTheDocument();
  });

  it("renders the day selector pills", () => {
    render(<Planner />);
    expect(screen.getByText("Seg")).toBeInTheDocument();
    expect(screen.getByText("Dom")).toBeInTheDocument();
  });
});
