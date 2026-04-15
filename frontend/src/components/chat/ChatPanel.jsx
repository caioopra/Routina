import { useEffect, useRef, useState } from "react";
import { useChatStore } from "../../stores/chatStore";
import { useSSE } from "../../hooks/useSSE";
import Message from "./Message";
import Composer from "./Composer";
import ConversationList from "./ConversationList";
import PlannerContextEditor from "./PlannerContextEditor";

/**
 * ChatPanel — full chat UI embedded in the routine detail page.
 *
 * Props:
 *   routineId: string
 *   onClose?: () => void
 */
export default function ChatPanel({ routineId, onClose }) {
  const conversations = useChatStore((s) => s.conversations);
  const activeConversationId = useChatStore((s) => s.activeConversationId);
  const messages = useChatStore((s) => s.messages);
  const streaming = useChatStore((s) => s.streaming);
  const pendingTokens = useChatStore((s) => s.pendingTokens);
  const error = useChatStore((s) => s.error);

  const loadConversations = useChatStore((s) => s.loadConversations);
  const openConversation = useChatStore((s) => s.openConversation);
  const createConversation = useChatStore((s) => s.createConversation);
  const appendUserMessage = useChatStore((s) => s.appendUserMessage);
  const startStreaming = useChatStore((s) => s.startStreaming);
  const appendToken = useChatStore((s) => s.appendToken);
  const finalizeAssistantMessage = useChatStore(
    (s) => s.finalizeAssistantMessage,
  );
  const setStreamingError = useChatStore((s) => s.setStreamingError);
  const clearError = useChatStore((s) => s.clearError);

  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [contextEditorOpen, setContextEditorOpen] = useState(false);

  const messageListRef = useRef(null);

  const { start: startSSE } = useSSE("/api/chat/message");

  // Load conversations for this routine on mount
  useEffect(() => {
    loadConversations();
  }, [loadConversations]);

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    const el = messageListRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [messages, pendingTokens]);

  async function handleNewConversation() {
    try {
      await createConversation(routineId);
    } catch {
      // error already stored in chatStore
    }
  }

  async function handleSelectConversation(id) {
    await openConversation(id);
    setSidebarOpen(false);
  }

  async function handleSend(text) {
    // Ensure we have an active conversation
    let convId = activeConversationId;
    if (!convId) {
      try {
        const conv = await createConversation(routineId);
        convId = conv.id;
      } catch {
        return;
      }
    }

    // Optimistic user message
    appendUserMessage(text);
    startStreaming();

    startSSE(
      {
        conversation_id: convId,
        message: text,
        routine_id: routineId,
      },
      {
        onEvent(eventType, data) {
          if (eventType === "token") {
            const tokenText =
              typeof data === "string" ? data : (data?.data ?? "");
            appendToken(tokenText);
          }
        },
        onDone() {
          finalizeAssistantMessage();
        },
      },
    );

    // Handle SSE-level error: the hook will set status='error', but we also
    // want the store to finalize gracefully.
  }

  const activeMessages = activeConversationId
    ? (messages[activeConversationId] ?? [])
    : [];

  // Conversations filtered for this routine (backend will only return owned ones,
  // but we keep all since the store is per-user)
  const routineConversations = conversations.filter(
    (c) => !routineId || c.routine_id === routineId,
  );

  return (
    <>
      <div
        className="flex flex-col h-full"
        style={{
          background: "#08060f",
          border: "1px solid rgba(139,92,246,0.18)",
          borderRadius: 16,
          overflow: "hidden",
        }}
        data-testid="chat-panel"
      >
        {/* Header */}
        <div
          className="flex items-center justify-between px-4 py-3 shrink-0 border-b"
          style={{ borderColor: "rgba(139,92,246,0.15)" }}
        >
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setSidebarOpen((v) => !v)}
              aria-label="Toggle conversation list"
              aria-expanded={sidebarOpen}
              className="text-xs px-2 py-1 rounded-lg transition-all"
              style={{
                background: sidebarOpen
                  ? "rgba(139,92,246,0.18)"
                  : "rgba(139,92,246,0.08)",
                color: "#c4b5fd",
                border: "1px solid rgba(139,92,246,0.2)",
              }}
            >
              Chats
            </button>
            <span
              className="font-display text-sm font-bold"
              style={{ color: "#e2e0f0" }}
            >
              AI Assistant
            </span>
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setContextEditorOpen(true)}
              aria-label="Edit planner context"
              className="text-xs px-2 py-1 rounded-lg transition-all"
              style={{
                background: "rgba(139,92,246,0.08)",
                color: "#8b5cf6",
                border: "1px solid rgba(139,92,246,0.15)",
              }}
            >
              Context
            </button>
            {onClose && (
              <button
                type="button"
                onClick={onClose}
                aria-label="Close chat panel"
                className="text-lg leading-none p-1 rounded-lg transition-colors"
                style={{ color: "#6e6890" }}
              >
                ×
              </button>
            )}
          </div>
        </div>

        <div className="flex flex-1 overflow-hidden">
          {/* Sidebar */}
          {sidebarOpen && (
            <div
              className="w-48 shrink-0 overflow-hidden"
              style={{ borderRight: "1px solid rgba(139,92,246,0.12)" }}
            >
              <ConversationList
                conversations={routineConversations}
                activeId={activeConversationId}
                onSelect={handleSelectConversation}
                onNew={handleNewConversation}
              />
            </div>
          )}

          {/* Main chat area */}
          <div className="flex flex-col flex-1 overflow-hidden">
            {/* Message list */}
            <div
              ref={messageListRef}
              className="flex-1 overflow-y-auto px-4 py-4"
              aria-live="polite"
              aria-label="Chat messages"
            >
              {activeMessages.length === 0 && !streaming && (
                <div
                  className="flex flex-col items-center justify-center h-full text-center gap-3"
                  style={{ minHeight: 120 }}
                >
                  <div
                    className="w-10 h-10 rounded-full flex items-center justify-center text-xl"
                    style={{
                      background: "rgba(139,92,246,0.12)",
                      color: "#8b5cf6",
                    }}
                  >
                    ✦
                  </div>
                  <p
                    className="text-sm font-medium"
                    style={{ color: "#8b5cf6" }}
                  >
                    Ask the AI to help you plan
                  </p>
                  <p className="text-xs" style={{ color: "#6e6890" }}>
                    It can create, update, or delete blocks and rules.
                  </p>
                </div>
              )}

              {activeMessages.map((msg) => (
                <Message key={msg.id} role={msg.role} content={msg.content} />
              ))}

              {/* Streaming assistant message */}
              {streaming && pendingTokens && (
                <Message
                  role="assistant"
                  content={pendingTokens}
                  streaming={true}
                />
              )}

              {streaming && !pendingTokens && (
                <div className="flex justify-start mb-3">
                  <div
                    className="rounded-2xl px-4 py-2.5 text-sm"
                    style={{
                      background: "#0f0c1a",
                      border: "1px solid rgba(139,92,246,0.15)",
                      color: "#6e6890",
                    }}
                  >
                    <span className="animate-pulse">Thinking…</span>
                  </div>
                </div>
              )}
            </div>

            {error && (
              <div
                role="alert"
                className="mx-4 mb-2 text-xs px-3 py-2 rounded-lg flex items-center justify-between"
                style={{
                  background: "rgba(239,68,68,0.1)",
                  border: "1px solid rgba(239,68,68,0.25)",
                  color: "#f87171",
                }}
              >
                <span>{error}</span>
                <button
                  type="button"
                  onClick={clearError}
                  aria-label="Dismiss error"
                  className="ml-2 text-xs"
                >
                  ×
                </button>
              </div>
            )}

            <Composer onSend={handleSend} disabled={streaming} />
          </div>
        </div>
      </div>

      <PlannerContextEditor
        open={contextEditorOpen}
        onClose={() => setContextEditorOpen(false)}
      />
    </>
  );
}
