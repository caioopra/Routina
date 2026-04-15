import { useState } from "react";

export default function LabelsManager({ labels, onCreate, onEdit, onDelete }) {
  const [adding, setAdding] = useState(false);
  const [addForm, setAddForm] = useState({
    name: "",
    color_bg: "#1e3a5f",
    color_text: "#93c5fd",
    color_border: "#2563eb",
    icon: "",
  });
  const [addError, setAddError] = useState("");
  const [addSubmitting, setAddSubmitting] = useState(false);

  const [editingId, setEditingId] = useState(null);
  const [editForm, setEditForm] = useState({});
  const [editError, setEditError] = useState("");
  const [editSubmitting, setEditSubmitting] = useState(false);

  function resetAddForm() {
    setAddForm({
      name: "",
      color_bg: "#1e3a5f",
      color_text: "#93c5fd",
      color_border: "#2563eb",
      icon: "",
    });
    setAddError("");
  }

  async function handleAdd(e) {
    e.preventDefault();
    if (!addForm.name.trim()) {
      setAddError("Name is required");
      return;
    }
    setAddError("");
    setAddSubmitting(true);
    try {
      const body = {
        name: addForm.name.trim(),
        color_bg: addForm.color_bg,
        color_text: addForm.color_text,
        color_border: addForm.color_border,
      };
      if (addForm.icon.trim()) body.icon = addForm.icon.trim();
      await onCreate(body);
      resetAddForm();
      setAdding(false);
    } catch (err) {
      setAddError(
        err?.response?.data?.error || err?.message || "Failed to create label",
      );
    } finally {
      setAddSubmitting(false);
    }
  }

  function startEdit(label) {
    setEditingId(label.id);
    setEditForm({
      name: label.name,
      color_bg: label.color_bg,
      color_text: label.color_text,
      color_border: label.color_border,
      icon: label.icon ?? "",
    });
    setEditError("");
  }

  function cancelEdit() {
    setEditingId(null);
    setEditForm({});
    setEditError("");
  }

  async function handleSaveEdit(e) {
    e.preventDefault();
    if (!editForm.name?.trim()) {
      setEditError("Name is required");
      return;
    }
    setEditError("");
    setEditSubmitting(true);
    try {
      const body = {
        name: editForm.name.trim(),
        color_bg: editForm.color_bg,
        color_text: editForm.color_text,
        color_border: editForm.color_border,
      };
      if (editForm.icon?.trim()) body.icon = editForm.icon.trim();
      await onEdit(editingId, body);
      setEditingId(null);
    } catch (err) {
      setEditError(
        err?.response?.data?.error || err?.message || "Failed to update label",
      );
    } finally {
      setEditSubmitting(false);
    }
  }

  return (
    <section aria-label="Labels manager">
      <div className="flex items-center justify-between mb-4">
        <h2 className="font-display text-base font-semibold text-text-primary flex items-center gap-2">
          <span
            className="inline-block w-0.5 h-4 rounded"
            style={{
              background: "linear-gradient(180deg, #8b5cf6, #6d45d9)",
              boxShadow: "0 0 8px rgba(139,92,246,0.15)",
            }}
          />
          Labels
        </h2>
        {!adding && (
          <button
            type="button"
            onClick={() => {
              setAdding(true);
              resetAddForm();
            }}
            className="text-sm text-accent hover:text-text-primary transition-colors"
          >
            + New label
          </button>
        )}
      </div>

      <div className="flex flex-col gap-2">
        {labels.map((label) =>
          editingId === label.id ? (
            <form
              key={label.id}
              onSubmit={handleSaveEdit}
              aria-label="edit label form"
              className="bg-raised border border-accent/40 rounded-xl p-4 flex flex-col gap-3"
            >
              <label className="flex flex-col gap-1">
                <span className="text-xs text-text-secondary">Name</span>
                <input
                  type="text"
                  value={editForm.name}
                  onChange={(e) =>
                    setEditForm((f) => ({ ...f, name: e.target.value }))
                  }
                  autoFocus
                  className="bg-overlay border border-border rounded-lg px-3 py-1.5 text-text-primary focus:outline-none focus:border-accent text-sm"
                />
              </label>
              <div className="flex gap-3">
                <label className="flex flex-col gap-1 flex-1">
                  <span className="text-xs text-text-secondary">BG color</span>
                  <input
                    type="color"
                    value={editForm.color_bg}
                    onChange={(e) =>
                      setEditForm((f) => ({ ...f, color_bg: e.target.value }))
                    }
                    className="h-8 w-full rounded cursor-pointer border border-border bg-overlay"
                  />
                </label>
                <label className="flex flex-col gap-1 flex-1">
                  <span className="text-xs text-text-secondary">
                    Text color
                  </span>
                  <input
                    type="color"
                    value={editForm.color_text}
                    onChange={(e) =>
                      setEditForm((f) => ({
                        ...f,
                        color_text: e.target.value,
                      }))
                    }
                    className="h-8 w-full rounded cursor-pointer border border-border bg-overlay"
                  />
                </label>
                <label className="flex flex-col gap-1 flex-1">
                  <span className="text-xs text-text-secondary">
                    Border color
                  </span>
                  <input
                    type="color"
                    value={editForm.color_border}
                    onChange={(e) =>
                      setEditForm((f) => ({
                        ...f,
                        color_border: e.target.value,
                      }))
                    }
                    className="h-8 w-full rounded cursor-pointer border border-border bg-overlay"
                  />
                </label>
              </div>
              <label className="flex flex-col gap-1">
                <span className="text-xs text-text-secondary">
                  Icon (emoji, optional)
                </span>
                <input
                  type="text"
                  value={editForm.icon}
                  onChange={(e) =>
                    setEditForm((f) => ({ ...f, icon: e.target.value }))
                  }
                  placeholder="e.g. ⭐"
                  className="bg-overlay border border-border rounded-lg px-3 py-1.5 text-text-primary focus:outline-none focus:border-accent text-sm placeholder:text-text-muted"
                />
              </label>
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
              key={label.id}
              data-testid={`label-${label.id}`}
              className="group bg-raised rounded-xl px-4 py-3 flex items-center gap-3 border border-transparent hover:border-border transition-all"
            >
              <span
                className="text-xs font-medium rounded-full px-3 py-0.5 border flex-shrink-0"
                style={{
                  background: label.color_bg,
                  color: label.color_text,
                  borderColor: label.color_border,
                }}
              >
                {label.icon ? `${label.icon} ` : ""}
                {label.name}
              </span>
              <span className="text-xs text-text-muted flex-1">
                {label.is_default ? "Default" : "Custom"}
              </span>
              <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                <button
                  type="button"
                  onClick={() => startEdit(label)}
                  aria-label={`Edit label ${label.name}`}
                  className="text-xs text-text-muted hover:text-accent px-2 py-1 rounded transition-colors"
                >
                  Edit
                </button>
                <button
                  type="button"
                  onClick={() => onDelete(label.id)}
                  disabled={label.is_default}
                  aria-label={`Delete label ${label.name}`}
                  className="text-xs text-red-400/70 hover:text-red-400 disabled:opacity-30 disabled:cursor-not-allowed px-2 py-1 rounded transition-colors"
                >
                  Delete
                </button>
              </div>
            </div>
          ),
        )}

        {labels.length === 0 && !adding && (
          <p className="text-sm text-text-muted italic text-center py-6">
            No labels yet.
          </p>
        )}

        {adding && (
          <form
            onSubmit={handleAdd}
            aria-label="add label form"
            className="bg-raised border border-accent/40 rounded-xl p-4 flex flex-col gap-3"
          >
            <label className="flex flex-col gap-1">
              <span className="text-xs text-text-secondary">
                Name <span className="text-red-400">*</span>
              </span>
              <input
                type="text"
                value={addForm.name}
                onChange={(e) =>
                  setAddForm((f) => ({ ...f, name: e.target.value }))
                }
                placeholder="Label name"
                autoFocus
                className="bg-overlay border border-border rounded-lg px-3 py-1.5 text-text-primary focus:outline-none focus:border-accent text-sm placeholder:text-text-muted"
              />
            </label>
            <div className="flex gap-3">
              <label className="flex flex-col gap-1 flex-1">
                <span className="text-xs text-text-secondary">BG color</span>
                <input
                  type="color"
                  value={addForm.color_bg}
                  onChange={(e) =>
                    setAddForm((f) => ({ ...f, color_bg: e.target.value }))
                  }
                  className="h-8 w-full rounded cursor-pointer border border-border bg-overlay"
                />
              </label>
              <label className="flex flex-col gap-1 flex-1">
                <span className="text-xs text-text-secondary">Text color</span>
                <input
                  type="color"
                  value={addForm.color_text}
                  onChange={(e) =>
                    setAddForm((f) => ({ ...f, color_text: e.target.value }))
                  }
                  className="h-8 w-full rounded cursor-pointer border border-border bg-overlay"
                />
              </label>
              <label className="flex flex-col gap-1 flex-1">
                <span className="text-xs text-text-secondary">
                  Border color
                </span>
                <input
                  type="color"
                  value={addForm.color_border}
                  onChange={(e) =>
                    setAddForm((f) => ({ ...f, color_border: e.target.value }))
                  }
                  className="h-8 w-full rounded cursor-pointer border border-border bg-overlay"
                />
              </label>
            </div>
            <label className="flex flex-col gap-1">
              <span className="text-xs text-text-secondary">
                Icon (emoji, optional)
              </span>
              <input
                type="text"
                value={addForm.icon}
                onChange={(e) =>
                  setAddForm((f) => ({ ...f, icon: e.target.value }))
                }
                placeholder="e.g. ⭐"
                className="bg-overlay border border-border rounded-lg px-3 py-1.5 text-text-primary focus:outline-none focus:border-accent text-sm placeholder:text-text-muted"
              />
            </label>
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
                  resetAddForm();
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
                {addSubmitting ? "Creating..." : "Create label"}
              </button>
            </div>
          </form>
        )}
      </div>
    </section>
  );
}
