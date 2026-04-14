import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, beforeEach, vi } from "vitest";
import Routines from "./Routines";
import { useRoutineStore } from "../stores/routineStore";
import { useAuthStore } from "../stores/authStore";
import { seedRoutines } from "../test/mocks/handlers";

function renderPage() {
  return render(
    <MemoryRouter>
      <Routines />
    </MemoryRouter>,
  );
}

describe("Routines page", () => {
  beforeEach(() => {
    useRoutineStore.setState({ routines: [], loading: false, error: null });
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "token-1",
      refresh_token: "refresh-1",
    });
  });

  it("renders routines list from the API on mount", async () => {
    seedRoutines([
      { id: "r1", name: "Morning flow", period: "weekly", is_active: true },
      { id: "r2", name: "Travel week", period: "custom", is_active: false },
    ]);

    renderPage();

    expect(await screen.findByText("Morning flow")).toBeInTheDocument();
    expect(screen.getByText("Travel week")).toBeInTheDocument();
    expect(screen.getByTestId("active-badge-r1")).toBeInTheDocument();
    expect(screen.queryByTestId("active-badge-r2")).not.toBeInTheDocument();
  });

  it("creates a new routine via the form", async () => {
    const user = userEvent.setup();
    renderPage();

    // Wait for initial fetch to settle
    expect(
      await screen.findByText(/you don't have any routines/i),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /new routine/i }));

    const form = screen.getByRole("form", { name: /create routine form/i });
    await user.type(within(form).getByLabelText(/name/i), "Deep focus week");
    await user.click(within(form).getByRole("button", { name: /^create$/i }));

    expect(await screen.findByText("Deep focus week")).toBeInTheDocument();
  });

  it("activates a routine and updates the active badge", async () => {
    seedRoutines([
      { id: "r1", name: "Alpha", period: "weekly", is_active: true },
      { id: "r2", name: "Beta", period: "weekly", is_active: false },
    ]);

    const user = userEvent.setup();
    renderPage();

    await screen.findByText("Alpha");
    expect(screen.getByTestId("active-badge-r1")).toBeInTheDocument();
    expect(screen.queryByTestId("active-badge-r2")).not.toBeInTheDocument();

    const betaRow = screen.getByTestId("routine-r2");
    await user.click(
      within(betaRow).getByRole("button", { name: /activate/i }),
    );

    expect(await screen.findByTestId("active-badge-r2")).toBeInTheDocument();
    expect(screen.queryByTestId("active-badge-r1")).not.toBeInTheDocument();
  });

  it("deletes a routine after confirmation", async () => {
    seedRoutines([
      { id: "r1", name: "Alpha", period: "weekly", is_active: true },
    ]);

    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("Alpha");
    await user.click(screen.getByRole("button", { name: /delete/i }));

    expect(
      await screen.findByText(/you don't have any routines/i),
    ).toBeInTheDocument();

    confirmSpy.mockRestore();
  });

  it("renames a routine via the prompt", async () => {
    seedRoutines([
      { id: "r1", name: "Alpha", period: "weekly", is_active: true },
    ]);

    const promptSpy = vi.spyOn(window, "prompt").mockReturnValue("Alpha v2");
    const user = userEvent.setup();
    renderPage();

    await screen.findByText("Alpha");
    await user.click(screen.getByRole("button", { name: /edit name/i }));

    expect(await screen.findByText("Alpha v2")).toBeInTheDocument();
    promptSpy.mockRestore();
  });
});
