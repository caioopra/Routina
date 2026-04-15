import apiClient from "./client";

/**
 * getProviders — fetch available and selected LLM providers.
 * @returns {{ available: string[], selected: string }}
 */
export async function getProviders() {
  const { data } = await apiClient.get("/settings/providers");
  return data;
}

/**
 * setProvider — update the selected LLM provider.
 * @param {string} provider  — "gemini" | "claude"
 * @returns {{ available: string[], selected: string }}
 */
export async function setProvider(provider) {
  const { data } = await apiClient.post("/settings/llm-provider", { provider });
  return data;
}
