import { useState, useEffect, useRef } from "react";
import { useAuthStore } from "../../stores/authStore";
import { updatePlannerContext } from "../../api/me";

/**
 * PlannerContextEditor — modal editor for the user's planner_context field.
 *
 * Props:
 *   open: boolean
 *   onClose: () => void
 */
export default function PlannerContextEditor({ open, onClose }) {
  const user = useAuthStore((s) => s.user);
  const setUser = useAuthStore((s) => s.setUser);

  const [value, setValue] = useState(user?.planner_context ?? "");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState(null);
  const [saved, setSaved] = useState(false);

  const textareaRef = useRef(null);

  // Sync value when modal opens
  useEffect(() => {
    if (open) {
      setValue(user?.planner_context ?? "");
      setError(null);
      setSaved(false);
      // Focus textarea on open
      setTimeout(() => textareaRef.current?.focus(), 50);
    }
  }, [open, user]);

  async function handleSave() {
    if (saving) return;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      const updated = await updatePlannerContext(value);
      // Merge into auth store user
      setUser({ ...user, ...updated, planner_context: value });
      setSaved(true);
      setTimeout(() => {
        setSaved(false);
        onClose();
      }, 800);
    } catch (err) {
      setError(
        err?.response?.data?.error || err?.message || "Failed to save context",
      );
    } finally {
      setSaving(false);
    }
  }

  // Close on Escape key — listen at document level so it always fires
  useEffect(() => {
    if (!open) return;
    function handleKeyDown(e) {
      if (e.key === "Escape") {
        onClose();
      }
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  if (!open) return null;

  return (
    /* Backdrop */
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Edit planner context"
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(8,6,15,0.85)" }}
    >
      <div
        className="w-full max-w-lg rounded-2xl flex flex-col shadow-2xl"
        style={{
          background: "#0f0c1a",
          border: "1px solid rgba(139,92,246,0.25)",
          maxHeight: "80vh",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          className="px-5 py-4 flex items-center justify-between border-b shrink-0"
          style={{ borderColor: "rgba(139,92,246,0.15)" }}
        >
          <div>
            <h2
              className="font-display text-base font-bold"
              style={{ color: "#e2e0f0" }}
            >
              Planner context
            </h2>
            <p className="text-xs mt-0.5" style={{ color: "#6e6890" }}>
              Describe your job, weekly intent, and long-term goals. The AI
              reads this on every conversation.
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close editor"
            className="text-lg leading-none rounded-lg p-1 transition-colors"
            style={{ color: "#6e6890" }}
          >
            ×
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto p-5">
          <label
            htmlFor="planner-context-textarea"
            className="block text-xs font-semibold mb-2 uppercase tracking-widest"
            style={{ color: "#8b5cf6" }}
          >
            About me
          </label>
          <textarea
            id="planner-context-textarea"
            ref={textareaRef}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            rows={8}
            placeholder="e.g. I'm a PhD student. My main goal this semester is to finish three paper chapters while keeping exercise consistent. I prefer mornings for deep work."
            aria-label="Planner context text"
            className="w-full rounded-xl px-3 py-2.5 text-sm leading-relaxed resize-none outline-none focus:ring-1 transition-all"
            style={{
              background: "#161227",
              color: "#e2e0f0",
              border: "1px solid rgba(139,92,246,0.2)",
              fontFamily: "'DM Sans', sans-serif",
              "--tw-ring-color": "rgba(139,92,246,0.4)",
            }}
          />

          {error && (
            <p
              role="alert"
              className="mt-2 text-xs"
              style={{ color: "#f87171" }}
            >
              {error}
            </p>
          )}
        </div>

        {/* Footer */}
        <div
          className="px-5 py-3 border-t flex items-center justify-end gap-3 shrink-0"
          style={{ borderColor: "rgba(139,92,246,0.15)" }}
        >
          <button
            type="button"
            onClick={onClose}
            className="text-sm px-4 py-2 rounded-xl transition-all"
            style={{
              background: "transparent",
              color: "#6e6890",
              border: "1px solid rgba(139,92,246,0.15)",
            }}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={saving}
            aria-label="Save planner context"
            className="text-sm px-4 py-2 rounded-xl font-semibold transition-all"
            style={{
              background: saved
                ? "rgba(22,163,74,0.2)"
                : "linear-gradient(135deg, #7c3aed, #8b5cf6)",
              color: saved ? "#4ade80" : "#fff",
              border: saved
                ? "1px solid rgba(22,163,74,0.3)"
                : "1px solid rgba(139,92,246,0.3)",
              opacity: saving ? 0.7 : 1,
              cursor: saving ? "default" : "pointer",
            }}
          >
            {saving ? "Saving…" : saved ? "Saved!" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
