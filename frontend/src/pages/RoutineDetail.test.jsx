import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { describe, it, expect, beforeEach, vi } from "vitest";
import RoutineDetail from "./RoutineDetail";
import { useRoutineStore } from "../stores/routineStore";
import { useBlockStore } from "../stores/blockStore";
import { useLabelStore } from "../stores/labelStore";
import { useRuleStore } from "../stores/ruleStore";
import { useAuthStore } from "../stores/authStore";
import {
  seedRoutines,
  seedBlocks,
  seedLabels,
  seedRules,
} from "../test/mocks/handlers";

const ROUTINE_ID = "routine-42";

function resetStores() {
  useRoutineStore.setState({ routines: [], loading: false, error: null });
  useBlockStore.setState({ byRoutine: {} });
  useLabelStore.setState({ labels: [], loading: false, error: null });
  useRuleStore.setState({ byRoutine: {} });
}

function renderPage(routineId = ROUTINE_ID) {
  return render(
    <MemoryRouter initialEntries={[`/routines/${routineId}`]}>
      <Routes>
        <Route path="/routines/:id" element={<RoutineDetail />} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("RoutineDetail page", () => {
  beforeEach(() => {
    resetStores();
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "token-1",
      refresh_token: "refresh-1",
    });
    seedRoutines([
      {
        id: ROUTINE_ID,
        name: "Deep Work Week",
        period: "weekly",
        is_active: true,
      },
    ]);
  });

  it("renders the routine name from store", async () => {
    renderPage();
    // findAllByText handles the case where the name appears in breadcrumb + heading
    expect(await screen.findAllByText("Deep Work Week")).not.toHaveLength(0);
  });

  it("renders all 7 day columns on desktop", async () => {
    renderPage();
    await screen.findAllByText("Deep Work Week");
    for (let i = 0; i < 7; i++) {
      expect(screen.getByTestId(`day-col-${i}`)).toBeInTheDocument();
    }
  });

  it("shows empty state when no blocks for a day", async () => {
    renderPage();
    await screen.findAllByText("Deep Work Week");
    const empties = screen.getAllByText(/empty/i);
    expect(empties.length).toBeGreaterThan(0);
  });

  it("renders blocks grouped by day_of_week", async () => {
    seedBlocks([
      {
        id: "b1",
        routine_id: ROUTINE_ID,
        day_of_week: 0,
        start_time: "09:00",
        end_time: "12:00",
        title: "Morning work",
        type: "trabalho",
        note: null,
        labels: [],
        subtasks: [],
        sort_order: 0,
      },
      {
        id: "b2",
        routine_id: ROUTINE_ID,
        day_of_week: 2,
        start_time: "14:00",
        end_time: "16:00",
        title: "Research",
        type: "mestrado",
        note: null,
        labels: [],
        subtasks: [],
        sort_order: 0,
      },
    ]);

    renderPage();
    await screen.findAllByText("Deep Work Week");
    // Check within the desktop grid columns (data-testid day-col-N)
    expect(await screen.findAllByText("Morning work")).not.toHaveLength(0);
    expect(screen.getAllByText("Research")).not.toHaveLength(0);

    // Morning work should be in day-col-0
    expect(
      within(screen.getByTestId("day-col-0")).getByText("Morning work"),
    ).toBeInTheDocument();
    // Research in day-col-2
    expect(
      within(screen.getByTestId("day-col-2")).getByText("Research"),
    ).toBeInTheDocument();
  });

  it("opens BlockModal when Add button is clicked", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findAllByText("Deep Work Week");

    const addButtons = screen.getAllByRole("button", { name: /add block to/i });
    await user.click(addButtons[0]);

    expect(
      screen.getByRole("dialog", { name: /add block/i }),
    ).toBeInTheDocument();
  });

  it("creates a block and it appears in the grid", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findAllByText("Deep Work Week");

    const addButtons = screen.getAllByRole("button", {
      name: /add block to monday/i,
    });
    await user.click(addButtons[0]);

    const form = screen.getByRole("form", { name: /add block form/i });
    await user.type(within(form).getByLabelText(/title/i), "New block");
    await user.type(within(form).getByLabelText(/start time/i), "08:00");
    await user.click(screen.getByRole("button", { name: /^add block$/i }));

    // Block appears in at least one place (desktop grid + mobile view both render in jsdom)
    expect(await screen.findAllByText("New block")).not.toHaveLength(0);
  });

  it("renders rules in the Rules tab", async () => {
    seedRules([
      {
        id: "rule-1",
        routine_id: ROUTINE_ID,
        text: "Sleep by 11pm",
        sort_order: 0,
      },
    ]);

    renderPage();
    await screen.findAllByText("Deep Work Week");
    expect(await screen.findByText("Sleep by 11pm")).toBeInTheDocument();
  });

  it("renders labels in the Labels tab", async () => {
    seedLabels([
      {
        id: "l1",
        name: "Focus",
        color_bg: "#1e3a5f",
        color_text: "#93c5fd",
        color_border: "#2563eb",
        is_default: false,
      },
    ]);

    const user = userEvent.setup();
    renderPage();
    await screen.findAllByText("Deep Work Week");

    await user.click(screen.getByRole("button", { name: /labels/i }));
    expect(await screen.findByTestId("label-l1")).toBeInTheDocument();
  });

  it("shows active badge when routine is_active", async () => {
    renderPage();
    expect(await screen.findByText(/^active$/i)).toBeInTheDocument();
  });

  it("opens edit modal when Edit is clicked on a block", async () => {
    seedBlocks([
      {
        id: "b1",
        routine_id: ROUTINE_ID,
        day_of_week: 0,
        start_time: "09:00",
        title: "Planner study",
        type: "mestrado",
        labels: [],
        subtasks: [],
        sort_order: 0,
      },
    ]);

    const user = userEvent.setup();
    renderPage();
    // Wait for block to appear; use getAllByText since it shows in both desktop+mobile
    expect(await screen.findAllByText("Planner study")).not.toHaveLength(0);

    // Find the first edit button for this block (desktop grid)
    const editBtn = screen.getAllByRole("button", {
      name: /edit block: planner study/i,
    })[0];
    await user.click(editBtn);

    expect(
      screen.getByRole("dialog", { name: /edit block/i }),
    ).toBeInTheDocument();
    expect(screen.getByDisplayValue("Planner study")).toBeInTheDocument();
  });
});
