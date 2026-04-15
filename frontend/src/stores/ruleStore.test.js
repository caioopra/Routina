import { describe, it, expect, beforeEach } from "vitest";
import { useRuleStore } from "./ruleStore";
import { useAuthStore } from "./authStore";
import {
  seedRoutines,
  seedRules,
  resetMockState,
} from "../test/mocks/handlers";

const ROUTINE_ID = "routine-1";

function resetStore() {
  useRuleStore.setState({ byRoutine: {} });
}

describe("ruleStore", () => {
  beforeEach(() => {
    resetMockState();
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
    expect(useRuleStore.getState().byRoutine).toEqual({});
  });

  it("fetchByRoutine loads rules from API", async () => {
    seedRules([
      {
        id: "rule-1",
        routine_id: ROUTINE_ID,
        text: "No screens after 10pm",
        sort_order: 0,
      },
    ]);

    await useRuleStore.getState().fetchByRoutine(ROUTINE_ID);

    const slice = useRuleStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.rules).toHaveLength(1);
    expect(slice.rules[0].text).toBe("No screens after 10pm");
    expect(slice.loading).toBe(false);
    expect(slice.error).toBeNull();
  });

  it("create adds a new rule to the slice", async () => {
    await useRuleStore.getState().fetchByRoutine(ROUTINE_ID);
    await useRuleStore
      .getState()
      .create(ROUTINE_ID, { text: "Exercise daily" });

    const slice = useRuleStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.rules).toHaveLength(1);
    expect(slice.rules[0].text).toBe("Exercise daily");
  });

  it("update patches the rule in state", async () => {
    await useRuleStore.getState().fetchByRoutine(ROUTINE_ID);
    const created = await useRuleStore
      .getState()
      .create(ROUTINE_ID, { text: "Original rule" });

    await useRuleStore
      .getState()
      .update(ROUTINE_ID, created.id, { text: "Updated rule" });

    const slice = useRuleStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.rules.find((r) => r.id === created.id).text).toBe(
      "Updated rule",
    );
  });

  it("remove deletes the rule from state", async () => {
    await useRuleStore.getState().fetchByRoutine(ROUTINE_ID);
    const created = await useRuleStore
      .getState()
      .create(ROUTINE_ID, { text: "Temp rule" });

    await useRuleStore.getState().remove(ROUTINE_ID, created.id);

    const slice = useRuleStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.rules).toHaveLength(0);
  });

  it("sets error on failed fetch when unauthenticated", async () => {
    useAuthStore.getState().logout();
    await useRuleStore.getState().fetchByRoutine(ROUTINE_ID);
    const slice = useRuleStore.getState().byRoutine[ROUTINE_ID];
    expect(slice.error).toBeTruthy();
    expect(slice.loading).toBe(false);
  });
});
