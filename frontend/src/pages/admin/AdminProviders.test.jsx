import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { resetMockState, setMockUserRole } from "../../test/mocks/handlers";
import { useAuthStore } from "../../stores/authStore";
import AdminProviders from "./AdminProviders";

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
}

function renderProviders() {
  return render(
    <QueryClientProvider client={makeClient()}>
      <MemoryRouter>
        <AdminProviders />
      </MemoryRouter>
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

describe("AdminProviders", () => {
  it("renders the Providers heading", () => {
    renderProviders();
    expect(
      screen.getByRole("heading", { name: /providers/i }),
    ).toBeInTheDocument();
  });

  it("shows loading state initially", () => {
    renderProviders();
    expect(screen.getByText(/loading settings/i)).toBeInTheDocument();
  });

  it("renders the LLM Configuration form after data loads", async () => {
    renderProviders();
    expect(await screen.findByText(/llm configuration/i)).toBeInTheDocument();
  });

  it("renders the Save Settings button", async () => {
    renderProviders();
    expect(
      await screen.findByRole("button", { name: /save settings/i }),
    ).toBeInTheDocument();
  });

  it("renders the chat feature section with KillSwitchToggle", async () => {
    renderProviders();
    expect(await screen.findByText(/chat feature/i)).toBeInTheDocument();
    // KillSwitchToggle shows "Chat is enabled"
    expect(screen.getByText(/chat is/i)).toBeInTheDocument();
  });

  it("opens step-up modal when Save Settings is clicked", async () => {
    const user = userEvent.setup();
    renderProviders();

    // Wait for the form to load
    const saveBtn = await screen.findByRole("button", {
      name: /save settings/i,
    });
    await user.click(saveBtn);

    // StepUpModal should open
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /confirm identity/i }),
    ).toBeInTheDocument();
  });

  it("shows error alert when settings fail to load", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    setMockUserRole("user"); // triggers 403 for /admin/settings

    renderProviders();

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByText(/failed to load settings/i)).toBeInTheDocument();

    consoleSpy.mockRestore();
  });
});
