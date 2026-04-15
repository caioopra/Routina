import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import BlockModal from "./BlockModal";

const LABELS = [
  {
    id: "l1",
    name: "Urgent",
    color_bg: "#3b1f4a",
    color_text: "#d8b4fe",
    color_border: "#7c3aed",
    is_default: true,
  },
  {
    id: "l2",
    name: "Focus",
    color_bg: "#1e3a5f",
    color_text: "#93c5fd",
    color_border: "#2563eb",
    is_default: false,
  },
];

function renderModal(props = {}) {
  const defaults = {
    open: true,
    onClose: vi.fn(),
    onSubmit: vi.fn().mockResolvedValue(undefined),
    initialBlock: null,
    defaultDay: 0,
    labels: [],
    ...props,
  };
  return { ...render(<BlockModal {...defaults} />), ...defaults };
}

describe("BlockModal", () => {
  it("renders add block form when no initialBlock", () => {
    renderModal();
    expect(
      screen.getByRole("dialog", { name: /add block/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /add block/i }),
    ).toBeInTheDocument();
  });

  it("renders edit block form when initialBlock is provided", () => {
    renderModal({
      initialBlock: {
        id: "b1",
        day_of_week: 1,
        start_time: "09:00",
        end_time: "10:00",
        title: "Study session",
        type: "mestrado",
        note: "Focus time",
        labels: [],
      },
    });
    expect(
      screen.getByRole("dialog", { name: /edit block/i }),
    ).toBeInTheDocument();
    expect(screen.getByDisplayValue("Study session")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /save changes/i }),
    ).toBeInTheDocument();
  });

  it("does not render when open is false", () => {
    renderModal({ open: false });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("submits create with valid data", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    renderModal({ onSubmit, onClose });

    const form = screen.getByRole("form", { name: /add block form/i });
    await user.clear(within(form).getByLabelText(/title/i));
    await user.type(within(form).getByLabelText(/title/i), "Morning run");

    const startTimeInput = within(form).getByLabelText(/start time/i);
    await user.clear(startTimeInput);
    await user.type(startTimeInput, "07:00");

    await user.click(screen.getByRole("button", { name: /add block/i }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        title: "Morning run",
        start_time: "07:00",
        day_of_week: 0,
      }),
    );
    expect(onClose).toHaveBeenCalled();
  });

  it("shows validation error when title is missing", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    renderModal({ onSubmit });

    // Start time only, no title
    const form = screen.getByRole("form", { name: /add block form/i });
    const startTimeInput = within(form).getByLabelText(/start time/i);
    await user.type(startTimeInput, "08:00");

    await user.click(screen.getByRole("button", { name: /add block/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /title is required/i,
    );
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("shows validation error when start time is missing", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    renderModal({ onSubmit });

    const form = screen.getByRole("form", { name: /add block form/i });
    await user.type(within(form).getByLabelText(/title/i), "Some block");

    await user.click(screen.getByRole("button", { name: /add block/i }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      /start time is required/i,
    );
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("calls onClose when Cancel is clicked", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    renderModal({ onClose });

    await user.click(screen.getByRole("button", { name: /cancel/i }));

    expect(onClose).toHaveBeenCalled();
  });

  it("renders label toggles and toggles selection", async () => {
    const user = userEvent.setup();
    renderModal({ labels: LABELS });

    const urgentBtn = screen.getByRole("button", { name: /urgent/i });
    expect(urgentBtn).toHaveAttribute("aria-pressed", "false");

    await user.click(urgentBtn);
    expect(urgentBtn).toHaveAttribute("aria-pressed", "true");

    await user.click(urgentBtn);
    expect(urgentBtn).toHaveAttribute("aria-pressed", "false");
  });

  it("includes selected label_ids in submit payload", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    renderModal({ onSubmit, onClose, labels: LABELS });

    const form = screen.getByRole("form", { name: /add block form/i });
    await user.type(within(form).getByLabelText(/title/i), "Focused work");
    await user.type(within(form).getByLabelText(/start time/i), "09:00");
    await user.click(screen.getByRole("button", { name: /urgent/i }));
    await user.click(screen.getByRole("button", { name: /add block/i }));

    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({ label_ids: ["l1"] }),
    );
  });

  it("populates edit form from initialBlock", () => {
    renderModal({
      initialBlock: {
        id: "b1",
        day_of_week: 3,
        start_time: "14:00",
        end_time: "15:30",
        title: "Team meeting",
        type: "trabalho",
        note: "Weekly sync",
        labels: [{ id: "l1" }],
      },
      labels: LABELS,
    });

    expect(screen.getByDisplayValue("Team meeting")).toBeInTheDocument();
    expect(screen.getByDisplayValue("14:00")).toBeInTheDocument();
    // The Urgent label should be pre-selected
    expect(screen.getByRole("button", { name: /urgent/i })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });
});
