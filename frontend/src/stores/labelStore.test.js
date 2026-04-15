import { describe, it, expect, beforeEach } from "vitest";
import { useLabelStore } from "./labelStore";
import { useAuthStore } from "./authStore";
import { seedLabels } from "../test/mocks/handlers";

function resetStore() {
  useLabelStore.setState({ labels: [], loading: false, error: null });
}

describe("labelStore", () => {
  beforeEach(() => {
    resetStore();
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "token-1",
      refresh_token: "refresh-1",
    });
  });

  it("starts with empty state", () => {
    const state = useLabelStore.getState();
    expect(state.labels).toEqual([]);
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });

  it("fetch loads labels from API", async () => {
    seedLabels([
      {
        id: "l1",
        name: "Urgent",
        color_bg: "#3b1f4a",
        color_text: "#d8b4fe",
        color_border: "#7c3aed",
        is_default: true,
      },
    ]);

    await useLabelStore.getState().fetch();

    const state = useLabelStore.getState();
    expect(state.labels).toHaveLength(1);
    expect(state.labels[0].name).toBe("Urgent");
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });

  it("create adds a new label", async () => {
    await useLabelStore.getState().create({
      name: "Focus",
      color_bg: "#1e3a5f",
      color_text: "#93c5fd",
      color_border: "#2563eb",
    });

    const state = useLabelStore.getState();
    expect(state.labels).toHaveLength(1);
    expect(state.labels[0].name).toBe("Focus");
  });

  it("update patches the label in state", async () => {
    const created = await useLabelStore.getState().create({
      name: "Old Name",
      color_bg: "#1e3a5f",
      color_text: "#93c5fd",
      color_border: "#2563eb",
    });

    await useLabelStore.getState().update(created.id, { name: "New Name" });

    const state = useLabelStore.getState();
    expect(state.labels.find((l) => l.id === created.id).name).toBe("New Name");
  });

  it("remove deletes the label from state", async () => {
    const created = await useLabelStore.getState().create({
      name: "Delete Me",
      color_bg: "#1e3a5f",
      color_text: "#93c5fd",
      color_border: "#2563eb",
    });

    await useLabelStore.getState().remove(created.id);

    expect(useLabelStore.getState().labels).toHaveLength(0);
  });

  it("sets error on failed fetch when unauthenticated", async () => {
    useAuthStore.getState().logout();
    await useLabelStore.getState().fetch();
    const state = useLabelStore.getState();
    expect(state.error).toBeTruthy();
    expect(state.loading).toBe(false);
  });
});
