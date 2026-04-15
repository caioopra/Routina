import apiClient from "./client";

export async function getMe() {
  const { data } = await apiClient.get("/auth/me");
  return data;
}

export async function updatePlannerContext(plannerContext) {
  const { data } = await apiClient.put("/me/planner-context", {
    planner_context: plannerContext,
  });
  return data;
}
