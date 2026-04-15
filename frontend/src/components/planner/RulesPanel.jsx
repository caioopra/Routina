import { useState } from "react";

export default function RulesPanel({
  rules,
  onAdd,
  onEdit,
  onDelete,
  confirmingDeleteRuleId = null,
  onCancelDeleteRule,
}) {
  const [adding, setAdding] = useState(false);
  const [newText, setNewText] = useState("");
  const [addError, setAddError] = useState("");
  const [addSubmitting, setAddSubmitting] = useState(false);

  const [editingId, setEditingId] = useState(null);
  const [editText, setEditText] = useState("");
  const [editError, setEditError] = useState("");
  const [editSubmitting, setEditSubmitting] = useState(false);

  async function handleAdd(e) {
    e.preventDefault();
    if (!newText.trim()) {
      setAddError("Rule text is required");
      return;
    }
    setAddError("");
    setAddSubmitting(true);
    try {
      await onAdd({ text: newText.trim() });
      setNewText("");
      setAdding(false);
    } catch (err) {
      setAddError(
        err?.response?.data?.error || err?.message || "Failed to add rule",
      );
    } finally {
      setAddSubmitting(false);
    }
  }

  function startEdit(rule) {
    setEditingId(rule.id);
    setEditText(rule.text);
    setEditError("");
  }

  function cancelEdit() {
    setEditingId(null);
    setEditText("");
    setEditError("");
  }

  async function handleSaveEdit(e) {
    e.preventDefault();
    if (!editText.trim()) {
      setEditError("Rule text is required");
      return;
    }
    setEditError("");
    setEditSubmitting(true);
    try {
      await onEdit(editingId, { text: editText.trim() });
      setEditingId(null);
    } catch (err) {
      setEditError(
        err?.response?.data?.error || err?.message || "Failed to update rule",
      );
    } finally {
      setEditSubmitting(false);
    }
  }

  return (
    <section aria-label="Rules panel">
      <div className="flex items-center justify-between mb-4">
        <h2 className="font-display text-base font-semibold text-text-primary flex items-center gap-2">
          <span
            className="inline-block w-0.5 h-4 rounded"
            style={{
              background: "linear-gradient(180deg, #8b5cf6, #6d45d9)",
              boxShadow: "0 0 8px rgba(139,92,246,0.15)",
            }}
          />
          Rules
        </h2>
        {!adding && (
          <button
            type="button"
            onClick={() => {
              setAdding(true);
              setAddError("");
              setNewText("");
            }}
            className="text-sm text-accent hover:text-text-primary transition-colors"
          >
            + Add rule
          </button>
        )}
      </div>

      <div className="flex flex-col gap-2">
        {rules.map((rule) =>
          editingId === rule.id ? (
            <form
              key={rule.id}
              onSubmit={handleSaveEdit}
              aria-label="edit rule form"
              className="bg-raised border border-accent/40 rounded-xl p-3 flex flex-col gap-2"
            >
              <textarea
                value={editText}
                onChange={(e) => setEditText(e.target.value)}
                rows={2}
                autoFocus
                aria-label="Rule text"
                className="bg-overlay border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent text-sm resize-none"
              />
              {editError && (
                <div
                  role="alert"
                  className="text-xs text-red-400 bg-red-500/10 border border-red-500/30 rounded px-2 py-1"
                >
                  {editError}
                </div>
              )}
              <div className="flex gap-2 justify-end">
                <button
                  type="button"
                  onClick={cancelEdit}
                  className="text-sm text-text-secondary hover:text-text-primary px-3 py-1 rounded-lg transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={editSubmitting}
                  className="text-sm bg-accent hover:bg-accent-dim disabled:opacity-50 text-white px-3 py-1 rounded-lg transition-colors"
                >
                  {editSubmitting ? "Saving..." : "Save"}
                </button>
              </div>
            </form>
          ) : (
            <div
              key={rule.id}
              data-testid={`rule-${rule.id}`}
              className="group bg-raised rounded-xl p-3 border-l-2 border-border flex items-start gap-2 transition-all hover:border-l-accent hover:bg-overlay"
            >
              <p className="flex-1 text-sm text-text-secondary leading-relaxed">
                {rule.text}
              </p>
              {confirmingDeleteRuleId === rule.id ? (
                <div className="flex items-center gap-1 shrink-0">
                  <span className="text-xs text-red-400">Sure?</span>
                  <button
                    type="button"
                    onClick={() => onDelete(rule.id)}
                    aria-label={`Confirm delete rule: ${rule.text}`}
                    className="text-xs text-red-300 bg-red-500/20 hover:bg-red-500/40 px-2 py-1 rounded transition-colors"
                  >
                    Yes
                  </button>
                  <button
                    type="button"
                    onClick={onCancelDeleteRule}
                    aria-label="Cancel delete rule"
                    className="text-xs text-text-muted hover:text-text-secondary px-2 py-1 rounded transition-colors"
                  >
                    No
                  </button>
                </div>
              ) : (
                <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                  <button
                    type="button"
                    onClick={() => startEdit(rule)}
                    aria-label={`Edit rule: ${rule.text}`}
                    className="text-xs text-text-muted hover:text-accent px-2 py-1 rounded transition-colors"
                  >
                    Edit
                  </button>
                  <button
                    type="button"
                    onClick={() => onDelete(rule.id)}
                    aria-label={`Delete rule: ${rule.text}`}
                    className="text-xs text-red-400/70 hover:text-red-400 px-2 py-1 rounded transition-colors"
                  >
                    Delete
                  </button>
                </div>
              )}
            </div>
          ),
        )}

        {rules.length === 0 && !adding && (
          <p className="text-sm text-text-muted italic text-center py-6">
            No rules yet. Add one to guide your routine.
          </p>
        )}

        {adding && (
          <form
            onSubmit={handleAdd}
            aria-label="add rule form"
            className="bg-raised border border-accent/40 rounded-xl p-3 flex flex-col gap-2"
          >
            <textarea
              value={newText}
              onChange={(e) => setNewText(e.target.value)}
              placeholder="e.g. No social media before 10am"
              rows={2}
              autoFocus
              aria-label="Rule text"
              className="bg-overlay border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent text-sm resize-none placeholder:text-text-muted"
            />
            {addError && (
              <div
                role="alert"
                className="text-xs text-red-400 bg-red-500/10 border border-red-500/30 rounded px-2 py-1"
              >
                {addError}
              </div>
            )}
            <div className="flex gap-2 justify-end">
              <button
                type="button"
                onClick={() => {
                  setAdding(false);
                  setAddError("");
                  setNewText("");
                }}
                className="text-sm text-text-secondary hover:text-text-primary px-3 py-1 rounded-lg transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={addSubmitting}
                className="text-sm bg-accent hover:bg-accent-dim disabled:opacity-50 text-white px-3 py-1 rounded-lg transition-colors"
              >
                {addSubmitting ? "Adding..." : "Add rule"}
              </button>
            </div>
          </form>
        )}
      </div>
    </section>
  );
}
