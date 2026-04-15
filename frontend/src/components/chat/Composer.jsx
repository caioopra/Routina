import { useRef, useState } from "react";

/**
 * Composer — textarea + send button.
 *
 * Props:
 *   onSend: (text: string) => void
 *   disabled?: boolean
 *   placeholder?: string
 */
export default function Composer({
  onSend,
  disabled = false,
  placeholder = "Ask the AI to edit your routine…",
}) {
  const [value, setValue] = useState("");
  const textareaRef = useRef(null);

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
        rows={1}
        aria-label="Message input"
        className="flex-1 resize-none rounded-xl px-3 py-2.5 text-sm leading-relaxed outline-none focus:ring-1 transition-all"
        style={{
          background: "#0f0c1a",
          color: "#e2e0f0",
          border: "1px solid rgba(139,92,246,0.2)",
          fontFamily: "'DM Sans', sans-serif",
          minHeight: 40,
          maxHeight: 120,
          overflowY: "auto",
          "--tw-ring-color": "rgba(139,92,246,0.4)",
          opacity: disabled ? 0.5 : 1,
        }}
        onInput={(e) => {
          // Auto-resize
          e.target.style.height = "auto";
          e.target.style.height = `${Math.min(e.target.scrollHeight, 120)}px`;
        }}
      />
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
    </div>
  );
}
