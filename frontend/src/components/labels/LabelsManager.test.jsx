import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import LabelsManager from "./LabelsManager";

const DEFAULT_LABELS = [
  {
    id: "l-default",
    name: "Urgent",
    color_bg: "#3b1f4a",
    color_text: "#d8b4fe",
    color_border: "#7c3aed",
    is_default: true,
  },
];

const CUSTOM_LABELS = [
  {
    id: "l-custom",
    name: "Focus",
    color_bg: "#1e3a5f",
    color_text: "#93c5fd",
    color_border: "#2563eb",
    is_default: false,
  },
];

function renderManager(props = {}) {
  const defaults = {
    labels: [],
    onCreate: vi
      .fn()
      .mockResolvedValue({ id: "new-l", name: "New", is_default: false }),
    onEdit: vi.fn().mockResolvedValue({}),
    onDelete: vi.fn().mockResolvedValue(undefined),
    ...props,
  };
  return { ...render(<LabelsManager {...defaults} />), ...defaults };
}

describe("LabelsManager", () => {
  it("renders empty state when no labels", () => {
    renderManager();
    expect(screen.getByText(/no labels yet/i)).toBeInTheDocument();
  });

  it("renders provided labels", () => {
    renderManager({ labels: [...DEFAULT_LABELS, ...CUSTOM_LABELS] });
    expect(screen.getByTestId("label-l-default")).toBeInTheDocument();
    expect(screen.getByTestId("label-l-custom")).toBeInTheDocument();
  });

  it("disables delete button for default labels", () => {
    renderManager({ labels: DEFAULT_LABELS });
    const row = screen.getByTestId("label-l-default");
    const deleteBtn = within(row).getByRole("button", { name: /delete/i });
    expect(deleteBtn).toBeDisabled();
  });

  it("enables delete button for custom labels", () => {
    renderManager({ labels: CUSTOM_LABELS });
    const row = screen.getByTestId("label-l-custom");
    const deleteBtn = within(row).getByRole("button", { name: /delete/i });
    expect(deleteBtn).not.toBeDisabled();
  });

  it("calls onDelete for custom labels", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn().mockResolvedValue(undefined);
    renderManager({ labels: CUSTOM_LABELS, onDelete });

    const row = screen.getByTestId("label-l-custom");
    await user.click(within(row).getByRole("button", { name: /delete/i }));

    expect(onDelete).toHaveBeenCalledWith("l-custom");
  });

  it("shows add form and creates label", async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn().mockResolvedValue({
      id: "new-label",
      name: "Priority",
      is_default: false,
    });
    renderManager({ onCreate });

    await user.click(screen.getByRole("button", { name: /new label/i }));

    const form = screen.getByRole("form", { name: /add label form/i });
    await user.type(within(form).getByLabelText(/name/i), "Priority");
    await user.click(
      within(form).getByRole("button", { name: /create label/i }),
    );

    expect(onCreate).toHaveBeenCalledWith(
      expect.objectContaining({ name: "Priority" }),
    );
  });

  it("shows validation error when name is missing on create", async () => {
    const user = userEvent.setup();
    const onCreate = vi.fn();
    renderManager({ onCreate });

    await user.click(screen.getByRole("button", { name: /new label/i }));
    await user.click(screen.getByRole("button", { name: /create label/i }));

    expect(screen.getByRole("alert")).toHaveTextContent(/name is required/i);
    expect(onCreate).not.toHaveBeenCalled();
  });

  it("shows edit form with pre-filled name", async () => {
    const user = userEvent.setup();
    renderManager({ labels: CUSTOM_LABELS });

    const row = screen.getByTestId("label-l-custom");
    await user.click(within(row).getByRole("button", { name: /edit/i }));

    const form = screen.getByRole("form", { name: /edit label form/i });
    expect(within(form).getByDisplayValue("Focus")).toBeInTheDocument();
  });

  it("calls onEdit when saving edited label", async () => {
    const user = userEvent.setup();
    const onEdit = vi
      .fn()
      .mockResolvedValue({ id: "l-custom", name: "Renamed" });
    renderManager({ labels: CUSTOM_LABELS, onEdit });

    const row = screen.getByTestId("label-l-custom");
    await user.click(within(row).getByRole("button", { name: /edit/i }));

    const form = screen.getByRole("form", { name: /edit label form/i });
    const nameInput = within(form).getByDisplayValue("Focus");
    await user.clear(nameInput);
    await user.type(nameInput, "Renamed");
    await user.click(within(form).getByRole("button", { name: /^save$/i }));

    expect(onEdit).toHaveBeenCalledWith(
      "l-custom",
      expect.objectContaining({ name: "Renamed" }),
    );
  });
});
