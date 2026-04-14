import { create } from "zustand";
import {
  listRoutines,
  createRoutine,
  updateRoutine,
  activateRoutine,
  deleteRoutine,
} from "../api/routines";

function extractError(err, fallback) {
  return err?.response?.data?.error || err?.message || fallback;
}

export const useRoutineStore = create((set, get) => ({
  routines: [],
  loading: false,
  error: null,

  get activeRoutineId() {
    return get().routines.find((r) => r.is_active)?.id ?? null;
  },

  fetchRoutines: async () => {
    set({ loading: true, error: null });
    try {
      const routines = await listRoutines();
      set({ routines: routines ?? [], loading: false });
    } catch (err) {
      set({
        error: extractError(err, "Failed to load routines"),
        loading: false,
      });
    }
  },

  create: async (body) => {
    set({ error: null });
    try {
      const created = await createRoutine(body);
      set((s) => ({ routines: [...s.routines, created] }));
      return created;
    } catch (err) {
      set({ error: extractError(err, "Failed to create routine") });
      throw err;
    }
  },

  update: async (id, body) => {
    set({ error: null });
    try {
      const updated = await updateRoutine(id, body);
      set((s) => ({
        routines: s.routines.map((r) =>
          r.id === id ? { ...r, ...updated } : r,
        ),
      }));
      return updated;
    } catch (err) {
      set({ error: extractError(err, "Failed to update routine") });
      throw err;
    }
  },

  activate: async (id) => {
    set({ error: null });
    try {
      await activateRoutine(id);
      set((s) => ({
        routines: s.routines.map((r) => ({ ...r, is_active: r.id === id })),
      }));
    } catch (err) {
      set({ error: extractError(err, "Failed to activate routine") });
      throw err;
    }
  },

  remove: async (id) => {
    set({ error: null });
    try {
      await deleteRoutine(id);
      set((s) => ({ routines: s.routines.filter((r) => r.id !== id) }));
    } catch (err) {
      set({ error: extractError(err, "Failed to delete routine") });
      throw err;
    }
  },
}));

export const useActiveRoutineId = () =>
  useRoutineStore((s) => s.routines.find((r) => r.is_active)?.id ?? null);

export default useRoutineStore;
