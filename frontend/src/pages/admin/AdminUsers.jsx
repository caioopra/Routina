import { useState } from "react";
import { useQuery, useMutation } from "@tanstack/react-query";
import { getUsers, setUserRateLimit } from "../../api/admin";

function formatDate(iso) {
  if (!iso) return "—";
  return new Date(iso).toLocaleDateString("en-GB", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

function RoleBadge({ role }) {
  const isAdmin = role === "admin";
  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${
        isAdmin
          ? "bg-purple-900/40 text-purple-300 ring-1 ring-purple-500/30"
          : "bg-[#1e1836] text-neutral-400 ring-1 ring-white/10"
      }`}
    >
      {role}
    </span>
  );
}

function RateLimitDialog({ user, onClose }) {
  const [form, setForm] = useState({
    daily_token_limit: "",
    daily_request_limit: "",
    override_reason: "",
  });
  const [submitError, setSubmitError] = useState("");
  const [success, setSuccess] = useState(false);

  const mutation = useMutation({
    mutationFn: () => {
      const limits = {};
      if (form.daily_token_limit !== "")
        limits.daily_token_limit = Number(form.daily_token_limit);
      if (form.daily_request_limit !== "")
        limits.daily_request_limit = Number(form.daily_request_limit);
      if (form.override_reason !== "")
        limits.override_reason = form.override_reason;
      return setUserRateLimit(user.id, limits);
    },
    onSuccess: () => {
      setSuccess(true);
      setSubmitError("");
    },
    onError: (err) => {
      setSubmitError(
        err?.response?.data?.error ??
          err?.message ??
          "Failed to set rate limit",
      );
    },
  });

  function handleChange(field, value) {
    setForm((prev) => ({ ...prev, [field]: value }));
  }

  function handleSubmit(e) {
    e.preventDefault();
    setSubmitError("");
    mutation.mutate();
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ backgroundColor: "rgba(30,24,54,0.80)" }}
      role="dialog"
      aria-modal="true"
      aria-labelledby="ratelimit-title"
    >
      <div className="absolute inset-0" onClick={onClose} aria-hidden="true" />
      <div className="relative w-full max-w-sm rounded-xl border border-purple-500/30 bg-[#161227] p-6 shadow-xl">
        <h2
          id="ratelimit-title"
          className="mb-1 text-lg font-bold tracking-tight text-[#f1eff8]"
          style={{ fontFamily: "Outfit, sans-serif" }}
        >
          Set Rate Limit
        </h2>
        <p className="mb-4 text-sm text-neutral-400">
          Override rate limits for{" "}
          <span className="text-[#f1eff8]">{user.email}</span>
        </p>

        {success ? (
          <div className="space-y-4">
            <p className="text-sm text-green-400">Rate limit updated.</p>
            <div className="flex justify-end">
              <button
                type="button"
                onClick={onClose}
                className="rounded-lg bg-purple-600 px-4 py-2 text-sm font-semibold text-white hover:bg-purple-500"
              >
                Close
              </button>
            </div>
          </div>
        ) : (
          <form onSubmit={handleSubmit} noValidate>
            <div className="space-y-3">
              <div>
                <label
                  htmlFor="daily-token-limit"
                  className="mb-1 block text-xs font-medium uppercase tracking-widest text-purple-300/70"
                >
                  Daily Token Limit
                </label>
                <input
                  id="daily-token-limit"
                  type="number"
                  min="0"
                  value={form.daily_token_limit}
                  onChange={(e) =>
                    handleChange("daily_token_limit", e.target.value)
                  }
                  placeholder="e.g. 100000"
                  className="w-full rounded-lg border border-purple-500/30 bg-[#1e1836] px-3 py-2 text-sm text-[#f1eff8] placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-purple-500/60"
                />
              </div>

              <div>
                <label
                  htmlFor="daily-request-limit"
                  className="mb-1 block text-xs font-medium uppercase tracking-widest text-purple-300/70"
                >
                  Daily Request Limit
                </label>
                <input
                  id="daily-request-limit"
                  type="number"
                  min="0"
                  value={form.daily_request_limit}
                  onChange={(e) =>
                    handleChange("daily_request_limit", e.target.value)
                  }
                  placeholder="e.g. 50"
                  className="w-full rounded-lg border border-purple-500/30 bg-[#1e1836] px-3 py-2 text-sm text-[#f1eff8] placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-purple-500/60"
                />
              </div>

              <div>
                <label
                  htmlFor="override-reason"
                  className="mb-1 block text-xs font-medium uppercase tracking-widest text-purple-300/70"
                >
                  Override Reason
                </label>
                <input
                  id="override-reason"
                  type="text"
                  value={form.override_reason}
                  onChange={(e) =>
                    handleChange("override_reason", e.target.value)
                  }
                  placeholder="e.g. Beta tester exception"
                  className="w-full rounded-lg border border-purple-500/30 bg-[#1e1836] px-3 py-2 text-sm text-[#f1eff8] placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-purple-500/60"
                />
              </div>
            </div>

            {submitError && (
              <p
                role="alert"
                className="mt-3 rounded-lg border border-red-500/30 bg-red-900/20 px-3 py-2 text-sm text-red-400"
              >
                {submitError}
              </p>
            )}

            <div className="mt-5 flex justify-end gap-3">
              <button
                type="button"
                onClick={onClose}
                disabled={mutation.isPending}
                className="rounded-lg px-4 py-2 text-sm text-neutral-400 hover:text-[#f1eff8] disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={mutation.isPending}
                className="rounded-lg bg-purple-600 px-4 py-2 text-sm font-semibold text-white hover:bg-purple-500 disabled:opacity-50"
                style={{ boxShadow: "0 0 8px 0 rgba(139,92,246,0.40)" }}
              >
                {mutation.isPending ? "Saving…" : "Apply"}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}

export default function AdminUsers() {
  const [selectedUser, setSelectedUser] = useState(null);

  const {
    data: users,
    isLoading,
    isError,
  } = useQuery({
    queryKey: ["admin", "users"],
    queryFn: getUsers,
  });

  return (
    <div>
      <h1
        className="mb-6 text-2xl font-bold tracking-tight text-[#f1eff8]"
        style={{ fontFamily: "Outfit, sans-serif" }}
      >
        Users
      </h1>

      {isError && (
        <div
          role="alert"
          className="mb-4 rounded-lg border border-red-500/40 bg-red-900/20 px-4 py-3 text-sm text-red-400"
        >
          Failed to load users.
        </div>
      )}

      {isLoading ? (
        <p className="font-mono text-sm text-purple-400">Loading users…</p>
      ) : (
        <div className="overflow-hidden rounded-xl border border-purple-500/20 bg-[#161227]">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-purple-500/20">
                <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-widest text-purple-300/70">
                  Email
                </th>
                <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-widest text-purple-300/70">
                  Name
                </th>
                <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-widest text-purple-300/70">
                  Role
                </th>
                <th className="px-4 py-3 text-left text-xs font-medium uppercase tracking-widest text-purple-300/70">
                  Joined
                </th>
                <th className="px-4 py-3 text-right text-xs font-medium uppercase tracking-widest text-purple-300/70">
                  Actions
                </th>
              </tr>
            </thead>
            <tbody>
              {users && users.length > 0 ? (
                users.map((user, idx) => (
                  <tr
                    key={user.id}
                    className={`border-b border-purple-500/10 last:border-0 ${
                      idx % 2 === 0 ? "" : "bg-white/[0.02]"
                    }`}
                  >
                    <td className="px-4 py-3 font-mono text-[#f1eff8]">
                      {user.email}
                    </td>
                    <td className="px-4 py-3 text-neutral-300">{user.name}</td>
                    <td className="px-4 py-3">
                      <RoleBadge role={user.role} />
                    </td>
                    <td className="px-4 py-3 text-neutral-400">
                      {formatDate(user.created_at)}
                    </td>
                    <td className="px-4 py-3 text-right">
                      <button
                        type="button"
                        onClick={() => setSelectedUser(user)}
                        className="rounded-md border border-purple-500/30 px-3 py-1 text-xs text-purple-300 transition-colors hover:border-purple-400 hover:text-purple-200"
                      >
                        Set Rate Limit
                      </button>
                    </td>
                  </tr>
                ))
              ) : (
                <tr>
                  <td
                    colSpan={5}
                    className="px-4 py-8 text-center text-neutral-500"
                  >
                    No users found.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}

      {selectedUser && (
        <RateLimitDialog
          user={selectedUser}
          onClose={() => setSelectedUser(null)}
        />
      )}
    </div>
  );
}
