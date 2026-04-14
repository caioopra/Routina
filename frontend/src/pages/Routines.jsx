import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { useRoutineStore } from "../stores/routineStore";

export default function Routines() {
  const routines = useRoutineStore((s) => s.routines);
  const loading = useRoutineStore((s) => s.loading);
  const error = useRoutineStore((s) => s.error);
  const fetchRoutines = useRoutineStore((s) => s.fetchRoutines);
  const create = useRoutineStore((s) => s.create);
  const update = useRoutineStore((s) => s.update);
  const activate = useRoutineStore((s) => s.activate);
  const remove = useRoutineStore((s) => s.remove);

  const [showForm, setShowForm] = useState(false);
  const [name, setName] = useState("");
  const [period, setPeriod] = useState("weekly");
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState("");

  useEffect(() => {
    fetchRoutines();
  }, [fetchRoutines]);

  async function handleCreate(e) {
    e.preventDefault();
    setFormError("");
    setSubmitting(true);
    try {
      const body = { name: name.trim() };
      if (period) body.period = period;
      await create(body);
      setName("");
      setPeriod("weekly");
      setShowForm(false);
    } catch (err) {
      setFormError(err?.response?.data?.error || "Failed to create routine");
    } finally {
      setSubmitting(false);
    }
  }

  async function handleEditName(routine) {
    const next = window.prompt("New routine name", routine.name);
    if (!next || next.trim() === "" || next === routine.name) return;
    await update(routine.id, { name: next.trim() });
  }

  async function handleDelete(routine) {
    if (!window.confirm(`Delete routine "${routine.name}"?`)) return;
    await remove(routine.id);
  }

  return (
    <div className="min-h-screen bg-base px-4 py-10">
      <div className="mx-auto w-full max-w-3xl">
        <header className="flex items-center justify-between mb-8">
          <div>
            <h1 className="font-display text-3xl font-semibold text-text-primary">
              Routines
            </h1>
            <p className="text-text-secondary text-sm mt-1">
              Manage your weekly routines and activate the one you&apos;re
              living.
            </p>
          </div>
          <div className="flex items-center gap-3">
            <Link
              to="/"
              className="text-sm text-text-secondary hover:text-text-primary transition-colors"
            >
              Back to planner
            </Link>
            <button
              type="button"
              onClick={() => setShowForm((v) => !v)}
              className="bg-accent hover:bg-accent-dim text-white font-medium rounded-lg px-4 py-2 transition-colors"
            >
              {showForm ? "Cancel" : "New routine"}
            </button>
          </div>
        </header>

        {showForm && (
          <form
            onSubmit={handleCreate}
            aria-label="create routine form"
            className="bg-surface border border-border rounded-2xl p-6 mb-6 shadow-2xl"
          >
            <h2 className="font-display text-lg font-semibold text-text-primary mb-4">
              New routine
            </h2>
            <div className="flex flex-col gap-4">
              <label className="flex flex-col gap-1">
                <span className="text-sm text-text-secondary">Name</span>
                <input
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                  className="bg-raised border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent"
                />
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-sm text-text-secondary">Period</span>
                <select
                  value={period}
                  onChange={(e) => setPeriod(e.target.value)}
                  className="bg-raised border border-border rounded-lg px-3 py-2 text-text-primary focus:outline-none focus:border-accent"
                >
                  <option value="weekly">Weekly</option>
                  <option value="daily">Daily</option>
                  <option value="custom">Custom</option>
                </select>
              </label>

              {formError && (
                <div
                  role="alert"
                  className="text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-lg px-3 py-2"
                >
                  {formError}
                </div>
              )}

              <div className="flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setShowForm(false)}
                  className="text-text-secondary hover:text-text-primary px-4 py-2 rounded-lg transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={submitting}
                  className="bg-accent hover:bg-accent-dim disabled:opacity-50 disabled:cursor-not-allowed text-white font-medium rounded-lg px-4 py-2 transition-colors"
                >
                  {submitting ? "Creating..." : "Create"}
                </button>
              </div>
            </div>
          </form>
        )}

        {error && (
          <div
            role="alert"
            className="text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-lg px-3 py-2 mb-6"
          >
            {error}
          </div>
        )}

        {loading && routines.length === 0 ? (
          <p className="text-text-secondary">Loading routines...</p>
        ) : routines.length === 0 ? (
          <div className="bg-surface border border-border rounded-2xl p-10 text-center">
            <p className="text-text-secondary">
              You don&apos;t have any routines yet.
            </p>
          </div>
        ) : (
          <ul className="flex flex-col gap-3">
            {routines.map((routine) => (
              <li
                key={routine.id}
                data-testid={`routine-${routine.id}`}
                className="bg-surface border border-border rounded-2xl p-5 shadow-lg flex items-center justify-between"
              >
                <div className="min-w-0">
                  <div className="flex items-center gap-3">
                    <h3 className="font-display text-lg font-semibold text-text-primary truncate">
                      {routine.name}
                    </h3>
                    {routine.is_active && (
                      <span
                        data-testid={`active-badge-${routine.id}`}
                        className="text-xs font-medium uppercase tracking-wide bg-accent text-white rounded-full px-2 py-0.5"
                      >
                        Active
                      </span>
                    )}
                  </div>
                  <p className="text-text-muted text-sm mt-1">
                    {routine.period || "weekly"}
                  </p>
                </div>
                <div className="flex items-center gap-2 shrink-0">
                  {!routine.is_active && (
                    <button
                      type="button"
                      onClick={() => activate(routine.id)}
                      className="text-sm bg-raised hover:bg-overlay border border-border text-text-primary rounded-lg px-3 py-1.5 transition-colors"
                    >
                      Activate
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => handleEditName(routine)}
                    className="text-sm bg-raised hover:bg-overlay border border-border text-text-primary rounded-lg px-3 py-1.5 transition-colors"
                  >
                    Edit name
                  </button>
                  <button
                    type="button"
                    onClick={() => handleDelete(routine)}
                    className="text-sm bg-red-500/10 hover:bg-red-500/20 border border-red-500/30 text-red-300 rounded-lg px-3 py-1.5 transition-colors"
                  >
                    Delete
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
