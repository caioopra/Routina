import { describe, it, expect, beforeEach } from "vitest";
import { useBlockStore } from "./blockStore";
import { useAuthStore } from "./authStore";
import { seedRoutines, seedBlocks } from "../test/mocks/handlers";

const ROUTINE_ID = "routine-1";

function resetStore() {
  useBlockStore.setState({ byRoutine: {} });
}

describe("blockStore", () => {
  beforeEach(() => {
    resetStore();
    useAuthStore.getState().setAuth({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "token-1",
      refresh_token: "refresh-1",
    });
    seedRoutines([
      { id: ROUTINE_ID, name: "Test", period: "weekly", is_active: true },
    ]);
  });

  it("starts with empty byRoutine", () => {
    expect(useBlockStore.getState().byRoutine).toEqual({});
  });

  it("fetchByRoutine loads blocks from API", async () => {
    seedBlocks([
      {
        id: "b1",
        routine_id: ROUTINE_ID,
        day_of_week: 0,
        start_time: "09:00",
        title: "Work",
        type: "trabalho",
        labels: [],
        subtasks: [],
      },
    ]);

    await useBlockStore.getState().fetchByRoutine(ROUTINE_ID);

    const slice = useBlockStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.blocks).toHaveLength(1);
    expect(slice.blocks[0].title).toBe("Work");
    expect(slice.loading).toBe(false);
    expect(slice.error).toBeNull();
  });

  it("create adds a new block to the slice", async () => {
    await useBlockStore.getState().fetchByRoutine(ROUTINE_ID);
    await useBlockStore.getState().create(ROUTINE_ID, {
      day_of_week: 1,
      start_time: "10:00",
      title: "New block",
      type: "mestrado",
    });

    const slice = useBlockStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.blocks).toHaveLength(1);
    expect(slice.blocks[0].title).toBe("New block");
  });

  it("update patches the block in state", async () => {
    await useBlockStore.getState().fetchByRoutine(ROUTINE_ID);
    const created = await useBlockStore.getState().create(ROUTINE_ID, {
      day_of_week: 0,
      start_time: "08:00",
      title: "Original",
      type: "trabalho",
    });

    await useBlockStore
      .getState()
      .update(ROUTINE_ID, created.id, { title: "Updated" });

    const slice = useBlockStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.blocks.find((b) => b.id === created.id).title).toBe("Updated");
  });

  it("remove deletes the block from state", async () => {
    await useBlockStore.getState().fetchByRoutine(ROUTINE_ID);
    const created = await useBlockStore.getState().create(ROUTINE_ID, {
      day_of_week: 0,
      start_time: "08:00",
      title: "To delete",
      type: "livre",
    });

    await useBlockStore.getState().remove(ROUTINE_ID, created.id);

    const slice = useBlockStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.blocks).toHaveLength(0);
  });

  it("sets error on failed fetch when unauthenticated", async () => {
    useAuthStore.getState().logout();
    await useBlockStore.getState().fetchByRoutine(ROUTINE_ID);
    const slice = useBlockStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.error).toBeTruthy();
    expect(slice.loading).toBe(false);
  });
});
