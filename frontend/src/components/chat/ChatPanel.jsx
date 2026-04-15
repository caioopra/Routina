import { useEffect, useRef, useState } from "react";
import { useChatStore } from "../../stores/chatStore";
import { useAuthStore } from "../../stores/authStore";
import { useBlockStore } from "../../stores/blockStore";
import { useRuleStore } from "../../stores/ruleStore";
import { useSSE } from "../../hooks/useSSE";
import Message from "./Message";
import Composer from "./Composer";
import ConversationList from "./ConversationList";
import PlannerContextEditor from "./PlannerContextEditor";
import ProviderToggle from "./ProviderToggle";
import ToolCallChip from "./ToolCallChip";

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
  const toolCalls = useChatStore((s) => s.toolCalls);
  const error = useChatStore((s) => s.error);

  const loadConversations = useChatStore((s) => s.loadConversations);
  const openConversation = useChatStore((s) => s.openConversation);
  const createConversation = useChatStore((s) => s.createConversation);
  const appendUserMessage = useChatStore((s) => s.appendUserMessage);
  const startStreaming = useChatStore((s) => s.startStreaming);
  const appendToken = useChatStore((s) => s.appendToken);
  const setProvider = useChatStore((s) => s.setProvider);
  const receiveToolCall = useChatStore((s) => s.receiveToolCall);
  const receiveToolResult = useChatStore((s) => s.receiveToolResult);
  const finalizeAssistantMessage = useChatStore(
    (s) => s.finalizeAssistantMessage,
  );
  const setStreamingError = useChatStore((s) => s.setStreamingError);
  const clearError = useChatStore((s) => s.clearError);

  const loadProviders = useAuthStore((s) => s.loadProviders);

  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [contextEditorOpen, setContextEditorOpen] = useState(false);

  const messageListRef = useRef(null);

  const cancelStreaming = useChatStore((s) => s.cancelStreaming);
  const getLastUserMessage = useChatStore((s) => s.getLastUserMessage);
  const popLastUserMessage = useChatStore((s) => s.popLastUserMessage);

  const { start: startSSE, cancel: cancelSSE } = useSSE("/api/chat/message");

  // Load conversations for this routine on mount
  useEffect(() => {
    loadConversations();
  }, [loadConversations]);

  // Load providers on first open
  useEffect(() => {
    loadProviders();
  }, [loadProviders]);

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    const el = messageListRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [messages, pendingTokens, toolCalls]);

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
    // Capture the conversation id *now* so every SSE callback below closes over
    // the correct value even if the user switches conversations mid-stream.
    startStreaming(convId);

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
              typeof data === "string"
                ? data
                : (data?.data ?? data?.text ?? "");
            appendToken(convId, tokenText);
          } else if (eventType === "provider") {
            setProvider(data?.name ?? data);
          } else if (eventType === "tool_call") {
            receiveToolCall({
              conversationId: convId,
              id: data.id,
              name: data.name,
              args: data.args ?? {},
            });
          } else if (eventType === "tool_result") {
            receiveToolResult({
              conversationId: convId,
              id: data.id,
              success: data.success,
              data: data.data ?? null,
            });
          } else if (eventType === "routine_updated") {
            const updatedRoutineId = data?.routine_id;
            if (updatedRoutineId && updatedRoutineId === routineId) {
              useBlockStore.getState().fetchByRoutine(updatedRoutineId);
              useRuleStore.getState().fetchByRoutine(updatedRoutineId);
            }
          } else if (eventType === "error") {
            const msg =
              typeof data === "string"
                ? data
                : (data?.message ?? "Stream error");
            setStreamingError(msg);
          }
        },
        onDone() {
          finalizeAssistantMessage(convId);
        },
      },
    );
  }

  function handleCancel() {
    cancelSSE();
    cancelStreaming();
  }

  function handleRetry() {
    clearError();
    const lastMsg = getLastUserMessage();
    if (!lastMsg) return;
    // Remove the existing optimistic user bubble so handleSend doesn't duplicate it.
    popLastUserMessage(lastMsg.content);
    handleSend(lastMsg.content);
  }

  const UNDO_PHRASE = "desfazer a última ação";

  function handleUndo() {
    if (streaming || !activeConversationId) return;
    handleSend(UNDO_PHRASE);
  }

  const activeMessages = activeConversationId
    ? (messages[activeConversationId] ?? [])
    : [];

  const activeToolCalls = activeConversationId
    ? (toolCalls[activeConversationId] ?? {})
    : {};

  // Conversations filtered for this routine (backend will only return owned ones,
  // but we keep all since the store is per-user)
  const routineConversations = conversations.filter(
    (c) => !routineId || c.routine_id === routineId,
  );

  // Sorted tool call entries for the current turn
  const toolCallEntries = Object.entries(activeToolCalls);

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
            <ProviderToggle />
            <button
              type="button"
              onClick={handleUndo}
              disabled={streaming || !activeConversationId}
              aria-label="Undo last action"
              className="text-xs px-2 py-1 rounded-lg transition-all"
              style={{
                background: "rgba(139,92,246,0.08)",
                color:
                  streaming || !activeConversationId ? "#4a4270" : "#a78bfa",
                border: "1px solid rgba(139,92,246,0.15)",
                cursor:
                  streaming || !activeConversationId ? "default" : "pointer",
                opacity: streaming || !activeConversationId ? 0.5 : 1,
              }}
            >
              Undo
            </button>
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

              {/* Tool call chips rendered during/after streaming */}
              {toolCallEntries.length > 0 && (
                <div
                  className="flex justify-start mb-2 pl-8"
                  data-testid="tool-calls-container"
                >
                  <div className="flex flex-col gap-1 w-full max-w-[80%]">
                    {toolCallEntries.map(([id, tc]) => (
                      <ToolCallChip
                        key={id}
                        id={id}
                        name={tc.name}
                        args={tc.args}
                        status={tc.status}
                        data={tc.data}
                      />
                    ))}
                  </div>
                </div>
              )}

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
                className="mx-4 mb-2 text-xs px-3 py-2 rounded-lg flex items-center justify-between gap-2"
                style={{
                  background: "rgba(239,68,68,0.1)",
                  border: "1px solid rgba(239,68,68,0.25)",
                  color: "#f87171",
                }}
              >
                <span className="flex-1">{error}</span>
                <div className="flex items-center gap-1 shrink-0">
                  {getLastUserMessage() && (
                    <button
                      type="button"
                      onClick={handleRetry}
                      aria-label="Retry last message"
                      className="text-xs px-2 py-0.5 rounded-md transition-all"
                      style={{
                        background: "rgba(239,68,68,0.15)",
                        border: "1px solid rgba(239,68,68,0.3)",
                        color: "#fca5a5",
                        cursor: "pointer",
                      }}
                    >
                      Retry
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={clearError}
                    aria-label="Dismiss error"
                    className="text-xs"
                  >
                    ×
                  </button>
                </div>
              </div>
            )}

            <Composer
              onSend={handleSend}
              onCancel={handleCancel}
              disabled={streaming}
              streaming={streaming}
            />
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
