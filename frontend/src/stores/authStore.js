import { create } from "zustand";
import { persist } from "zustand/middleware";
import { getProviders, setProvider } from "../api/settings";

export const useAuthStore = create(
  persist(
    (set, get) => ({
      user: null,
      token: null,
      refreshToken: null,
      providers: { available: [], selected: null },

      setAuth: ({ user, token, refresh_token }) =>
        set({ user, token, refreshToken: refresh_token }),

      setTokens: ({ token, refresh_token }) =>
        set({ token, refreshToken: refresh_token }),

      setUser: (user) => set({ user }),

      logout: () =>
        set({
          user: null,
          token: null,
          refreshToken: null,
          providers: { available: [], selected: null },
        }),

      /**
       * loadProviders — fetch available + selected providers from the API.
       * Safe to call multiple times; noop if already loaded.
       */
      loadProviders: async () => {
        try {
          const data = await getProviders();
          set({ providers: data });
        } catch {
          // Non-fatal; providers UI will be hidden
        }
      },

      /**
       * selectProvider — optimistically switch provider, rollback on error.
       */
      selectProvider: async (name) => {
        const previous = get().providers;
        set((s) => ({
          providers: { ...s.providers, selected: name },
        }));
        try {
          const data = await setProvider(name);
          set({ providers: data });
        } catch {
          set({ providers: previous });
        }
      },
    }),
    {
      name: "planner-auth",
      partialize: (state) => ({
        token: state.token,
        refreshToken: state.refreshToken,
        user: state.user,
      }),
    },
  ),
);

export const useAuth = () => {
  const s = useAuthStore();
  return {
    user: s.user,
    token: s.token,
    isAuthenticated: !!s.token,
    logout: s.logout,
  };
};

export default useAuthStore;
