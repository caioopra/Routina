import apiClient from "./client";

export async function listByRoutine(routineId, { day } = {}) {
  const params = {};
  if (day !== undefined) params.day = day;
  const { data } = await apiClient.get(`/routines/${routineId}/blocks`, {
    params,
  });
  return data;
}

export async function createBlock(routineId, body) {
  const { data } = await apiClient.post(`/routines/${routineId}/blocks`, body);
  return data;
}

export async function updateBlock(id, body) {
  const { data } = await apiClient.put(`/blocks/${id}`, body);
  return data;
}

export async function removeBlock(id) {
  await apiClient.delete(`/blocks/${id}`);
}
