import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { resetMockState, setMockUserRole } from "../../test/mocks/handlers";
import { useAuthStore } from "../../stores/authStore";
import AdminUsers from "./AdminUsers";

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
}

function renderUsers() {
  return render(
    <QueryClientProvider client={makeClient()}>
      <MemoryRouter>
        <AdminUsers />
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

describe("AdminUsers", () => {
  it("renders the Users heading", () => {
    renderUsers();
    expect(screen.getByRole("heading", { name: /users/i })).toBeInTheDocument();
  });

  it("shows loading state initially", () => {
    renderUsers();
    expect(screen.getByText(/loading users/i)).toBeInTheDocument();
  });

  it("renders the user table with correct column headers", async () => {
    renderUsers();
    await screen.findByText("admin@test.com");
    expect(
      screen.getByRole("columnheader", { name: /email/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: /name/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: /role/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("columnheader", { name: /joined/i }),
    ).toBeInTheDocument();
  });

  it("renders all users from mock data", async () => {
    renderUsers();
    expect(await screen.findByText("admin@test.com")).toBeInTheDocument();
    expect(screen.getByText("alice@test.com")).toBeInTheDocument();
    expect(screen.getByText("bob@test.com")).toBeInTheDocument();
  });

  it("renders role badges for each user", async () => {
    renderUsers();
    await screen.findByText("admin@test.com");
    expect(screen.getByText("admin")).toBeInTheDocument();
    // Two "user" role badges
    const userBadges = screen.getAllByText("user");
    expect(userBadges.length).toBe(2);
  });

  it("renders a Set Rate Limit button for each user", async () => {
    renderUsers();
    await screen.findByText("admin@test.com");
    const buttons = screen.getAllByRole("button", { name: /set rate limit/i });
    expect(buttons.length).toBe(3);
  });

  it("opens rate limit dialog when Set Rate Limit is clicked", async () => {
    const user = userEvent.setup();
    renderUsers();

    await screen.findByText("admin@test.com");
    const [firstBtn] = screen.getAllByRole("button", {
      name: /set rate limit/i,
    });
    await user.click(firstBtn);

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /set rate limit/i }),
    ).toBeInTheDocument();
  });

  it("closes rate limit dialog when Cancel is clicked", async () => {
    const user = userEvent.setup();
    renderUsers();

    await screen.findByText("admin@test.com");
    const [firstBtn] = screen.getAllByRole("button", {
      name: /set rate limit/i,
    });
    await user.click(firstBtn);

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /cancel/i }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("submits rate limit form and shows success", async () => {
    const user = userEvent.setup();
    renderUsers();

    await screen.findByText("admin@test.com");
    const [firstBtn] = screen.getAllByRole("button", {
      name: /set rate limit/i,
    });
    await user.click(firstBtn);

    await user.type(screen.getByLabelText(/daily token limit/i), "50000");
    await user.click(screen.getByRole("button", { name: /apply/i }));

    await waitFor(() => {
      expect(screen.getByText(/rate limit updated/i)).toBeInTheDocument();
    });
  });

  it("shows error alert when users fail to load", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    setMockUserRole("user");

    renderUsers();

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByText(/failed to load users/i)).toBeInTheDocument();

    consoleSpy.mockRestore();
  });
});
