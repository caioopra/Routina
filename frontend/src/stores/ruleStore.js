import { create } from "zustand";
import {
  listByRoutine,
  createRule,
  updateRule,
  removeRule,
} from "../api/rules";

function extractError(err, fallback) {
  return err?.response?.data?.error || err?.message || fallback;
}

export const useRuleStore = create((set, get) => ({
  // keyed by routineId: { rules: [], loading: false, error: null }
  byRoutine: {},

  _getSlice(routineId) {
    return (
      get().byRoutine[routineId] ?? { rules: [], loading: false, error: null }
    );
  },

  _setSlice(routineId, patch) {
    set((s) => ({
      byRoutine: {
        ...s.byRoutine,
        [routineId]: {
          ...(s.byRoutine[routineId] ?? {
            rules: [],
            loading: false,
            error: null,
          }),
          ...patch,
        },
      },
    }));
  },

  fetchByRoutine: async (routineId) => {
    const store = get();
    store._setSlice(routineId, { loading: true, error: null });
    try {
      const rules = await listByRoutine(routineId);
      store._setSlice(routineId, { rules: rules ?? [], loading: false });
    } catch (err) {
      store._setSlice(routineId, {
        error: extractError(err, "Failed to load rules"),
        loading: false,
      });
    }
  },

  create: async (routineId, body) => {
    const store = get();
    store._setSlice(routineId, { error: null });
    try {
      const created = await createRule(routineId, body);
      const slice = store._getSlice(routineId);
      store._setSlice(routineId, { rules: [...slice.rules, created] });
      return created;
    } catch (err) {
      store._setSlice(routineId, {
        error: extractError(err, "Failed to create rule"),
      });
      throw err;
    }
  },

  update: async (routineId, id, body) => {
    const store = get();
    store._setSlice(routineId, { error: null });
    try {
      const updated = await updateRule(id, body);
      const slice = store._getSlice(routineId);
      store._setSlice(routineId, {
        rules: slice.rules.map((r) => (r.id === id ? { ...r, ...updated } : r)),
      });
      return updated;
    } catch (err) {
      store._setSlice(routineId, {
        error: extractError(err, "Failed to update rule"),
      });
      throw err;
    }
  },

  remove: async (routineId, id) => {
    const store = get();
    store._setSlice(routineId, { error: null });
    try {
      await removeRule(id);
      const slice = store._getSlice(routineId);
      store._setSlice(routineId, {
        rules: slice.rules.filter((r) => r.id !== id),
      });
    } catch (err) {
      store._setSlice(routineId, {
        error: extractError(err, "Failed to delete rule"),
      });
      throw err;
    }
  },
}));

export default useRuleStore;
