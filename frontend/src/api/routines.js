import apiClient from "./client";

export async function listRoutines() {
  const { data } = await apiClient.get("/routines");
  return data;
}

export async function createRoutine(body) {
  const { data } = await apiClient.post("/routines", body);
  return data;
}

export async function getRoutine(id) {
  const { data } = await apiClient.get(`/routines/${id}`);
  return data;
}

export async function updateRoutine(id, body) {
  const { data } = await apiClient.put(`/routines/${id}`, body);
  return data;
}

export async function activateRoutine(id) {
  const { data } = await apiClient.post(`/routines/${id}/activate`);
  return data;
}

export async function deleteRoutine(id) {
  await apiClient.delete(`/routines/${id}`);
}
