import { useState } from "react";
import { getConfirmToken } from "../../api/admin";

/**
 * StepUpModal — password re-entry before sensitive operations.
 *
 * Props:
 *   open       {boolean}          — controls visibility
 *   onClose    {() => void}       — called when dismissed or cancelled
 *   action     {string}           — e.g. "settings.update"
 *   onSuccess  {(token) => void}  — called with the confirm token on success
 */
export default function StepUpModal({ open, onClose, action, onSuccess }) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  if (!open) return null;

  async function handleSubmit(e) {
    e.preventDefault();
    setError("");
    setLoading(true);
    try {
      const result = await getConfirmToken(password, action);
      setPassword("");
      onSuccess(result.confirm_token);
      onClose();
    } catch (err) {
      const msg =
        err?.response?.data?.error ?? err?.message ?? "Authentication failed";
      setError(msg);
    } finally {
      setLoading(false);
    }
  }

  function handleClose() {
    setPassword("");
    setError("");
    onClose();
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ backgroundColor: "rgba(30,24,54,0.80)" }}
      role="dialog"
      aria-modal="true"
      aria-labelledby="stepup-title"
    >
      {/* Backdrop click to close */}
      <div
        className="absolute inset-0"
        onClick={handleClose}
        aria-hidden="true"
      />

      <div className="relative w-full max-w-sm rounded-xl border border-purple-500/30 bg-[#161227] p-6 shadow-xl">
        <h2
          id="stepup-title"
          className="mb-1 text-lg font-bold tracking-tight text-[#f1eff8]"
          style={{ fontFamily: "Outfit, sans-serif" }}
        >
          Confirm Identity
        </h2>
        <p className="mb-4 text-sm text-neutral-400">
          Enter your password to authorise this action.
        </p>

        <form onSubmit={handleSubmit} noValidate>
          <label
            htmlFor="stepup-password"
            className="mb-1 block text-xs font-medium uppercase tracking-widest text-purple-300/70"
          >
            Password
          </label>
          <input
            id="stepup-password"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className="mb-4 w-full rounded-lg border border-purple-500/30 bg-[#1e1836] px-3 py-2 text-sm text-[#f1eff8] placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-purple-500/60"
            placeholder="Your password"
            autoComplete="current-password"
            required
            disabled={loading}
          />

          {error && (
            <p
              role="alert"
              className="mb-3 rounded-lg border border-red-500/30 bg-red-900/20 px-3 py-2 text-sm text-red-400"
            >
              {error}
            </p>
          )}

          <div className="flex justify-end gap-3">
            <button
              type="button"
              onClick={handleClose}
              disabled={loading}
              className="rounded-lg px-4 py-2 text-sm text-neutral-400 transition-colors hover:text-[#f1eff8] disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={loading || !password}
              className="rounded-lg bg-purple-600 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-purple-500 disabled:opacity-50"
              style={{ boxShadow: "0 0 8px 0 rgba(139,92,246,0.40)" }}
            >
              {loading ? "Verifying…" : "Confirm"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
