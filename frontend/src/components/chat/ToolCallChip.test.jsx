import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect } from "vitest";
import ToolCallChip from "./ToolCallChip";

describe("ToolCallChip", () => {
  const defaultProps = {
    id: "tc-1",
    name: "create_block",
    args: { title: "Morning run", day_of_week: 1 },
    status: "pending",
    data: null,
  };

  it("renders human-readable label for create_block", () => {
    render(<ToolCallChip {...defaultProps} />);
    expect(screen.getByText("Created a block")).toBeInTheDocument();
  });

  it("renders the tool icon ✦", () => {
    render(<ToolCallChip {...defaultProps} />);
    expect(screen.getByText("✦")).toBeInTheDocument();
  });

  it("renders pulsing dot indicator while pending", () => {
    render(<ToolCallChip {...defaultProps} status="pending" />);
    expect(screen.getByLabelText("pending")).toBeInTheDocument();
  });

  it("renders check mark on success", () => {
    render(<ToolCallChip {...defaultProps} status="success" />);
    expect(screen.getByLabelText("success")).toBeInTheDocument();
    expect(screen.getByLabelText("success")).toHaveTextContent("✓");
  });

  it("renders error indicator on failure", () => {
    render(
      <ToolCallChip
        {...defaultProps}
        status="error"
        data={{ error: "Block creation failed" }}
      />,
    );
    expect(screen.getByLabelText("error")).toBeInTheDocument();
    expect(screen.getByLabelText("error")).toHaveTextContent("✕");
  });

  it("error indicator has title attr with error message", () => {
    render(
      <ToolCallChip
        {...defaultProps}
        status="error"
        data={{ error: "Block creation failed" }}
      />,
    );
    expect(screen.getByLabelText("error")).toHaveAttribute(
      "title",
      "Block creation failed",
    );
  });

  it("details panel is hidden by default (collapsed)", () => {
    render(<ToolCallChip {...defaultProps} />);
    expect(
      screen.queryByRole("region", { name: /details for created a block/i }),
    ).not.toBeInTheDocument();
  });

  it("expands details panel on click showing args JSON", async () => {
    const user = userEvent.setup();
    render(<ToolCallChip {...defaultProps} />);

    await user.click(screen.getByRole("button", { name: /created a block/i }));

    const details = screen.getByRole("region", {
      name: /details for created a block/i,
    });
    expect(details).toBeInTheDocument();
    expect(details).toHaveTextContent("Morning run");
  });

  it("collapses details panel on second click", async () => {
    const user = userEvent.setup();
    render(<ToolCallChip {...defaultProps} />);

    const btn = screen.getByRole("button", { name: /created a block/i });
    await user.click(btn);
    await user.click(btn);

    expect(
      screen.queryByRole("region", { name: /details for created a block/i }),
    ).not.toBeInTheDocument();
  });

  it("renders label for update_rule tool", () => {
    render(<ToolCallChip {...defaultProps} name="update_rule" />);
    expect(screen.getByText("Updated a rule")).toBeInTheDocument();
  });

  it("renders label for undo_last_action tool", () => {
    render(<ToolCallChip {...defaultProps} name="undo_last_action" />);
    expect(screen.getByText("Undone last action")).toBeInTheDocument();
  });

  it("shows error detail text in expanded view when status is error", async () => {
    const user = userEvent.setup();
    render(
      <ToolCallChip
        {...defaultProps}
        status="error"
        data={{ error: "Some error detail" }}
      />,
    );

    await user.click(screen.getByRole("button", { name: /created a block/i }));

    expect(screen.getByText(/some error detail/i)).toBeInTheDocument();
  });
});
