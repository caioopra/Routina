import { render, screen } from "@testing-library/react";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { describe, it, expect, beforeEach } from "vitest";
import ProtectedRoute from "./ProtectedRoute";
import { useAuthStore } from "../../stores/authStore";

function renderWithRouter() {
  return render(
    <MemoryRouter initialEntries={["/"]}>
      <Routes>
        <Route
          path="/"
          element={
            <ProtectedRoute>
              <div>secret content</div>
            </ProtectedRoute>
          }
        />
        <Route path="/login" element={<div>login sentinel</div>} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("ProtectedRoute", () => {
  beforeEach(() => {
    useAuthStore.getState().logout();
    localStorage.clear();
  });

  it("redirects to /login when unauthenticated", () => {
    renderWithRouter();
    expect(screen.getByText("login sentinel")).toBeInTheDocument();
    expect(screen.queryByText("secret content")).not.toBeInTheDocument();
  });

  it("renders children when authenticated", () => {
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "t1",
      refresh_token: "r1",
    });

    renderWithRouter();
    expect(screen.getByText("secret content")).toBeInTheDocument();
    expect(screen.queryByText("login sentinel")).not.toBeInTheDocument();
  });
});
