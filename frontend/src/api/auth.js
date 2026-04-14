import apiClient from "./client";

export async function register({ email, name, password }) {
  const { data } = await apiClient.post("/auth/register", {
    email,
    name,
    password,
  });
  return data;
}

export async function login({ email, password }) {
  const { data } = await apiClient.post("/auth/login", { email, password });
  return data;
}

export async function refresh(refresh_token) {
  const { data } = await apiClient.post("/auth/refresh", { refresh_token });
  return data;
}

export async function me() {
  const { data } = await apiClient.get("/auth/me");
  return data;
}
