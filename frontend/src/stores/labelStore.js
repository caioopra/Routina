import { create } from "zustand";
import {
  listLabels,
  createLabel,
  updateLabel,
  removeLabel,
} from "../api/labels";

function extractError(err, fallback) {
  return err?.response?.data?.error || err?.message || fallback;
}

export const useLabelStore = create((set, get) => ({
  labels: [],
  loading: false,
  error: null,

  fetch: async () => {
    set({ loading: true, error: null });
    try {
      const labels = await listLabels();
      set({ labels: labels ?? [], loading: false });
    } catch (err) {
      set({
        error: extractError(err, "Failed to load labels"),
        loading: false,
      });
    }
  },

  create: async (body) => {
    set({ error: null });
    try {
      const created = await createLabel(body);
      set((s) => ({ labels: [...s.labels, created] }));
      return created;
    } catch (err) {
      set({ error: extractError(err, "Failed to create label") });
      throw err;
    }
  },

  update: async (id, body) => {
    set({ error: null });
    try {
      const updated = await updateLabel(id, body);
      set((s) => ({
        labels: s.labels.map((l) => (l.id === id ? { ...l, ...updated } : l)),
      }));
      return updated;
    } catch (err) {
      set({ error: extractError(err, "Failed to update label") });
      throw err;
    }
  },

  remove: async (id) => {
    set({ error: null });
    try {
      await removeLabel(id);
      set((s) => ({ labels: s.labels.filter((l) => l.id !== id) }));
    } catch (err) {
      set({ error: extractError(err, "Failed to delete label") });
      throw err;
    }
  },
}));

export default useLabelStore;
