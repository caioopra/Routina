import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { resetMockState, setMockUserRole } from "../../test/mocks/handlers";
import { useAuthStore } from "../../stores/authStore";
import StepUpModal from "./StepUpModal";

beforeEach(() => {
  resetMockState();
  setMockUserRole("admin");
  useAuthStore.setState({
    user: { id: "u1", email: "admin@test.com", name: "Admin" },
    token: "token-1",
    refreshToken: "refresh-1",
    role: "admin",
  });
});

describe("StepUpModal", () => {
  it("does not render when open is false", () => {
    render(
      <StepUpModal
        open={false}
        onClose={vi.fn()}
        action="settings.update"
        onSuccess={vi.fn()}
      />,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("renders the dialog when open is true", () => {
    render(
      <StepUpModal
        open={true}
        onClose={vi.fn()}
        action="settings.update"
        onSuccess={vi.fn()}
      />,
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /confirm identity/i }),
    ).toBeInTheDocument();
  });

  it("renders a password input and Confirm button", () => {
    render(
      <StepUpModal
        open={true}
        onClose={vi.fn()}
        action="settings.update"
        onSuccess={vi.fn()}
      />,
    );
    expect(screen.getByLabelText(/password/i)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /confirm/i }),
    ).toBeInTheDocument();
  });

  it("calls onSuccess with the confirm token on successful submission", async () => {
    const user = userEvent.setup();
    const onSuccess = vi.fn();
    const onClose = vi.fn();

    render(
      <StepUpModal
        open={true}
        onClose={onClose}
        action="settings.update"
        onSuccess={onSuccess}
      />,
    );

    await user.type(screen.getByLabelText(/password/i), "mypassword");
    await user.click(screen.getByRole("button", { name: /confirm/i }));

    await waitFor(() => {
      expect(onSuccess).toHaveBeenCalledWith("mock-confirm-token");
    });
    expect(onClose).toHaveBeenCalled();
  });

  it("shows an error message when the API call fails", async () => {
    const user = userEvent.setup();
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    // Non-admin role causes the confirm endpoint to return 403
    setMockUserRole("user");

    render(
      <StepUpModal
        open={true}
        onClose={vi.fn()}
        action="settings.update"
        onSuccess={vi.fn()}
      />,
    );

    await user.type(screen.getByLabelText(/password/i), "wrongpassword");
    await user.click(screen.getByRole("button", { name: /confirm/i }));

    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });

    consoleSpy.mockRestore();
  });

  it("calls onClose when Cancel is clicked", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();

    render(
      <StepUpModal
        open={true}
        onClose={onClose}
        action="settings.update"
        onSuccess={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onClose).toHaveBeenCalled();
  });
});
