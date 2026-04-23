import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, beforeEach, vi } from "vitest";
import AdminDashboard from "./AdminDashboard";
import { useAuthStore } from "../../stores/authStore";
import { resetMockState, setMockUserRole } from "../../test/mocks/handlers";

function makeClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: 0,
      },
    },
  });
}

function renderDashboard() {
  return render(
    <QueryClientProvider client={makeClient()}>
      <MemoryRouter>
        <AdminDashboard />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("AdminDashboard", () => {
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

  it("renders the Dashboard heading", () => {
    renderDashboard();
    expect(
      screen.getByRole("heading", { name: /dashboard/i }),
    ).toBeInTheDocument();
  });

  it("shows loading state initially", () => {
    renderDashboard();
    expect(screen.getByText(/loading metrics/i)).toBeInTheDocument();
  });

  it("renders all four metric card labels after data loads", async () => {
    renderDashboard();

    expect(await screen.findByText(/total users/i)).toBeInTheDocument();
    expect(screen.getByText(/monthly cost/i)).toBeInTheDocument();
    expect(screen.getByText(/active provider/i)).toBeInTheDocument();
    expect(screen.getByText(/chat status/i)).toBeInTheDocument();
  });

  it("displays the correct user count from mock data", async () => {
    renderDashboard();
    // mockAdminUsers has 3 entries
    expect(await screen.findByText("3")).toBeInTheDocument();
  });

  it("displays the aggregated monthly cost from mock metrics", async () => {
    renderDashboard();
    // mockUsageMetrics: 0.02 + 0.03 = $0.05
    expect(await screen.findByText("$0.05")).toBeInTheDocument();
  });

  it("displays the active provider from mock settings", async () => {
    renderDashboard();
    // llm_default_provider setting is "gemini"
    expect(await screen.findByText("gemini")).toBeInTheDocument();
  });

  it("displays chat enabled status from mock settings", async () => {
    renderDashboard();
    // chat_enabled is "true" → "Enabled"
    expect(await screen.findByText("Enabled")).toBeInTheDocument();
  });

  it("shows error banner when queries fail", async () => {
    // Suppress console.error noise from React Query error logging in this test
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    // Non-admin role causes all three admin endpoints to return 403
    setMockUserRole("user");

    renderDashboard();

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByText(/failed to load dashboard data/i),
    ).toBeInTheDocument();

    consoleSpy.mockRestore();
  });
});
