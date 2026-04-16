/**
 * MetricCard — a simple stat card for the admin dashboard.
 *
 * Props:
 *   label     {string}  — card title (e.g. "Total Users")
 *   value     {string|number}  — primary displayed value
 *   subtitle  {string}  — optional secondary line below the value
 */
export default function MetricCard({ label, value, subtitle }) {
  return (
    <div
      className="rounded-xl border border-purple-500/30 bg-[#161227] p-6"
      style={{ boxShadow: "0 0 12px 0 rgba(139,92,246,0.08)" }}
    >
      <p className="mb-1 text-xs font-medium uppercase tracking-widest text-purple-300/70">
        {label}
      </p>
      <p className="font-mono text-3xl font-bold text-[#f1eff8]">{value}</p>
      {subtitle && <p className="mt-1 text-sm text-neutral-400">{subtitle}</p>}
    </div>
  );
}
