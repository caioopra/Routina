import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, beforeEach } from "vitest";
import ProviderToggle from "./ProviderToggle";
import { useAuthStore } from "../../stores/authStore";
import { resetMockState } from "../../test/mocks/handlers";

function setProviders(available, selected) {
  useAuthStore.setState({ providers: { available, selected } });
}

describe("ProviderToggle", () => {
  beforeEach(() => {
    resetMockState();
    useAuthStore.setState({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "token-1",
      refreshToken: "refresh-1",
      providers: { available: [], selected: null },
    });
  });

  it("renders nothing when no providers are available", () => {
    const { container } = render(<ProviderToggle />);
    expect(container.firstChild).toBeNull();
  });

  it("renders both provider buttons when two providers are available", () => {
    setProviders(["gemini", "claude"], "gemini");
    render(<ProviderToggle />);

    expect(screen.getByRole("button", { name: /gemini/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /claude/i })).toBeInTheDocument();
  });

  it("highlights the selected provider via aria-pressed", () => {
    setProviders(["gemini", "claude"], "gemini");
    render(<ProviderToggle />);

    expect(screen.getByRole("button", { name: /gemini/i })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: /claude/i })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("disables all buttons when only one provider is available", () => {
    setProviders(["gemini"], "gemini");
    render(<ProviderToggle />);

    expect(screen.getByRole("button", { name: /gemini/i })).toBeDisabled();
  });

  it("clicking inactive provider triggers selectProvider optimistically", async () => {
    const user = userEvent.setup();
    setProviders(["gemini", "claude"], "gemini");
    render(<ProviderToggle />);

    await user.click(screen.getByRole("button", { name: /claude/i }));

    // After optimistic update the store should reflect "claude" as selected
    await waitFor(() => {
      const { providers } = useAuthStore.getState();
      expect(providers.selected).toBe("claude");
    });
  });

  it("clicking already-selected provider does not re-trigger selectProvider", async () => {
    const user = userEvent.setup();
    setProviders(["gemini", "claude"], "gemini");
    render(<ProviderToggle />);

    // Click gemini (already selected) — the state should remain "gemini"
    await user.click(screen.getByRole("button", { name: /gemini/i }));

    const { providers } = useAuthStore.getState();
    expect(providers.selected).toBe("gemini");
  });

  it("rolls back selection if API call fails", async () => {
    const user = userEvent.setup();
    // Use a provider name that the mock doesn't recognize as valid
    // The mock will 422, causing the rollback
    setProviders(["gemini", "bad-provider"], "gemini");
    render(<ProviderToggle />);

    // MSW returns 422 for unknown providers, so the store will rollback
    await user.click(screen.getByRole("button", { name: /bad-provider/i }));

    await waitFor(() => {
      const { providers } = useAuthStore.getState();
      expect(providers.selected).toBe("gemini");
    });
  });

  it("provider buttons have a focus-visible ring class and no unconditional outline:none", () => {
    setProviders(["gemini", "claude"], "gemini");
    render(<ProviderToggle />);

    const btn = screen.getByRole("button", { name: /gemini/i });
    // Must carry the Tailwind focus-visible ring
    expect(btn.className).toMatch(/focus-visible:ring-2/);
    // Must NOT have an inline outline:none (which would suppress keyboard focus)
    expect(btn.style.outline).toBe("");
  });
});
