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
  const { data: metrics, isLoading: metricsLoading } = useQuery({
    queryKey: ["admin", "metrics", "usage", 30],
    queryFn: () => getUsageMetrics(30),
    refetchInterval: 30_000,
  });

  const { data: settings, isLoading: settingsLoading } = useQuery({
    queryKey: ["admin", "settings"],
    queryFn: getSettings,
    refetchInterval: 30_000,
  });

  const { data: users, isLoading: usersLoading } = useQuery({
    queryKey: ["admin", "users"],
    queryFn: getUsers,
    refetchInterval: 30_000,
  });

  const isLoading = metricsLoading || settingsLoading || usersLoading;

  return (
    <div>
      <h1
        className="mb-6 text-2xl font-bold tracking-tight text-[#f1eff8]"
        style={{ fontFamily: "Outfit, sans-serif" }}
      >
        Dashboard
      </h1>

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
