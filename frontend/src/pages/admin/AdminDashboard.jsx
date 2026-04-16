import { useQuery } from "@tanstack/react-query";
import { getUsageMetrics, getSettings, getUsers } from "../../api/admin";
import MetricCard from "../../components/admin/MetricCard";

function sumCost(metrics) {
  if (!metrics || metrics.length === 0) return "0.00";
  const total = metrics.reduce(
    (acc, row) => acc + (row.estimated_cost_usd ?? 0),
    0,
  );
  return total.toFixed(2);
}

function activeProvider(settings) {
  if (!settings) return "—";
  const entry = settings.find((s) => s.key === "active_provider");
  return entry ? entry.value : "—";
}

function chatStatus(settings) {
  if (!settings) return "—";
  const entry = settings.find((s) => s.key === "chat_enabled");
  if (!entry) return "—";
  return entry.value === "true" ? "Enabled" : "Disabled";
}

export default function AdminDashboard() {
  const {
    data: metrics,
    isLoading: metricsLoading,
    isError: metricsError,
  } = useQuery({
    queryKey: ["admin", "metrics", "usage", 30],
    queryFn: () => getUsageMetrics(30),
    refetchInterval: 30_000,
  });

  const {
    data: settings,
    isLoading: settingsLoading,
    isError: settingsError,
  } = useQuery({
    queryKey: ["admin", "settings"],
    queryFn: getSettings,
    refetchInterval: 30_000,
  });

  const {
    data: users,
    isLoading: usersLoading,
    isError: usersError,
  } = useQuery({
    queryKey: ["admin", "users"],
    queryFn: getUsers,
    refetchInterval: 30_000,
  });

  const isLoading = metricsLoading || settingsLoading || usersLoading;
  const hasError = metricsError || settingsError || usersError;

  return (
    <div>
      <h1
        className="mb-6 text-2xl font-bold tracking-tight text-[#f1eff8]"
        style={{ fontFamily: "Outfit, sans-serif" }}
      >
        Dashboard
      </h1>

      {hasError && (
        <div
          role="alert"
          className="mb-4 rounded-lg border border-red-500/40 bg-red-900/20 px-4 py-3 text-sm text-red-400"
        >
          Failed to load dashboard data. Showing partial results.
        </div>
      )}

      {isLoading ? (
        <p className="font-mono text-sm text-purple-400">Loading metrics…</p>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <MetricCard
            label="Total Users"
            value={users ? users.length : "—"}
            subtitle="registered accounts"
          />
          <MetricCard
            label="Monthly Cost"
            value={`$${sumCost(metrics)}`}
            subtitle="last 30 days (USD)"
          />
          <MetricCard
            label="Active Provider"
            value={activeProvider(settings)}
            subtitle="LLM backend"
          />
          <MetricCard
            label="Chat Status"
            value={chatStatus(settings)}
            subtitle="chat feature toggle"
          />
        </div>
      )}
    </div>
  );
}
