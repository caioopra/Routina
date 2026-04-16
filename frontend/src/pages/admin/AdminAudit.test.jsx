import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, beforeEach, vi, afterEach } from "vitest";
import { resetMockState, setMockUserRole } from "../../test/mocks/handlers";
import { useAuthStore } from "../../stores/authStore";
import * as adminApi from "../../api/admin";
import AdminAudit from "./AdminAudit";

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
}

function renderAudit() {
  return render(
    <QueryClientProvider client={makeClient()}>
      <MemoryRouter>
        <AdminAudit />
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

afterEach(() => {
  vi.restoreAllMocks();
});

describe("AdminAudit", () => {
  it("renders the Audit Log heading", () => {
    renderAudit();
    expect(
      screen.getByRole("heading", { name: /audit log/i }),
    ).toBeInTheDocument();
  });

  it("shows loading state initially", () => {
    renderAudit();
    expect(screen.getByText(/loading audit log/i)).toBeInTheDocument();
  });

  it("renders audit entries from mock data", async () => {
    renderAudit();
    // mockAuditLog has 2 entries: "setting.update" and "user.role_change"
    expect(await screen.findByText("setting.update")).toBeInTheDocument();
    expect(screen.getByText("user.role_change")).toBeInTheDocument();
  });

  it("displays actor email for each entry", async () => {
    renderAudit();
    await screen.findByText("setting.update");
    const adminEmails = screen.getAllByText("admin@test.com");
    expect(adminEmails.length).toBeGreaterThanOrEqual(2);
  });

  it("renders the filter input", () => {
    renderAudit();
    expect(
      screen.getByPlaceholderText(/filter by action/i),
    ).toBeInTheDocument();
  });

  it("does not show Load more button when entries are fewer than page size", async () => {
    renderAudit();
    await screen.findByText("setting.update");
    // Only 2 mock entries, well below PAGE_SIZE=20, so no Load more button
    expect(
      screen.queryByRole("button", { name: /load more/i }),
    ).not.toBeInTheDocument();
  });

  it("shows error alert when audit log fails to load", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    setMockUserRole("user");

    renderAudit();

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByText(/failed to load audit log/i)).toBeInTheDocument();
  });

  it("shows load-more error message when loadMore fails", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    // Spy on getAuditLog: first call succeeds with PAGE_SIZE entries, second call rejects
    let callCount = 0;
    vi.spyOn(adminApi, "getAuditLog").mockImplementation(async (params) => {
      callCount += 1;
      if (callCount === 1) {
        // Return exactly PAGE_SIZE entries to trigger "Load more"
        return Array.from({ length: 20 }, (_, i) => ({
          id: `audit-${i + 1}`,
          actor_email: "admin@test.com",
          action: "setting.update",
          target_type: "setting",
          target_id: `key-${i}`,
          payload: {},
          created_at: "2026-04-01T12:00:00Z",
        }));
      }
      throw new Error("Network error");
    });

    const user = userEvent.setup();
    renderAudit();

    // Wait for entries and the Load more button to appear
    const loadMoreBtn = await screen.findByRole("button", {
      name: /load more/i,
    });
    await user.click(loadMoreBtn);

    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
      expect(screen.getByText(/network error/i)).toBeInTheDocument();
    });
  });

  it("filters entries when the action filter is updated", async () => {
    const user = userEvent.setup();
    renderAudit();

    await screen.findByText("setting.update");

    const filterInput = screen.getByPlaceholderText(/filter by action/i);
    await user.type(filterInput, "user");

    // After filtering by "user", only "user.role_change" should remain
    await waitFor(() => {
      expect(screen.queryByText("setting.update")).not.toBeInTheDocument();
      expect(screen.getByText("user.role_change")).toBeInTheDocument();
    });
  });
});
