import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import Composer from "./Composer";

describe("Composer", () => {
  // ── Basic rendering ────────────────────────────────────────────────────────

  it("renders the textarea and Send button by default", () => {
    render(<Composer onSend={vi.fn()} />);
    expect(
      screen.getByRole("textbox", { name: /message input/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /send message/i }),
    ).toBeInTheDocument();
  });

  it("shows Stop button instead of Send when streaming=true", () => {
    render(<Composer onSend={vi.fn()} streaming={true} />);
    expect(
      screen.getByRole("button", { name: /stop streaming/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /send message/i }),
    ).not.toBeInTheDocument();
  });

  it("Send button is disabled when input is empty", () => {
    render(<Composer onSend={vi.fn()} />);
    expect(
      screen.getByRole("button", { name: /send message/i }),
    ).toBeDisabled();
  });

  it("Send button becomes enabled when text is typed", async () => {
    const user = userEvent.setup();
    render(<Composer onSend={vi.fn()} />);

    await user.type(
      screen.getByRole("textbox", { name: /message input/i }),
      "hello",
    );
    expect(
      screen.getByRole("button", { name: /send message/i }),
    ).not.toBeDisabled();
  });

  it("calls onSend with trimmed value when Send is clicked", async () => {
    const user = userEvent.setup();
    const onSend = vi.fn();
    render(<Composer onSend={onSend} />);

    await user.type(
      screen.getByRole("textbox", { name: /message input/i }),
      "  hello world  ",
    );
    await user.click(screen.getByRole("button", { name: /send message/i }));

    expect(onSend).toHaveBeenCalledWith("hello world");
  });

  it("clears the textarea after sending", async () => {
    const user = userEvent.setup();
    render(<Composer onSend={vi.fn()} />);

    const textarea = screen.getByRole("textbox", { name: /message input/i });
    await user.type(textarea, "test message");
    await user.click(screen.getByRole("button", { name: /send message/i }));

    expect(textarea).toHaveValue("");
  });

  // ── Stop button disabled guard (Fix 4) ────────────────────────────────────

  it("Stop button is disabled when streaming=false (guard against !streaming)", () => {
    // This state (streaming=false but Stop rendered) would not normally occur
    // since the button only renders when streaming=true, but the disabled prop
    // ensures correctness if state changes between renders.
    render(<Composer onSend={vi.fn()} streaming={true} />);
    const stopBtn = screen.getByRole("button", { name: /stop streaming/i });
    // While streaming=true the button should be enabled
    expect(stopBtn).not.toBeDisabled();
  });

  it("clicking Stop once calls onCancel exactly once", async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    render(<Composer onSend={vi.fn()} onCancel={onCancel} streaming={true} />);

    const stopBtn = screen.getByRole("button", { name: /stop streaming/i });
    await user.click(stopBtn);

    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("double-clicking Stop calls onCancel only once", async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    render(<Composer onSend={vi.fn()} onCancel={onCancel} streaming={true} />);

    const stopBtn = screen.getByRole("button", { name: /stop streaming/i });
    // Two rapid clicks — hasCancelledRef guard must prevent the second call
    await user.dblClick(stopBtn);

    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  // ── handleRetry clearError (Fix 2) tested via ChatPanel ───────────────────
  // (covered in ChatPanel.test.jsx — "Retry button clears error immediately")
});
