import apiClient from "./client";

export async function listByRoutine(routineId) {
  const { data } = await apiClient.get(`/routines/${routineId}/rules`);
  return data;
}

export async function createRule(routineId, body) {
  const { data } = await apiClient.post(`/routines/${routineId}/rules`, body);
  return data;
}

export async function updateRule(id, body) {
  const { data } = await apiClient.put(`/rules/${id}`, body);
  return data;
}

export async function removeRule(id) {
  await apiClient.delete(`/rules/${id}`);
}
