/**
 * Message — renders a single chat bubble.
 *
 * Props:
 *   role: 'user' | 'assistant'
 *   content: string
 *   streaming?: boolean  — when true, shows a blinking cursor at the end
 */
export default function Message({ role, content, streaming = false }) {
  const isUser = role === "user";

  return (
    <div
      className={`flex ${isUser ? "justify-end" : "justify-start"} mb-3`}
      data-testid={`message-${role}`}
    >
      {!isUser && (
        <div
          className="w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold shrink-0 mr-2 mt-0.5"
          style={{ background: "rgba(139,92,246,0.25)", color: "#c4b5fd" }}
          aria-hidden="true"
        >
          AI
        </div>
      )}

      <div
        className="max-w-[80%] rounded-2xl px-4 py-2.5 text-sm leading-relaxed whitespace-pre-wrap break-words"
        style={
          isUser
            ? {
                background: "linear-gradient(135deg, #1e1836, #2d1f5e)",
                color: "#e2e0f0",
                border: "1px solid rgba(139,92,246,0.3)",
                borderBottomRightRadius: 4,
              }
            : {
                background: "#0f0c1a",
                color: "#c4b5fd",
                border: "1px solid rgba(139,92,246,0.15)",
                borderBottomLeftRadius: 4,
              }
        }
      >
        {content}
        {streaming && (
          <span
            aria-label="streaming"
            className="inline-block w-0.5 h-3.5 ml-0.5 bg-current align-middle animate-pulse"
          />
        )}
      </div>

      {isUser && (
        <div
          className="w-6 h-6 rounded-full flex items-center justify-center text-xs font-bold shrink-0 ml-2 mt-0.5"
          style={{ background: "rgba(139,92,246,0.15)", color: "#a78bfa" }}
          aria-hidden="true"
        >
          U
        </div>
      )}
    </div>
  );
}
