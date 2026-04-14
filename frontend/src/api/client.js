import axios from "axios";

const apiClient = axios.create({
  baseURL: "/api",
  headers: {
    "Content-Type": "application/json",
  },
});

const REFRESH_URL = "/auth/refresh";

apiClient.interceptors.request.use(async (config) => {
  const { useAuthStore } = await import("../stores/authStore");
  const token = useAuthStore.getState().token;
  if (token) {
    config.headers = config.headers || {};
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

apiClient.interceptors.response.use(
  (response) => response,
  async (error) => {
    const originalRequest = error.config;

    if (!error.response || !originalRequest) {
      return Promise.reject(error);
    }

    const isRefreshRequest = originalRequest.url?.includes(REFRESH_URL);

    if (
      error.response.status === 401 &&
      !originalRequest._retry &&
      !isRefreshRequest
    ) {
      originalRequest._retry = true;

      const { useAuthStore } = await import("../stores/authStore");
      const store = useAuthStore.getState();
      const currentRefreshToken = store.refreshToken;

      if (!currentRefreshToken) {
        store.logout();
        return Promise.reject(error);
      }

      try {
        const { refresh } = await import("./auth");
        const data = await refresh(currentRefreshToken);
        store.setTokens({
          token: data.token,
          refresh_token: data.refresh_token,
        });
        originalRequest.headers = originalRequest.headers || {};
        originalRequest.headers.Authorization = `Bearer ${data.token}`;
        return apiClient(originalRequest);
      } catch (refreshError) {
        store.logout();
        return Promise.reject(refreshError);
      }
    }

    return Promise.reject(error);
  },
);

export default apiClient;
