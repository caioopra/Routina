import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { describe, it, expect } from "vitest";
import AdminShell from "./AdminShell";

function renderShell(initialPath = "/admin/dashboard") {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <Routes>
        <Route path="/admin" element={<AdminShell />}>
          <Route path="dashboard" element={<div>dashboard outlet</div>} />
          <Route path="providers" element={<div>providers outlet</div>} />
        </Route>
        <Route path="/" element={<div>home sentinel</div>} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("AdminShell", () => {
  it("renders the Admin Console header title", () => {
    renderShell();
    expect(screen.getByText("Admin Console")).toBeInTheDocument();
  });

  it("renders the Back to App link", () => {
    renderShell();
    const backLink = screen.getByRole("link", { name: /back to app/i });
    expect(backLink).toBeInTheDocument();
    expect(backLink).toHaveAttribute("href", "/");
  });

  it("renders the sidebar navigation", () => {
    renderShell();
    expect(
      screen.getByRole("navigation", { name: /admin navigation/i }),
    ).toBeInTheDocument();
  });

  it("renders all sidebar nav links", () => {
    renderShell();
    expect(screen.getByRole("link", { name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Providers" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Users" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Audit Log" })).toBeInTheDocument();
  });

  it("renders the outlet content for the active route", () => {
    renderShell("/admin/dashboard");
    expect(screen.getByText("dashboard outlet")).toBeInTheDocument();
  });

  it("toggles mobile sidebar when the hamburger button is pressed", async () => {
    const user = userEvent.setup();
    renderShell();

    const toggleBtn = screen.getByRole("button", { name: /toggle sidebar/i });

    // Before opening: only the desktop sidebar nav is rendered
    expect(
      screen.getAllByRole("navigation", { name: /admin navigation/i }),
    ).toHaveLength(1);

    await user.click(toggleBtn);

    // After opening: mobile overlay mounts a second AdminSidebar — two navs in the DOM
    expect(
      screen.getAllByRole("navigation", { name: /admin navigation/i }),
    ).toHaveLength(2);

    // All nav links are present in the overlay
    const navLinks = screen.getAllByRole("link", { name: "Dashboard" });
    expect(navLinks.length).toBeGreaterThanOrEqual(2);
  });
});
