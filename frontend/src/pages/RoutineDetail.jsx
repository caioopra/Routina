import { useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import { useRoutineStore } from "../stores/routineStore";
import { useBlockStore } from "../stores/blockStore";
import { useLabelStore } from "../stores/labelStore";
import { useRuleStore } from "../stores/ruleStore";
import BlockModal from "../components/planner/BlockModal";
import RulesPanel from "../components/planner/RulesPanel";
import LabelsManager from "../components/labels/LabelsManager";

const COLORS = {
  trabalho: { bg: "#1e3a5f", text: "#93c5fd", border: "#2563eb" },
  mestrado: { bg: "#3b1f4a", text: "#d8b4fe", border: "#7c3aed" },
  aula: { bg: "#4a2c1b", text: "#fdba74", border: "#ea580c" },
  exercicio: { bg: "#1a3a2a", text: "#86efac", border: "#16a34a" },
  slides: { bg: "#4a3f1b", text: "#fde68a", border: "#ca8a04" },
  viagem: { bg: "#3b3b3b", text: "#d4d4d4", border: "#737373" },
  livre: { bg: "#1e2d3d", text: "#7dd3fc", border: "#0284c7" },
};

const DAY_NAMES = [
  "Monday",
  "Tuesday",
  "Wednesday",
  "Thursday",
  "Friday",
  "Saturday",
  "Sunday",
];

const DAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// Tabs for the side panel
const TABS = ["Rules", "Labels"];

export default function RoutineDetail() {
  const { id } = useParams();

  const routines = useRoutineStore((s) => s.routines);
  const fetchRoutines = useRoutineStore((s) => s.fetchRoutines);

  const blocksByRoutine = useBlockStore((s) => s.byRoutine);
  const fetchBlocks = useBlockStore((s) => s.fetchByRoutine);
  const createBlock = useBlockStore((s) => s.create);
  const updateBlock = useBlockStore((s) => s.update);
  const removeBlock = useBlockStore((s) => s.remove);

  const labels = useLabelStore((s) => s.labels);
  const labelsLoading = useLabelStore((s) => s.loading);
  const fetchLabels = useLabelStore((s) => s.fetch);
  const createLabel = useLabelStore((s) => s.create);
  const updateLabel = useLabelStore((s) => s.update);
  const removeLabel = useLabelStore((s) => s.remove);

  const rulesByRoutine = useRuleStore((s) => s.byRoutine);
  const fetchRules = useRuleStore((s) => s.fetchByRoutine);
  const createRule = useRuleStore((s) => s.create);
  const updateRule = useRuleStore((s) => s.update);
  const removeRule = useRuleStore((s) => s.remove);

  const [activeTab, setActiveTab] = useState("Rules");
  const [modalOpen, setModalOpen] = useState(false);
  const [modalDay, setModalDay] = useState(0);
  const [editingBlock, setEditingBlock] = useState(null);
  const [selectedDay, setSelectedDay] = useState(0); // for mobile

  // Derive routine from store (may not be loaded yet)
  const routine = routines.find((r) => r.id === id);

  const blockSlice = blocksByRoutine[id] ?? {
    blocks: [],
    loading: false,
    error: null,
  };
  const ruleSlice = rulesByRoutine[id] ?? {
    rules: [],
    loading: false,
    error: null,
  };

  useEffect(() => {
    if (!routine) fetchRoutines();
    fetchBlocks(id);
    fetchLabels();
    fetchRules(id);
  }, [id]); // eslint-disable-line react-hooks/exhaustive-deps

  // Group blocks by day_of_week
  const blocksByDay = Array.from({ length: 7 }, (_, i) =>
    blockSlice.blocks
      .filter((b) => b.day_of_week === i)
      .sort((a, b) => {
        // sort by start_time
        if (a.start_time < b.start_time) return -1;
        if (a.start_time > b.start_time) return 1;
        return (a.sort_order ?? 0) - (b.sort_order ?? 0);
      }),
  );

  function openAddModal(day) {
    setEditingBlock(null);
    setModalDay(day);
    setModalOpen(true);
  }

  function openEditModal(block) {
    setEditingBlock(block);
    setModalDay(block.day_of_week);
    setModalOpen(true);
  }

  async function handleBlockSubmit(body) {
    if (editingBlock) {
      await updateBlock(id, editingBlock.id, body);
    } else {
      await createBlock(id, body);
    }
  }

  async function handleDeleteBlock(blockId) {
    if (!window.confirm("Delete this block?")) return;
    await removeBlock(id, blockId);
  }

  async function handleAddRule(body) {
    await createRule(id, body);
  }

  async function handleEditRule(ruleId, body) {
    await updateRule(id, ruleId, body);
  }

  async function handleDeleteRule(ruleId) {
    if (!window.confirm("Delete this rule?")) return;
    await removeRule(id, ruleId);
  }

  // Loading state if we have no routine info and blocks are loading
  if (!routine && blockSlice.loading) {
    return (
      <div className="min-h-screen bg-base flex items-center justify-center">
        <p className="text-text-secondary">Loading routine...</p>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-base">
      {/* Header */}
      <header className="border-b border-border-subtle px-6 py-5">
        <div className="mx-auto max-w-screen-xl flex items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2 mb-1">
              <Link
                to="/routines"
                className="text-sm text-text-muted hover:text-text-secondary transition-colors"
              >
                Routines
              </Link>
              <span className="text-text-muted text-sm">/</span>
              <span className="text-sm text-text-secondary">
                {routine?.name ?? id}
              </span>
            </div>
            <h1 className="font-display text-2xl font-bold text-text-primary leading-tight">
              {routine?.name ?? "Routine"}
            </h1>
            {routine?.period && (
              <p className="text-text-muted text-sm mt-0.5">{routine.period}</p>
            )}
          </div>
          <div className="flex items-center gap-2 shrink-0 mt-1">
            {routine?.is_active && (
              <span className="text-xs font-medium uppercase tracking-wide bg-accent text-white rounded-full px-2 py-0.5">
                Active
              </span>
            )}
          </div>
        </div>
      </header>

      <div className="mx-auto max-w-screen-xl px-4 py-6 flex flex-col gap-6 lg:flex-row lg:gap-8">
        {/* ── Weekly Grid ── */}
        <div className="flex-1 min-w-0">
          {blockSlice.error && (
            <div
              role="alert"
              className="mb-4 text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-lg px-3 py-2"
            >
              {blockSlice.error}
            </div>
          )}

          {/* Desktop: 7-column grid */}
          <div className="hidden md:block bg-surface border border-border rounded-2xl overflow-hidden shadow-2xl">
            <div className="grid grid-cols-7">
              {DAY_NAMES.map((name, dayIndex) => {
                const isWeekend = dayIndex >= 5;
                const dayBlocks = blocksByDay[dayIndex];
                return (
                  <div
                    key={dayIndex}
                    data-testid={`day-col-${dayIndex}`}
                    className="border-r border-border-subtle last:border-r-0"
                    style={{
                      background: isWeekend
                        ? "rgba(139, 92, 246, 0.015)"
                        : "transparent",
                    }}
                  >
                    {/* Day header */}
                    <div
                      className="px-2 py-3 border-b border-border-subtle text-center"
                      style={{
                        background: isWeekend
                          ? "linear-gradient(180deg, rgba(139,92,246,0.04), transparent)"
                          : "linear-gradient(180deg, #161227, transparent)",
                      }}
                    >
                      <div
                        className="text-xs font-semibold uppercase tracking-widest"
                        style={{ color: isWeekend ? "#6e6890" : "#8b5cf6" }}
                      >
                        {DAY_SHORT[dayIndex]}
                      </div>
                      <div
                        className="text-sm font-bold font-display mt-0.5 leading-tight"
                        style={{ color: isWeekend ? "#6e6890" : "#a8a3c0" }}
                      >
                        {name}
                      </div>
                    </div>

                    {/* Blocks */}
                    <div className="p-1.5 flex flex-col gap-1">
                      {dayBlocks.map((block) => (
                        <BlockCard
                          key={block.id}
                          block={block}
                          compact
                          onEdit={() => openEditModal(block)}
                          onDelete={() => handleDeleteBlock(block.id)}
                        />
                      ))}

                      {dayBlocks.length === 0 && (
                        <div
                          className="text-center py-8 text-xs italic"
                          style={{ color: "#6e6890", opacity: 0.7 }}
                        >
                          Empty
                        </div>
                      )}

                      <button
                        type="button"
                        onClick={() => openAddModal(dayIndex)}
                        aria-label={`Add block to ${name}`}
                        className="w-full mt-1 py-1.5 text-xs text-text-muted hover:text-accent border border-dashed border-border hover:border-accent/50 rounded-lg transition-all"
                      >
                        + Add
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Mobile: single day view */}
          <div className="md:hidden">
            {/* Day pill selector */}
            <div className="flex gap-1.5 overflow-x-auto pb-2 mb-4">
              {DAY_SHORT.map((short, dayIndex) => {
                const active = selectedDay === dayIndex;
                return (
                  <button
                    key={dayIndex}
                    type="button"
                    onClick={() => setSelectedDay(dayIndex)}
                    aria-label={DAY_NAMES[dayIndex]}
                    className="flex-shrink-0 px-3 py-2 rounded-xl text-xs font-medium transition-all border"
                    style={{
                      background: active ? "#1e1836" : "#0f0c1a",
                      color: active ? "#c4b5fd" : "#6e6890",
                      borderColor: active ? "#8b5cf6" : "#1c1733",
                    }}
                  >
                    {short}
                  </button>
                );
              })}
            </div>

            <h2 className="font-display text-xl font-bold text-text-primary mb-3">
              {DAY_NAMES[selectedDay]}
            </h2>

            <div className="bg-surface border border-border-subtle rounded-xl p-3 flex flex-col gap-2">
              {blocksByDay[selectedDay].map((block) => (
                <BlockCard
                  key={block.id}
                  block={block}
                  onEdit={() => openEditModal(block)}
                  onDelete={() => handleDeleteBlock(block.id)}
                />
              ))}
              {blocksByDay[selectedDay].length === 0 && (
                <p className="text-center text-text-muted text-sm italic py-6">
                  No blocks for this day.
                </p>
              )}
              <button
                type="button"
                onClick={() => openAddModal(selectedDay)}
                aria-label={`Add block to ${DAY_NAMES[selectedDay]}`}
                className="w-full py-2 text-sm text-text-muted hover:text-accent border border-dashed border-border hover:border-accent/50 rounded-lg transition-all mt-1"
              >
                + Add block
              </button>
            </div>
          </div>
        </div>

        {/* ── Side Panel ── */}
        <aside className="w-full lg:w-80 shrink-0 flex flex-col gap-4">
          {/* Tab bar */}
          <div className="flex bg-surface border border-border rounded-xl p-1 gap-1">
            {TABS.map((tab) => (
              <button
                key={tab}
                type="button"
                onClick={() => setActiveTab(tab)}
                aria-label={tab}
                className="flex-1 py-1.5 text-sm font-medium rounded-lg transition-all"
                style={{
                  background:
                    activeTab === tab
                      ? "linear-gradient(135deg, rgba(139,92,246,0.15), #1e1836)"
                      : "transparent",
                  color: activeTab === tab ? "#c4b5fd" : "#6e6890",
                  boxShadow:
                    activeTab === tab
                      ? "0 0 12px rgba(139,92,246,0.08), inset 0 0 0 1px rgba(139,92,246,0.2)"
                      : "none",
                }}
              >
                {tab}
              </button>
            ))}
          </div>

          <div className="bg-surface border border-border rounded-2xl p-5">
            {activeTab === "Rules" && (
              <RulesPanel
                rules={ruleSlice.rules}
                onAdd={handleAddRule}
                onEdit={handleEditRule}
                onDelete={handleDeleteRule}
              />
            )}
            {activeTab === "Labels" && (
              <LabelsManager
                labels={labels}
                onCreate={createLabel}
                onEdit={updateLabel}
                onDelete={removeLabel}
              />
            )}
          </div>

          {ruleSlice.error && activeTab === "Rules" && (
            <div
              role="alert"
              className="text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-lg px-3 py-2"
            >
              {ruleSlice.error}
            </div>
          )}
        </aside>
      </div>

      {/* Block modal */}
      <BlockModal
        open={modalOpen}
        onClose={() => {
          setModalOpen(false);
          setEditingBlock(null);
        }}
        onSubmit={handleBlockSubmit}
        initialBlock={editingBlock}
        defaultDay={modalDay}
        labels={labels}
      />
    </div>
  );
}

// ── BlockCard sub-component ──

function BlockCard({ block, compact = false, onEdit, onDelete }) {
  const c = COLORS[block.type] ?? COLORS.trabalho;
  return (
    <div
      className="group relative rounded-lg overflow-hidden transition-transform hover:-translate-y-0.5"
      style={{
        borderLeft: `3px solid ${c.border}`,
        background: `${c.bg}33`,
        padding: compact ? "6px 8px" : "10px 14px",
      }}
    >
      <div className="flex items-center gap-1.5 flex-wrap mb-0.5">
        {block.start_time && (
          <span
            className="font-mono text-xs"
            style={{ color: "#a8a3c0", fontSize: compact ? 9 : 11 }}
          >
            {block.start_time}
            {block.end_time ? `–${block.end_time}` : ""}
          </span>
        )}
        <span
          className="text-xs rounded font-semibold uppercase px-1 py-0.5 leading-none"
          style={{
            background: `${c.bg}bb`,
            color: c.text,
            border: `1px solid ${c.border}55`,
            fontSize: 8,
            letterSpacing: "0.08em",
          }}
        >
          {block.type}
        </span>
      </div>
      <div
        className="font-medium leading-snug"
        style={{
          color: "#eeedf5",
          fontSize: compact ? 12 : 13,
          fontFamily: "'DM Sans', sans-serif",
        }}
      >
        {block.title}
      </div>
      {block.note && !compact && (
        <div
          className="italic mt-1 leading-snug"
          style={{ color: "#6e6890", fontSize: 11 }}
        >
          {block.note}
        </div>
      )}

      {/* Action buttons on hover */}
      <div className="absolute top-1 right-1 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          type="button"
          onClick={onEdit}
          aria-label={`Edit block: ${block.title}`}
          className="text-xs bg-overlay/80 hover:bg-overlay border border-border rounded px-1.5 py-0.5 text-text-muted hover:text-accent transition-colors"
        >
          Edit
        </button>
        <button
          type="button"
          onClick={onDelete}
          aria-label={`Delete block: ${block.title}`}
          className="text-xs bg-red-500/10 hover:bg-red-500/20 border border-red-500/20 rounded px-1.5 py-0.5 text-red-400/70 hover:text-red-400 transition-colors"
        >
          Del
        </button>
      </div>
    </div>
  );
}
