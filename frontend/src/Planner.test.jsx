import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { MemoryRouter } from "react-router-dom";
import Planner from "./Planner";

function renderPlanner() {
  return render(
    <MemoryRouter>
      <Planner />
    </MemoryRouter>,
  );
}

describe("Planner", () => {
  it("renders the title from rotina.json", () => {
    renderPlanner();
    expect(screen.getByText("Rotina Semanal")).toBeInTheDocument();
  });

  it("renders the day selector pills", () => {
    renderPlanner();
    expect(screen.getByText("Seg")).toBeInTheDocument();
    expect(screen.getByText("Dom")).toBeInTheDocument();
  });

  it("renders a link to the Routines page", () => {
    renderPlanner();
    const link = screen.getByRole("link", { name: /minhas rotinas/i });
    expect(link).toHaveAttribute("href", "/routines");
  });
});
