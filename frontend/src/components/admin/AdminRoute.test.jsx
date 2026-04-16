import { render, screen } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { describe, it, expect, beforeEach } from "vitest";
import AdminRoute from "./AdminRoute";
import { useAuthStore } from "../../stores/authStore";

function renderWithRouter(initialPath = "/admin/dashboard") {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route
          path="/admin/dashboard"
          element={
            <AdminRoute>
              <div>admin content</div>
            </AdminRoute>
          }
        />
        <Route path="/" element={<div>home sentinel</div>} />
        <Route path="/login" element={<div>login sentinel</div>} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("AdminRoute", () => {
  beforeEach(() => {
    useAuthStore.getState().logout();
    localStorage.clear();
  });

  it("redirects to /login when not authenticated", () => {
    renderWithRouter();
    expect(screen.getByText("login sentinel")).toBeInTheDocument();
    expect(screen.queryByText("admin content")).not.toBeInTheDocument();
  });

  it("renders loading state when authenticated but role is null", () => {
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "t1",
      refresh_token: "r1",
    });
    // role remains null after setAuth (it is not set by setAuth)

    renderWithRouter();
    expect(screen.getByText("Loading...")).toBeInTheDocument();
    expect(screen.queryByText("admin content")).not.toBeInTheDocument();
    expect(screen.queryByText("login sentinel")).not.toBeInTheDocument();
    expect(screen.queryByText("home sentinel")).not.toBeInTheDocument();
  });

  it("redirects to / when authenticated but role is 'user'", () => {
    useAuthStore.setState({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "t1",
      refreshToken: "r1",
      role: "user",
    });

    renderWithRouter();
    expect(screen.getByText("home sentinel")).toBeInTheDocument();
    expect(screen.queryByText("admin content")).not.toBeInTheDocument();
  });

  it("renders children when authenticated and role is 'admin'", () => {
    useAuthStore.setState({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "t1",
      refreshToken: "r1",
      role: "admin",
    });

    renderWithRouter();
    expect(screen.getByText("admin content")).toBeInTheDocument();
    expect(screen.queryByText("login sentinel")).not.toBeInTheDocument();
    expect(screen.queryByText("home sentinel")).not.toBeInTheDocument();
  });
});
