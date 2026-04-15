import { create } from "zustand";
import {
  listConversations,
  createConversation,
  getMessages,
} from "../api/conversations";
import { useAuthStore } from "./authStore";

function extractError(err, fallback) {
  return err?.response?.data?.error || err?.message || fallback;
}

/**
 * chatStore — manages conversations, messages, and streaming state.
 *
 * State shape:
 *   conversations: Array<{ id, routine_id, title, created_at }>
 *   activeConversationId: string | null
 *   messages: { [conversationId]: Array<{ id, role, content, created_at }> }
 *   streaming: boolean
 *   pendingTokens: string          — assembling the in-flight assistant message
 *   provider: string | null        — active LLM provider for current stream
 *   toolCalls: {
 *     [conversationId]: {
 *       [toolCallId]: { name, args, status: 'pending'|'success'|'error', data }
 *     }
 *   }
 *   error: string | null
 */
export const useChatStore = create((set, get) => ({
  conversations: [],
  activeConversationId: null,
  messages: {},
  streaming: false,
  pendingTokens: "",
  provider: null,
  toolCalls: {},
  error: null,

  // ── Conversation management ──────────────────────────────────────────────

  loadConversations: async () => {
    try {
      const data = await listConversations();
      set({ conversations: data ?? [] });
    } catch (err) {
      set({ error: extractError(err, "Failed to load conversations") });
    }
  },

  openConversation: async (id) => {
    // Clear tool calls for the newly opened conversation (fresh turn)
    set((s) => ({
      activeConversationId: id,
      toolCalls: { ...s.toolCalls, [id]: {} },
    }));
    const existing = get().messages[id];
    if (!existing) {
      try {
        const msgs = await getMessages(id);
        set((s) => ({
          messages: { ...s.messages, [id]: msgs ?? [] },
        }));
      } catch (err) {
        set({ error: extractError(err, "Failed to load messages") });
      }
    }
  },

  createConversation: async (routineId) => {
    try {
      const conv = await createConversation({ routine_id: routineId });
      set((s) => ({
        conversations: [conv, ...s.conversations],
        activeConversationId: conv.id,
        messages: { ...s.messages, [conv.id]: [] },
        toolCalls: { ...s.toolCalls, [conv.id]: {} },
      }));
      return conv;
    } catch (err) {
      set({ error: extractError(err, "Failed to create conversation") });
      throw err;
    }
  },

  // ── Messaging ────────────────────────────────────────────────────────────

  appendUserMessage: (text) => {
    const id = get().activeConversationId;
    if (!id) return;
    const userMsg = {
      id: `optimistic-${Date.now()}`,
      role: "user",
      content: text,
      created_at: new Date().toISOString(),
    };
    set((s) => ({
      messages: {
        ...s.messages,
        [id]: [...(s.messages[id] ?? []), userMsg],
      },
    }));
  },

  startStreaming: (conversationId) => {
    // Clear tool calls for the target conversation when a new turn starts.
    // Falls back to activeConversationId for backwards compat.
    const id = conversationId ?? get().activeConversationId;
    set((s) => ({
      streaming: true,
      pendingTokens: "",
      provider: null,
      toolCalls: id ? { ...s.toolCalls, [id]: {} } : s.toolCalls,
    }));
  },

  appendToken: (conversationId, token) => {
    // conversationId is accepted but pendingTokens is a single scalar; the
    // caller (ChatPanel) already guards that only the originating conv writes.
    set((s) => ({ pendingTokens: s.pendingTokens + token }));
  },

  setProvider: (providerName) => {
    set({ provider: providerName });
  },

  /**
   * receiveToolCall — add a new tool call in 'pending' status.
   */
  receiveToolCall: ({ conversationId, id, name, args }) => {
    set((s) => {
      const convCalls = s.toolCalls[conversationId] ?? {};
      return {
        toolCalls: {
          ...s.toolCalls,
          [conversationId]: {
            ...convCalls,
            [id]: { name, args, status: "pending", data: null },
          },
        },
      };
    });
  },

  /**
   * receiveToolResult — update an existing tool call with its result.
   */
  receiveToolResult: ({ conversationId, id, success, data }) => {
    set((s) => {
      const convCalls = s.toolCalls[conversationId] ?? {};
      const existing = convCalls[id];
      if (!existing) return s;
      return {
        toolCalls: {
          ...s.toolCalls,
          [conversationId]: {
            ...convCalls,
            [id]: {
              ...existing,
              status: success ? "success" : "error",
              data: data ?? null,
            },
          },
        },
      };
    });
  },

  finalizeAssistantMessage: (conversationId) => {
    // Use the supplied conversationId to avoid a race when the user switches
    // conversations while a stream is in flight.  Falls back to
    // activeConversationId for backwards compat.
    const id = conversationId ?? get().activeConversationId;
    const { pendingTokens } = get();
    if (!id || !pendingTokens) {
      set({ streaming: false, pendingTokens: "" });
      return;
    }
    const assistantMsg = {
      id: `assistant-${Date.now()}`,
      role: "assistant",
      content: pendingTokens,
      created_at: new Date().toISOString(),
    };
    set((s) => ({
      streaming: false,
      pendingTokens: "",
      // Clear stale chips for this conversation once the turn is done.
      toolCalls: { ...s.toolCalls, [id]: {} },
      messages: {
        ...s.messages,
        [id]: [...(s.messages[id] ?? []), assistantMsg],
      },
    }));
  },

  setStreamingError: (errorMsg) => {
    set({ streaming: false, pendingTokens: "", error: errorMsg });
  },

  clearError: () => set({ error: null }),
}));

export default useChatStore;
