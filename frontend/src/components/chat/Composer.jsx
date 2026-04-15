import { useRef, useState, useEffect } from "react";

/**
 * Composer — textarea + send/stop button.
 *
 * Props:
 *   onSend: (text: string) => void
 *   onCancel?: () => void        — called when Stop is clicked during streaming
 *   disabled?: boolean
 *   streaming?: boolean          — when true, shows Stop instead of Send
 *   placeholder?: string
 */
export default function Composer({
  onSend,
  onCancel,
  disabled = false,
  streaming = false,
  placeholder = "Ask the AI to edit your routine…",
}) {
  const [value, setValue] = useState("");
  const textareaRef = useRef(null);
  // Prevents onCancel from firing more than once per streaming session.
  const hasCancelledRef = useRef(false);

  // Reset the guard whenever a new streaming session begins.
  useEffect(() => {
    if (streaming) {
      hasCancelledRef.current = false;
    }
  }, [streaming]);

  function handleKeyDown(e) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  }

  function submit() {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setValue("");
    textareaRef.current?.focus();
  }

  return (
    <div
      className="flex items-end gap-2 p-3 border-t"
      style={{ borderColor: "rgba(139,92,246,0.15)" }}
    >
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder}
        disabled={disabled}
        rows={2}
        aria-label="Message input"
        className="flex-1 resize-none rounded-xl px-4 py-3 text-base leading-relaxed outline-none focus:ring-2 transition-all placeholder:text-[#7a7498]"
        style={{
          background: "#0f0c1a",
          color: "#f1eff8",
          border: "1px solid rgba(139,92,246,0.25)",
          fontFamily: "'DM Sans', sans-serif",
          minHeight: 64,
          maxHeight: 240,
          overflowY: "auto",
          "--tw-ring-color": "rgba(139,92,246,0.45)",
          opacity: disabled ? 0.5 : 1,
        }}
        onInput={(e) => {
          e.target.style.height = "auto";
          e.target.style.height = `${Math.min(e.target.scrollHeight, 240)}px`;
        }}
      />
      {streaming ? (
        <button
          type="button"
          onClick={() => {
            if (hasCancelledRef.current) return;
            hasCancelledRef.current = true;
            onCancel?.();
          }}
          disabled={!streaming}
          aria-label="Stop streaming"
          className="shrink-0 rounded-xl px-4 py-2.5 text-sm font-semibold transition-all"
          style={{
            background: streaming
              ? "rgba(239,68,68,0.15)"
              : "rgba(239,68,68,0.06)",
            color: streaming ? "#f87171" : "#7a5a5a",
            border: "1px solid rgba(239,68,68,0.3)",
            cursor: streaming ? "pointer" : "default",
            minHeight: 40,
            opacity: streaming ? 1 : 0.5,
          }}
        >
          Stop
        </button>
      ) : (
        <button
          type="button"
          onClick={submit}
          disabled={disabled || !value.trim()}
          aria-label="Send message"
          className="shrink-0 rounded-xl px-4 py-2.5 text-sm font-semibold transition-all"
          style={{
            background:
              disabled || !value.trim()
                ? "rgba(139,92,246,0.1)"
                : "linear-gradient(135deg, #7c3aed, #8b5cf6)",
            color: disabled || !value.trim() ? "#6e6890" : "#fff",
            border: "1px solid rgba(139,92,246,0.3)",
            cursor: disabled || !value.trim() ? "default" : "pointer",
            minHeight: 40,
          }}
        >
          Send
        </button>
      )}
    </div>
  );
}
