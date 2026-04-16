import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { resetMockState, setMockUserRole } from "../../test/mocks/handlers";
import { useAuthStore } from "../../stores/authStore";
import KillSwitchToggle from "./KillSwitchToggle";

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
}

const enabledSettings = [
  { key: "chat_enabled", value: "true", updated_at: "2026-01-01T00:00:00Z" },
];

const disabledSettings = [
  { key: "chat_enabled", value: "false", updated_at: "2026-01-01T00:00:00Z" },
];

function renderToggle(settings) {
  return render(
    <QueryClientProvider client={makeClient()}>
      <KillSwitchToggle settings={settings} />
    </QueryClientProvider>,
  );
}

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

describe("KillSwitchToggle", () => {
  it("shows enabled status with a green indicator", () => {
    renderToggle(enabledSettings);
    expect(screen.getByText(/enabled/i)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /disable chat/i }),
    ).toBeInTheDocument();
  });

  it("shows disabled status", () => {
    renderToggle(disabledSettings);
    expect(screen.getByText(/disabled/i)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /enable chat/i }),
    ).toBeInTheDocument();
  });

  it("opens StepUpModal when the toggle button is clicked", async () => {
    const user = userEvent.setup();
    renderToggle(enabledSettings);

    await user.click(screen.getByRole("button", { name: /disable chat/i }));

    // StepUpModal should now be in the DOM
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /confirm identity/i }),
    ).toBeInTheDocument();
  });

  it("closes modal when Cancel is clicked", async () => {
    const user = userEvent.setup();
    renderToggle(enabledSettings);

    await user.click(screen.getByRole("button", { name: /disable chat/i }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("calls updateSetting after confirming with a valid password", async () => {
    const user = userEvent.setup();
    renderToggle(enabledSettings);

    // Open modal
    await user.click(screen.getByRole("button", { name: /disable chat/i }));

    // Enter password and confirm
    await user.type(screen.getByLabelText(/password/i), "adminpassword");
    await user.click(screen.getByRole("button", { name: /^confirm$/i }));

    // Modal should close after success
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });
});
