import apiClient from "./client";

export async function listLabels() {
  const { data } = await apiClient.get("/labels");
  return data;
}

export async function createLabel(body) {
  const { data } = await apiClient.post("/labels", body);
  return data;
}

export async function updateLabel(id, body) {
  const { data } = await apiClient.put(`/labels/${id}`, body);
  return data;
}

export async function removeLabel(id) {
  await apiClient.delete(`/labels/${id}`);
}
