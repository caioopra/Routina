import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect } from "vitest";
import AdminSidebar from "./AdminSidebar";

function renderSidebar(initialPath = "/admin/dashboard") {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <AdminSidebar />
    </MemoryRouter>,
  );
}

describe("AdminSidebar", () => {
  it("renders all 4 navigation links", () => {
    renderSidebar();
    expect(screen.getByRole("link", { name: "Dashboard" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Providers" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Users" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Audit Log" })).toBeInTheDocument();
  });

  it("applies accent active styling to the current route link", () => {
    renderSidebar("/admin/dashboard");
    const dashboardLink = screen.getByRole("link", { name: "Dashboard" });
    // Active link should have the accent purple class
    expect(dashboardLink.className).toMatch(/text-purple-400/);
  });

  it("does not apply active styling to inactive links", () => {
    renderSidebar("/admin/dashboard");
    const providersLink = screen.getByRole("link", { name: "Providers" });
    expect(providersLink.className).not.toMatch(/text-purple-400/);
    expect(providersLink.className).toMatch(/text-neutral-400/);
  });

  it("marks the correct link as active when on the users route", () => {
    renderSidebar("/admin/users");
    const usersLink = screen.getByRole("link", { name: "Users" });
    expect(usersLink.className).toMatch(/text-purple-400/);
    // Others should not be active
    const dashboardLink = screen.getByRole("link", { name: "Dashboard" });
    expect(dashboardLink.className).not.toMatch(/text-purple-400/);
  });
});
