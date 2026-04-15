import { useState, useEffect } from "react";

const BLOCK_TYPES = [
  "trabalho",
  "mestrado",
  "aula",
  "exercicio",
  "slides",
  "viagem",
  "livre",
];

const DAY_NAMES = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
];

const EMPTY_FORM = {
  day_of_week: 0,
  start_time: "",
  end_time: "",
  title: "",
  type: "trabalho",
  note: "",
  label_ids: [],
};

export default function BlockModal({
  open,
  onClose,
  onSubmit,
  initialBlock = null,
  defaultDay = 0,
  labels = [],
}) {
  const isEdit = !!initialBlock;

  const [form, setForm] = useState(EMPTY_FORM);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (open) {
      if (isEdit) {
        setForm({
          day_of_week: initialBlock.day_of_week ?? 0,
          start_time: initialBlock.start_time ?? "",
          end_time: initialBlock.end_time ?? "",
          title: initialBlock.title ?? "",
          type: initialBlock.type ?? "trabalho",
          note: initialBlock.note ?? "",
          label_ids: (initialBlock.labels ?? []).map((l) => l.id),
        });
      } else {
        setForm({ ...EMPTY_FORM, day_of_week: defaultDay });
      }
      setError("");
      setSubmitting(false);
    }
  }, [open, isEdit, initialBlock, defaultDay]);

  if (!open) return null;

  function set(field, value) {
    setForm((f) => ({ ...f, [field]: value }));
  }

  function toggleLabel(id) {
    setForm((f) => {
      const has = f.label_ids.includes(id);
      return {
        ...f,
        label_ids: has
          ? f.label_ids.filter((x) => x !== id)
          : [...f.label_ids, id],
      };
    });
  }

  async function handleSubmit(e) {
    e.preventDefault();
    if (!form.title.trim()) {
      setError("Title is required");
      return;
    }
    if (!form.start_time) {
      setError("Start time is required");
      return;
    }
    setError("");
    setSubmitting(true);
    try {
      const body = {
        day_of_week: Number(form.day_of_week),
        start_time: form.start_time,
        title: form.title.trim(),
        type: form.type,
      };
      if (form.end_time) body.end_time = form.end_time;
      if (form.note.trim()) body.note = form.note.trim();
      if (form.label_ids.length) body.label_ids = form.label_ids;
      await onSubmit(body);
      onClose();
    } catch (err) {
      setError(
        err?.response?.data?.error || err?.message || "Failed to save block",
      );
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={isEdit ? "Edit block" : "Add block"}
      className="fixed inset-0 z-50 flex items-center justify-center"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      style={{ background: "rgba(8,6,15,0.75)", backdropFilter: "blur(4px)" }}
    >
      <div
        className="w-full max-w-lg mx-4 rounded-2xl border border-border shadow-2xl"
        style={{ background: "#1e1836" }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-6 pt-5 pb-4 border-b border-border">
          <h2 className="font-display text-lg font-semibold text-text-primary">
            {isEdit ? "Edit block" : "Add block"}
          </h2>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close modal"
            className="text-text-muted hover:text-text-primary transition-colors text-xl leading-none"
          >
            &times;
          </button>
        </div>

        <form
          onSubmit={handleSubmit}
          aria-label={isEdit ? "edit block form" : "add block form"}
          className="px-6 py-5 flex flex-col gap-4"
        >
          {/* Day */}
          <label className="flex flex-col gap-1">
            <span className="text-sm text-text-secondary">Day</span>
            <select
              value={form.day_of_week}
              onChange={(e) => set("day_of_week", e.target.value)}
              className="bg-raised border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent"
            >
              {DAY_NAMES.map((name, i) => (
                <option key={i} value={i}>
                  {name}
                </option>
              ))}
            </select>
          </label>

          {/* Title */}
          <label className="flex flex-col gap-1">
            <span className="text-sm text-text-secondary">
              Title <span className="text-red-400">*</span>
            </span>
            <input
              type="text"
              value={form.title}
              onChange={(e) => set("title", e.target.value)}
              placeholder="Block title"
              className="bg-raised border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent placeholder:text-text-muted"
            />
          </label>

          {/* Type */}
          <label className="flex flex-col gap-1">
            <span className="text-sm text-text-secondary">Type</span>
            <select
              value={form.type}
              onChange={(e) => set("type", e.target.value)}
              className="bg-raised border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent"
            >
              {BLOCK_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </label>

          {/* Time row */}
          <div className="flex gap-3">
            <label className="flex flex-col gap-1 flex-1">
              <span className="text-sm text-text-secondary">
                Start time <span className="text-red-400">*</span>
              </span>
              <input
                type="time"
                value={form.start_time}
                onChange={(e) => set("start_time", e.target.value)}
                className="bg-raised border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent font-mono"
              />
            </label>
            <label className="flex flex-col gap-1 flex-1">
              <span className="text-sm text-text-secondary">End time</span>
              <input
                type="time"
                value={form.end_time}
                onChange={(e) => set("end_time", e.target.value)}
                className="bg-raised border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent font-mono"
              />
            </label>
          </div>

          {/* Note */}
          <label className="flex flex-col gap-1">
            <span className="text-sm text-text-secondary">Note</span>
            <textarea
              value={form.note}
              onChange={(e) => set("note", e.target.value)}
              placeholder="Optional note"
              rows={2}
              className="bg-raised border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent placeholder:text-text-muted resize-none"
            />
          </label>

          {/* Labels */}
          {labels.length > 0 && (
            <fieldset>
              <legend className="text-sm text-text-secondary mb-2">
                Labels
              </legend>
              <div className="flex flex-wrap gap-2">
                {labels.map((label) => {
                  const selected = form.label_ids.includes(label.id);
                  return (
                    <button
                      key={label.id}
                      type="button"
                      onClick={() => toggleLabel(label.id)}
                      aria-pressed={selected}
                      className="text-xs rounded-full px-3 py-1 border transition-all"
                      style={{
                        background: selected ? label.color_bg : "transparent",
                        color: selected ? label.color_text : "#a8a3c0",
                        borderColor: selected ? label.color_border : "#2a2242",
                      }}
                    >
                      {label.icon ? `${label.icon} ` : ""}
                      {label.name}
                    </button>
                  );
                })}
              </div>
            </fieldset>
          )}

          {error && (
            <div
              role="alert"
              className="text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-lg px-3 py-2"
            >
              {error}
            </div>
          )}

          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="text-text-secondary hover:text-text-primary px-4 py-2 rounded-lg transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="bg-accent hover:bg-accent-dim disabled:opacity-50 disabled:cursor-not-allowed text-white font-medium rounded-lg px-4 py-2 transition-colors"
            >
              {submitting ? "Saving..." : isEdit ? "Save changes" : "Add block"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
