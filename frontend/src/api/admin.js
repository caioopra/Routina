import apiClient from "./client";

/**
 * getDashboard — fetch admin dashboard summary.
 * @returns {{ ok: boolean, admin_email: string }}
 */
export async function getDashboard() {
  const { data } = await apiClient.get("/admin/dashboard");
  return data;
}

/**
 * getSettings — fetch all admin settings.
 * @returns {Array<{ key: string, value: string, updated_at: string }>}
 */
export async function getSettings() {
  const { data } = await apiClient.get("/admin/settings");
  return data;
}

/**
 * updateSetting — update an admin setting value.
 * @param {string} key
 * @param {string} value
 * @param {string} confirmToken
 * @returns {{ key: string, value: string, updated_at: string }}
 */
export async function updateSetting(key, value, confirmToken) {
  const { data } = await apiClient.post(
    "/admin/settings",
    { key, value },
    { headers: { "x-confirm-token": confirmToken } },
  );
  return data;
}

/**
 * getUsageMetrics — fetch usage metrics for the past N days.
 * @param {number} days
 * @returns {Array<{ day: string, provider: string, model: string, input_tokens: number, output_tokens: number, request_count: number, estimated_cost_usd: number }>}
 */
export async function getUsageMetrics(days = 30) {
  const { data } = await apiClient.get("/admin/metrics/usage", {
    params: { days },
  });
  return data;
}

/**
 * getUsers — fetch all users.
 * @returns {Array<{ id: string, email: string, name: string, role: string, created_at: string }>}
 */
export async function getUsers() {
  const { data } = await apiClient.get("/admin/users");
  return data;
}

/**
 * getAuditLog — fetch audit log entries with optional cursor and filters.
 * @param {{ before?: string, action?: string, limit?: number }} options
 * @returns {Array<{ id: string, actor_email: string, action: string, target_type: string, target_id: string, payload: object, created_at: string }>}
 */
export async function getAuditLog({ before, action, limit } = {}) {
  const params = {};
  if (before !== undefined) params.before = before;
  if (action !== undefined) params.action = action;
  if (limit !== undefined) params.limit = limit;
  const { data } = await apiClient.get("/admin/audit", { params });
  return data;
}

/**
 * getConfirmToken — obtain a short-lived confirm token for sensitive actions.
 * @param {string} password
 * @param {string} action
 * @returns {{ confirm_token: string }}
 */
export async function getConfirmToken(password, action) {
  const { data } = await apiClient.post("/admin/confirm", { password, action });
  return data;
}

/**
 * setUserRateLimit — override rate limits for a specific user.
 * @param {string} userId
 * @param {{ daily_token_limit?: number, daily_request_limit?: number, override_reason?: string }} limits
 * @param {string} [confirmToken]
 * @returns {object}
 */
export async function setUserRateLimit(userId, limits, confirmToken) {
  const headers = confirmToken
    ? { "x-confirm-token": confirmToken }
    : undefined;
  const { data } = await apiClient.post(
    `/admin/users/${userId}/rate-limit`,
    limits,
    headers ? { headers } : undefined,
  );
  return data;
}
