/**
 * ConversationList — sidebar listing past conversations, with a
 * "New conversation" button.
 *
 * Props:
 *   conversations: Array<{ id, title, created_at }>
 *   activeId: string | null
 *   onSelect: (id: string) => void
 *   onNew: () => void
 *   loading?: boolean
 */
export default function ConversationList({
  conversations = [],
  activeId,
  onSelect,
  onNew,
  loading = false,
}) {
  return (
    <div
      className="flex flex-col h-full"
      style={{ borderRight: "1px solid rgba(139,92,246,0.12)" }}
    >
      {/* Header */}
      <div
        className="px-3 py-3 border-b flex items-center justify-between shrink-0"
        style={{ borderColor: "rgba(139,92,246,0.12)" }}
      >
        <span
          className="text-xs font-semibold uppercase tracking-widest"
          style={{ color: "#8b5cf6" }}
        >
          Conversations
        </span>
        <button
          type="button"
          onClick={onNew}
          aria-label="New conversation"
          className="text-xs px-2 py-1 rounded-lg transition-all"
          style={{
            background: "rgba(139,92,246,0.12)",
            color: "#c4b5fd",
            border: "1px solid rgba(139,92,246,0.2)",
          }}
        >
          + New
        </button>
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto py-1">
        {loading && (
          <p className="text-xs text-center py-4" style={{ color: "#6e6890" }}>
            Loading…
          </p>
        )}

        {!loading && conversations.length === 0 && (
          <p
            className="text-xs text-center py-4 italic"
            style={{ color: "#6e6890" }}
          >
            No conversations yet
          </p>
        )}

        {conversations.map((conv) => {
          const isActive = conv.id === activeId;
          const label = conv.title || "Untitled conversation";
          const date = conv.created_at
            ? new Date(conv.created_at).toLocaleDateString(undefined, {
                month: "short",
                day: "numeric",
              })
            : "";

          return (
            <button
              key={conv.id}
              type="button"
              onClick={() => onSelect(conv.id)}
              aria-pressed={isActive}
              aria-label={`Open conversation: ${label}`}
              className="w-full text-left px-3 py-2.5 transition-all"
              style={{
                background: isActive ? "rgba(139,92,246,0.12)" : "transparent",
                borderLeft: isActive
                  ? "2px solid #8b5cf6"
                  : "2px solid transparent",
              }}
            >
              <div
                className="text-sm font-medium truncate"
                style={{ color: isActive ? "#c4b5fd" : "#a8a3c0" }}
              >
                {label}
              </div>
              {date && (
                <div className="text-xs mt-0.5" style={{ color: "#6e6890" }}>
                  {date}
                </div>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
