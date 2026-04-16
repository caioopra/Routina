import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { getAuditLog } from "../../api/admin";

const PAGE_SIZE = 20;

function formatDateTime(iso) {
  if (!iso) return "—";
  return new Date(iso).toLocaleString("en-GB", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function ActionBadge({ action }) {
  const color = action.startsWith("setting")
    ? "text-yellow-300 bg-yellow-900/20 ring-yellow-500/30"
    : action.startsWith("user")
      ? "text-blue-300 bg-blue-900/20 ring-blue-500/30"
      : "text-neutral-300 bg-white/5 ring-white/10";

  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-mono ring-1 ${color}`}
    >
      {action}
    </span>
  );
}

export default function AdminAudit() {
  const [filterAction, setFilterAction] = useState("");
  const [entries, setEntries] = useState([]);
  const [cursor, setCursor] = useState(undefined);
  const [hasMore, setHasMore] = useState(true);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [loadMoreError, setLoadMoreError] = useState("");

  // Primary query — fetches first page or filtered results
  const { isLoading, isError, refetch } = useQuery({
    queryKey: ["admin", "audit", filterAction],
    queryFn: async () => {
      const params = { limit: PAGE_SIZE };
      if (filterAction) params.action = filterAction;
      const data = await getAuditLog(params);
      setEntries(data);
      setCursor(data.length > 0 ? data[data.length - 1].id : undefined);
      setHasMore(data.length >= PAGE_SIZE);
      return data;
    },
    refetchOnWindowFocus: false,
  });

  async function loadMore() {
    setIsLoadingMore(true);
    setLoadMoreError("");
    try {
      const params = { limit: PAGE_SIZE };
      if (cursor) params.before = cursor;
      if (filterAction) params.action = filterAction;
      const data = await getAuditLog(params);
      setEntries((prev) => [...prev, ...data]);
      setCursor(data.length > 0 ? data[data.length - 1].id : cursor);
      setHasMore(data.length >= PAGE_SIZE);
    } catch (err) {
      setLoadMoreError(
        err?.response?.data?.error ??
          err?.message ??
          "Failed to load more entries",
      );
    } finally {
      setIsLoadingMore(false);
    }
  }

  function handleFilterChange(e) {
    setFilterAction(e.target.value);
    // Reset pagination when filter changes — the query key change triggers a
    // fresh fetch via React Query, and the queryFn callback resets entries.
    setCursor(undefined);
    setHasMore(true);
  }

  return (
    <div>
      <div className="mb-6 flex flex-wrap items-center justify-between gap-3">
        <h1
          className="text-2xl font-bold tracking-tight text-[#f1eff8]"
          style={{ fontFamily: "Outfit, sans-serif" }}
        >
          Audit Log
        </h1>

        <div className="flex items-center gap-2">
          <label htmlFor="audit-filter" className="sr-only">
            Filter by action
          </label>
          <input
            id="audit-filter"
            type="text"
            value={filterAction}
            onChange={handleFilterChange}
            placeholder="Filter by action…"
            className="rounded-lg border border-purple-500/30 bg-[#1e1836] px-3 py-1.5 text-sm text-[#f1eff8] placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-purple-500/60"
          />
        </div>
      </div>

      {isError && (
        <div
          role="alert"
          className="mb-4 rounded-lg border border-red-500/40 bg-red-900/20 px-4 py-3 text-sm text-red-400"
        >
          Failed to load audit log.
        </div>
      )}

      {isLoading ? (
        <p className="font-mono text-sm text-purple-400">Loading audit log…</p>
      ) : (
        <div className="space-y-2">
          {entries.length === 0 ? (
            <p className="py-8 text-center text-sm text-neutral-500">
              No audit entries found.
            </p>
          ) : (
            entries.map((entry) => (
              <div
                key={entry.id}
                className="flex flex-wrap items-start gap-3 rounded-xl border border-purple-500/10 bg-[#161227] px-4 py-3"
              >
                {/* Timeline dot */}
                <span
                  className="mt-1 h-2 w-2 shrink-0 rounded-full bg-purple-500/60"
                  aria-hidden="true"
                />

                <div className="min-w-0 flex-1">
                  <div className="flex flex-wrap items-center gap-2">
                    <ActionBadge action={entry.action} />
                    <span className="text-xs text-neutral-500">
                      {formatDateTime(entry.created_at)}
                    </span>
                  </div>
                  <p className="mt-1 text-sm text-neutral-300">
                    <span className="font-medium text-[#f1eff8]">
                      {entry.actor_email}
                    </span>{" "}
                    {entry.target_type && (
                      <>
                        on{" "}
                        <span className="font-mono text-purple-300">
                          {entry.target_type}
                        </span>
                      </>
                    )}
                    {entry.target_id && (
                      <>
                        {" "}
                        <span className="font-mono text-xs text-neutral-500">
                          #{entry.target_id}
                        </span>
                      </>
                    )}
                  </p>
                </div>
              </div>
            ))
          )}

          {entries.length > 0 && hasMore && (
            <div className="flex flex-col items-center gap-2 pt-2">
              {loadMoreError && (
                <p
                  role="alert"
                  className="rounded-lg border border-red-500/30 bg-red-900/20 px-3 py-2 text-sm text-red-400"
                >
                  {loadMoreError}
                </p>
              )}
              <button
                type="button"
                onClick={loadMore}
                disabled={isLoadingMore}
                className="rounded-lg border border-purple-500/30 px-5 py-2 text-sm text-purple-300 transition-colors hover:border-purple-400 hover:text-purple-200 disabled:opacity-50"
              >
                {isLoadingMore ? "Loading…" : "Load more"}
              </button>
            </div>
          )}

          {!hasMore && entries.length > 0 && (
            <p className="pt-2 text-center text-xs text-neutral-600">
              End of audit log
            </p>
          )}
        </div>
      )}
    </div>
  );
}
