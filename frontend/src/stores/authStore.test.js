import { describe, it, expect, beforeEach } from "vitest";
import { useAuthStore } from "./authStore";

describe("authStore", () => {
  beforeEach(() => {
    useAuthStore.getState().logout();
    localStorage.clear();
  });

  it("starts with empty state", () => {
    const state = useAuthStore.getState();
    expect(state.user).toBeNull();
    expect(state.token).toBeNull();
    expect(state.refreshToken).toBeNull();
  });

  it("setAuth populates user, token, and refreshToken", () => {
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "t1",
      refresh_token: "r1",
    });

    const state = useAuthStore.getState();
    expect(state.user).toEqual({ id: "u1", email: "a@b.com", name: "A" });
    expect(state.token).toBe("t1");
    expect(state.refreshToken).toBe("r1");
  });

  it("setTokens updates tokens without touching user", () => {
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "t1",
      refresh_token: "r1",
    });
    useAuthStore.getState().setTokens({
      token: "t2",
      refresh_token: "r2",
    });

    const state = useAuthStore.getState();
    expect(state.user).toEqual({ id: "u1", email: "a@b.com", name: "A" });
    expect(state.token).toBe("t2");
    expect(state.refreshToken).toBe("r2");
  });

  it("logout clears state and storage", () => {
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "t1",
      refresh_token: "r1",
    });

    useAuthStore.getState().logout();

    const state = useAuthStore.getState();
    expect(state.user).toBeNull();
    expect(state.token).toBeNull();
    expect(state.refreshToken).toBeNull();

    const stored = localStorage.getItem("planner-auth");
    if (stored) {
      const parsed = JSON.parse(stored);
      expect(parsed.state.token).toBeNull();
      expect(parsed.state.refreshToken).toBeNull();
      expect(parsed.state.user).toBeNull();
    }
  });

  it("persists token/refreshToken/user to localStorage (roundtrip)", () => {
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "t1",
      refresh_token: "r1",
    });

    const raw = localStorage.getItem("planner-auth");
    expect(raw).not.toBeNull();
    const parsed = JSON.parse(raw);
    expect(parsed.state.token).toBe("t1");
    expect(parsed.state.refreshToken).toBe("r1");
    expect(parsed.state.user).toEqual({
      id: "u1",
      email: "a@b.com",
      name: "A",
    });
  });
});
