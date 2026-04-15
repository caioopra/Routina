import { useState } from "react";
import { getToolLabel } from "./toolLabels";

/**
 * ToolCallChip — compact chip showing a tool call's status and details.
 *
 * Props:
 *   id: string
 *   name: string          — tool name e.g. "create_block"
 *   args: object          — the arguments passed to the tool
 *   status: 'pending' | 'success' | 'error'
 *   data?: object         — result data from tool_result event
 */
export default function ToolCallChip({ id, name, args, status, data }) {
  const [expanded, setExpanded] = useState(false);

  const label = getToolLabel(name);

  const statusIndicator = () => {
    if (status === "pending") {
      return (
        <span
          className="inline-block w-2 h-2 rounded-full animate-pulse shrink-0"
          style={{ background: "#8b5cf6" }}
          aria-label="pending"
        />
      );
    }
    if (status === "success") {
      return (
        <span
          className="inline-block text-xs shrink-0 font-bold"
          style={{ color: "#4ade80" }}
          aria-label="success"
        >
          ✓
        </span>
      );
    }
    // error
    return (
      <span
        className="inline-block text-xs shrink-0 font-bold"
        style={{ color: "#f87171" }}
        aria-label="error"
        title={data?.error ?? "Tool call failed"}
      >
        ✕
      </span>
    );
  };

  return (
    <div
      className="mt-1 animate-fadeIn"
      data-testid={`tool-call-chip-${id}`}
      style={{ animationDuration: "200ms" }}
    >
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
        aria-controls={`tool-call-details-${id}`}
        className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs transition-all w-full text-left"
        style={{
          background: expanded
            ? "rgba(139,92,246,0.14)"
            : "rgba(139,92,246,0.07)",
          border: "1px solid rgba(139,92,246,0.18)",
          color: "#a78bfa",
        }}
      >
        <span
          aria-hidden="true"
          style={{ color: "#8b5cf6", fontSize: "0.7rem" }}
        >
          ✦
        </span>
        <span className="flex-1 font-medium">{label}</span>
        {statusIndicator()}
        <span
          className="text-xs ml-1"
          style={{
            color: "#6e6890",
            transform: expanded ? "rotate(180deg)" : "none",
            display: "inline-block",
            transition: "transform 0.15s",
          }}
          aria-hidden="true"
        >
          ▾
        </span>
      </button>

      {expanded && (
        <div
          id={`tool-call-details-${id}`}
          role="region"
          aria-label={`Details for ${label}`}
          className="mt-1 rounded-lg px-3 py-2 text-xs overflow-auto"
          style={{
            background: "rgba(8,6,15,0.8)",
            border: "1px solid rgba(139,92,246,0.12)",
            color: "#c4b5fd",
            fontFamily: "JetBrains Mono, monospace",
            maxHeight: 200,
            whiteSpace: "pre-wrap",
            wordBreak: "break-all",
          }}
        >
          {JSON.stringify(args, null, 2)}
          {status === "error" && data?.error && (
            <div
              className="mt-2 pt-2"
              style={{
                borderTop: "1px solid rgba(239,68,68,0.25)",
                color: "#f87171",
              }}
            >
              Error: {data.error}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
