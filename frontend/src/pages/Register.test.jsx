import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { describe, it, expect, beforeEach } from "vitest";
import Register from "./Register";
import { useAuthStore } from "../stores/authStore";
import { register } from "../api/auth";

function renderRegister() {
  return render(
    <MemoryRouter initialEntries={["/register"]}>
      <Routes>
        <Route path="/register" element={<Register />} />
        <Route path="/" element={<div>home page</div>} />
      </Routes>
    </MemoryRouter>,
  );
}

describe("Register", () => {
  beforeEach(() => {
    useAuthStore.getState().logout();
    localStorage.clear();
  });

  it("renders name, email, password fields", () => {
    renderRegister();
    expect(screen.getByLabelText(/name/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/email/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/password/i)).toBeInTheDocument();
  });

  it("registers a new user and navigates home", async () => {
    const user = userEvent.setup();
    renderRegister();

    await user.type(screen.getByLabelText(/name/i), "Jane");
    await user.type(screen.getByLabelText(/email/i), "jane@example.com");
    await user.type(screen.getByLabelText(/password/i), "password123");
    await user.click(screen.getByRole("button", { name: /create account/i }));

    await screen.findByText("home page");
    expect(useAuthStore.getState().token).toBeTruthy();
    expect(useAuthStore.getState().user?.email).toBe("jane@example.com");
  });

  it("shows error on duplicate email", async () => {
    const user = userEvent.setup();
    await register({
      email: "dup@example.com",
      name: "Dup",
      password: "password123",
    });
    useAuthStore.getState().logout();

    renderRegister();

    await user.type(screen.getByLabelText(/name/i), "Another");
    await user.type(screen.getByLabelText(/email/i), "dup@example.com");
    await user.type(screen.getByLabelText(/password/i), "password123");
    await user.click(screen.getByRole("button", { name: /create account/i }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/already exists/i);
  });

  it("shows error for short password", async () => {
    const user = userEvent.setup();
    renderRegister();

    const nameInput = screen.getByLabelText(/name/i);
    const emailInput = screen.getByLabelText(/email/i);
    const passwordInput = screen.getByLabelText(/password/i);

    // Remove the HTML minLength so we can test the server-side validation response
    passwordInput.removeAttribute("minLength");

    await user.type(nameInput, "Short");
    await user.type(emailInput, "short@example.com");
    await user.type(passwordInput, "short");
    await user.click(screen.getByRole("button", { name: /create account/i }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/at least 8 characters/i);
  });
});
