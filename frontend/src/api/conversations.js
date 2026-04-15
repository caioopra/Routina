import apiClient from "./client";

export async function listConversations() {
  const { data } = await apiClient.get("/conversations");
  return data;
}

export async function createConversation(body) {
  const { data } = await apiClient.post("/conversations", body);
  return data;
}

export async function getMessages(conversationId) {
  const { data } = await apiClient.get(
    `/conversations/${conversationId}/messages`,
  );
  return data;
}
