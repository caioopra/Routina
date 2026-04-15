import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import RulesPanel from "./RulesPanel";

const SAMPLE_RULES = [
  { id: "r1", text: "No social media before 10am", sort_order: 0 },
  { id: "r2", text: "Exercise every day", sort_order: 1 },
];

function renderPanel(props = {}) {
  const defaults = {
    rules: [],
    onAdd: vi
      .fn()
      .mockResolvedValue({ id: "new-r", text: "new", sort_order: 0 }),
    onEdit: vi.fn().mockResolvedValue({}),
    onDelete: vi.fn().mockResolvedValue(undefined),
    ...props,
  };
  return { ...render(<RulesPanel {...defaults} />), ...defaults };
}

describe("RulesPanel", () => {
  it("renders empty state when no rules", () => {
    renderPanel();
    expect(screen.getByText(/no rules yet/i)).toBeInTheDocument();
  });

  it("renders provided rules", () => {
    renderPanel({ rules: SAMPLE_RULES });
    expect(screen.getByTestId("rule-r1")).toBeInTheDocument();
    expect(screen.getByTestId("rule-r2")).toBeInTheDocument();
    expect(screen.getByText("No social media before 10am")).toBeInTheDocument();
    expect(screen.getByText("Exercise every day")).toBeInTheDocument();
  });

  it("adds a new rule via the form", async () => {
    const user = userEvent.setup();
    const onAdd = vi
      .fn()
      .mockResolvedValue({ id: "new", text: "New rule", sort_order: 0 });
    renderPanel({ onAdd });

    await user.click(screen.getByRole("button", { name: /add rule/i }));

    const form = screen.getByRole("form", { name: /add rule form/i });
    await user.type(within(form).getByLabelText(/rule text/i), "New rule");
    await user.click(within(form).getByRole("button", { name: /add rule/i }));

    expect(onAdd).toHaveBeenCalledWith({ text: "New rule" });
  });

  it("shows validation error when adding empty rule", async () => {
    const user = userEvent.setup();
    const onAdd = vi.fn();
    renderPanel({ onAdd });

    await user.click(screen.getByRole("button", { name: /add rule/i }));
    await user.click(screen.getByRole("button", { name: /add rule/i }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      /rule text is required/i,
    );
    expect(onAdd).not.toHaveBeenCalled();
  });

  it("cancels adding and hides the form", async () => {
    const user = userEvent.setup();
    renderPanel();

    await user.click(screen.getByRole("button", { name: /add rule/i }));
    expect(
      screen.getByRole("form", { name: /add rule form/i }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(
      screen.queryByRole("form", { name: /add rule form/i }),
    ).not.toBeInTheDocument();
  });

  it("shows inline edit form for a rule", async () => {
    const user = userEvent.setup();
    renderPanel({ rules: SAMPLE_RULES });

    const ruleEl = screen.getByTestId("rule-r1");
    await user.click(
      within(ruleEl).getByRole("button", { name: /edit rule/i }),
    );

    const form = screen.getByRole("form", { name: /edit rule form/i });
    expect(
      within(form).getByDisplayValue("No social media before 10am"),
    ).toBeInTheDocument();
  });

  it("calls onEdit when saving edited rule", async () => {
    const user = userEvent.setup();
    const onEdit = vi.fn().mockResolvedValue({ id: "r1", text: "Updated" });
    renderPanel({ rules: SAMPLE_RULES, onEdit });

    const ruleEl = screen.getByTestId("rule-r1");
    await user.click(
      within(ruleEl).getByRole("button", { name: /edit rule/i }),
    );

    const form = screen.getByRole("form", { name: /edit rule form/i });
    const textarea = within(form).getByLabelText(/rule text/i);
    await user.clear(textarea);
    await user.type(textarea, "Updated rule text");
    await user.click(within(form).getByRole("button", { name: /^save$/i }));

    expect(onEdit).toHaveBeenCalledWith("r1", { text: "Updated rule text" });
  });

  it("shows validation error when edit text is empty", async () => {
    const user = userEvent.setup();
    const onEdit = vi.fn();
    renderPanel({ rules: SAMPLE_RULES, onEdit });

    const ruleEl = screen.getByTestId("rule-r1");
    await user.click(
      within(ruleEl).getByRole("button", { name: /edit rule/i }),
    );

    const form = screen.getByRole("form", { name: /edit rule form/i });
    const textarea = within(form).getByLabelText(/rule text/i);
    await user.clear(textarea);
    await user.click(within(form).getByRole("button", { name: /^save$/i }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      /rule text is required/i,
    );
    expect(onEdit).not.toHaveBeenCalled();
  });

  it("calls onDelete when delete is clicked", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn().mockResolvedValue(undefined);
    renderPanel({ rules: SAMPLE_RULES, onDelete });

    const ruleEl = screen.getByTestId("rule-r1");
    await user.click(
      within(ruleEl).getByRole("button", { name: /delete rule/i }),
    );

    expect(onDelete).toHaveBeenCalledWith("r1");
  });
});
