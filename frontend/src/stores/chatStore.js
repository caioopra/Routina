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
 *   pendingTokens: string   -- assembling the in-flight assistant message
 *   error: string | null
 */
export const useChatStore = create((set, get) => ({
  conversations: [],
  activeConversationId: null,
  messages: {},
  streaming: false,
  pendingTokens: "",
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
    set({ activeConversationId: id });
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
      }));
      return conv;
    } catch (err) {
      set({ error: extractError(err, "Failed to create conversation") });
      throw err;
    }
  },

  // ── Messaging ────────────────────────────────────────────────────────────

  /**
   * sendMessage — called externally with the user's text. Returns the
   * `start` function from useSSE — callers should wire this up; the store
   * manages optimistic message append and token accumulation.
   *
   * Because Zustand stores cannot use React hooks, the actual SSE invocation
   * is performed by the ChatPanel component via `useSSE`. This action only
   * manages the message list state transitions.
   */

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

  startStreaming: () => {
    set({ streaming: true, pendingTokens: "" });
  },

  appendToken: (token) => {
    set((s) => ({ pendingTokens: s.pendingTokens + token }));
  },

  finalizeAssistantMessage: () => {
    const id = get().activeConversationId;
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
