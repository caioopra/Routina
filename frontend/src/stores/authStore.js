import { create } from "zustand";
import { persist } from "zustand/middleware";

export const useAuthStore = create(
  persist(
    (set) => ({
      user: null,
      token: null,
      refreshToken: null,

      setAuth: ({ user, token, refresh_token }) =>
        set({ user, token, refreshToken: refresh_token }),

      setTokens: ({ token, refresh_token }) =>
        set({ token, refreshToken: refresh_token }),

      setUser: (user) => set({ user }),

      logout: () => set({ user: null, token: null, refreshToken: null }),
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
