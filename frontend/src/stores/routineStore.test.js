import { describe, it, expect, beforeEach } from "vitest";
import { useRoutineStore } from "./routineStore";
import { useAuthStore } from "./authStore";
import { seedRoutines } from "../test/mocks/handlers";

function resetStore() {
  useRoutineStore.setState({ routines: [], loading: false, error: null });
}

describe("routineStore", () => {
  beforeEach(() => {
    resetStore();
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "token-1",
      refresh_token: "refresh-1",
    });
  });

  it("starts with empty state", () => {
    const state = useRoutineStore.getState();
    expect(state.routines).toEqual([]);
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });

  it("fetchRoutines loads routines from API", async () => {
    seedRoutines([
      { id: "r1", name: "Weekly", period: "weekly", is_active: true },
      { id: "r2", name: "Daily", period: "daily", is_active: false },
    ]);

    await useRoutineStore.getState().fetchRoutines();

    const state = useRoutineStore.getState();
    expect(state.routines).toHaveLength(2);
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });

  it("create adds a new routine and marks first as active", async () => {
    await useRoutineStore.getState().create({ name: "My routine" });
    const state = useRoutineStore.getState();
    expect(state.routines).toHaveLength(1);
    expect(state.routines[0].name).toBe("My routine");
    expect(state.routines[0].is_active).toBe(true);
  });

  it("update updates a routine", async () => {
    const created = await useRoutineStore
      .getState()
      .create({ name: "Original" });
    await useRoutineStore.getState().update(created.id, { name: "Renamed" });
    const state = useRoutineStore.getState();
    expect(state.routines[0].name).toBe("Renamed");
  });

  it("activate marks only the selected routine as active", async () => {
    const first = await useRoutineStore.getState().create({ name: "First" });
    const second = await useRoutineStore.getState().create({ name: "Second" });

    expect(first.is_active).toBe(true);
    expect(second.is_active).toBe(false);

    await useRoutineStore.getState().activate(second.id);

    const state = useRoutineStore.getState();
    const a = state.routines.find((r) => r.id === first.id);
    const b = state.routines.find((r) => r.id === second.id);
    expect(a.is_active).toBe(false);
    expect(b.is_active).toBe(true);
  });

  it("remove deletes a routine", async () => {
    const created = await useRoutineStore
      .getState()
      .create({ name: "To delete" });
    await useRoutineStore.getState().remove(created.id);
    expect(useRoutineStore.getState().routines).toHaveLength(0);
  });

  it("sets error on failed fetch", async () => {
    useAuthStore.getState().logout();
    await useRoutineStore.getState().fetchRoutines();
    const state = useRoutineStore.getState();
    expect(state.error).toBeTruthy();
    expect(state.loading).toBe(false);
  });
});
