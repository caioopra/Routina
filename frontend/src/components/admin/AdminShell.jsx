import { useState } from "react";
import { Outlet, Link } from "react-router-dom";
import AdminSidebar from "./AdminSidebar";

export default function AdminShell() {
  const [sidebarOpen, setSidebarOpen] = useState(false);

  return (
    <div className="flex min-h-screen flex-col bg-[#08060f]">
      {/* Header */}
      <header className="flex h-14 shrink-0 items-center gap-4 border-b border-purple-500/20 bg-[#0f0c1a] px-4">
        {/* Mobile sidebar toggle */}
        <button
          type="button"
          aria-label="Toggle sidebar"
          className="rounded-md p-1.5 text-neutral-400 hover:text-[#f1eff8] md:hidden"
          onClick={() => setSidebarOpen((v) => !v)}
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            className="h-5 w-5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M4 6h16M4 12h16M4 18h16"
            />
          </svg>
        </button>

        <span
          className="text-lg font-bold tracking-tight text-[#f1eff8]"
          style={{ fontFamily: "Outfit, sans-serif" }}
        >
          Admin Console
        </span>

        <div className="ml-auto">
          <Link
            to="/"
            className="rounded-md px-3 py-1.5 text-sm text-neutral-400 transition-colors hover:text-purple-400"
          >
            Back to App
          </Link>
        </div>
      </header>

      <div className="flex flex-1 overflow-hidden">
        {/* Desktop sidebar */}
        <aside className="hidden w-[220px] shrink-0 border-r border-purple-500/20 bg-[#0f0c1a] md:block">
          <AdminSidebar />
        </aside>

        {/* Mobile sidebar overlay */}
        {sidebarOpen && (
          <>
            <div
              className="fixed inset-0 z-20 bg-black/60 md:hidden"
              onClick={() => setSidebarOpen(false)}
              aria-hidden="true"
            />
            <aside className="fixed inset-y-0 left-0 z-30 w-[220px] border-r border-purple-500/20 bg-[#0f0c1a] md:hidden">
              <AdminSidebar />
            </aside>
          </>
        )}

        {/* Main content */}
        <main className="flex-1 overflow-y-auto p-6">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
