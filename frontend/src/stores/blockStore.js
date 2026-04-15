import { create } from "zustand";
import {
  listByRoutine,
  createBlock,
  updateBlock,
  removeBlock,
} from "../api/blocks";

function extractError(err, fallback) {
  return err?.response?.data?.error || err?.message || fallback;
}

export const useBlockStore = create((set, get) => ({
  // keyed by routineId: { blocks: [], loading: false, error: null }
  byRoutine: {},

  _getSlice(routineId) {
    return (
      get().byRoutine[routineId] ?? { blocks: [], loading: false, error: null }
    );
  },

  _setSlice(routineId, patch) {
    set((s) => ({
      byRoutine: {
        ...s.byRoutine,
        [routineId]: {
          ...(s.byRoutine[routineId] ?? {
            blocks: [],
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
      const blocks = await listByRoutine(routineId);
      store._setSlice(routineId, { blocks: blocks ?? [], loading: false });
    } catch (err) {
      store._setSlice(routineId, {
        error: extractError(err, "Failed to load blocks"),
        loading: false,
      });
    }
  },

  create: async (routineId, body) => {
    const store = get();
    const snapshot = store._getSlice(routineId);
    store._setSlice(routineId, { error: null });
    try {
      const created = await createBlock(routineId, body);
      store._setSlice(routineId, { blocks: [...snapshot.blocks, created] });
      return created;
    } catch (err) {
      store._setSlice(routineId, {
        error: extractError(err, "Failed to create block"),
      });
      throw err;
    }
  },

  update: async (routineId, id, body) => {
    const store = get();
    const snapshot = store._getSlice(routineId);
    store._setSlice(routineId, { error: null });
    try {
      const updated = await updateBlock(id, body);
      store._setSlice(routineId, {
        blocks: snapshot.blocks.map((b) =>
          b.id === id ? { ...b, ...updated } : b,
        ),
      });
      return updated;
    } catch (err) {
      store._setSlice(routineId, {
        error: extractError(err, "Failed to update block"),
      });
      throw err;
    }
  },

  remove: async (routineId, id) => {
    const store = get();
    const snapshot = store._getSlice(routineId);
    store._setSlice(routineId, { error: null });
    try {
      await removeBlock(id);
      store._setSlice(routineId, {
        blocks: snapshot.blocks.filter((b) => b.id !== id),
      });
    } catch (err) {
      store._setSlice(routineId, {
        error: extractError(err, "Failed to delete block"),
      });
      throw err;
    }
  },
}));

export default useBlockStore;
