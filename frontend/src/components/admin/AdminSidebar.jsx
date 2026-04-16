import { NavLink } from "react-router-dom";

const NAV_ITEMS = [
  { to: "/admin/dashboard", label: "Dashboard" },
  { to: "/admin/providers", label: "Providers" },
  { to: "/admin/users", label: "Users" },
  { to: "/admin/audit", label: "Audit Log" },
];

export default function AdminSidebar() {
  return (
    <nav aria-label="Admin navigation" className="flex flex-col gap-1 p-4">
      {NAV_ITEMS.map(({ to, label }) => (
        <NavLink
          key={to}
          to={to}
          className={({ isActive }) =>
            [
              "rounded-lg px-4 py-2.5 text-sm font-medium transition-colors",
              isActive
                ? "bg-purple-500/20 text-purple-400"
                : "text-neutral-400 hover:bg-[#1e1836] hover:text-[#f1eff8]",
            ].join(" ")
          }
        >
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
