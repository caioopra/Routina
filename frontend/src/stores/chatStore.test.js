import { describe, it, expect, beforeEach } from "vitest";
import { useChatStore } from "./chatStore";
import { useAuthStore } from "./authStore";
import {
  seedConversations,
  seedChatMessages,
  resetMockState,
} from "../test/mocks/handlers";

function resetStore() {
  useChatStore.setState({
    conversations: [],
    activeConversationId: null,
    messages: {},
    streaming: false,
    pendingTokens: "",
    error: null,
  });
}

describe("chatStore", () => {
  beforeEach(() => {
    resetMockState();
    resetStore();
    useAuthStore.setState({
      user: { id: "u1", email: "a@b.com", name: "A" },
      token: "token-1",
      refreshToken: "refresh-1",
    });
  });

  // ── loadConversations ──────────────────────────────────────────────────────

  it("loadConversations populates the conversations list", async () => {
    seedConversations([
      {
        id: "conv-1",
        routine_id: "r-1",
        title: "Chat 1",
        created_at: "2026-01-01T00:00:00Z",
      },
      {
        id: "conv-2",
        routine_id: "r-1",
        title: "Chat 2",
        created_at: "2026-01-02T00:00:00Z",
      },
    ]);

    await useChatStore.getState().loadConversations();

    const { conversations } = useChatStore.getState();
    expect(conversations).toHaveLength(2);
  });

  it("loadConversations stores empty array when none exist", async () => {
    await useChatStore.getState().loadConversations();
    expect(useChatStore.getState().conversations).toHaveLength(0);
  });

  // ── createConversation ─────────────────────────────────────────────────────

  it("createConversation creates a conversation and sets it active", async () => {
    const conv = await useChatStore.getState().createConversation("routine-1");

    expect(conv).toHaveProperty("id");
    expect(conv.routine_id).toBe("routine-1");

    const state = useChatStore.getState();
    expect(state.activeConversationId).toBe(conv.id);
    expect(state.conversations).toHaveLength(1);
    expect(state.messages[conv.id]).toEqual([]);
  });

  // ── openConversation ───────────────────────────────────────────────────────

  it("openConversation sets activeConversationId and loads messages", async () => {
    seedConversations([
      {
        id: "conv-10",
        routine_id: "r-1",
        title: "Test",
        created_at: "2026-01-01T00:00:00Z",
      },
    ]);
    seedChatMessages([
      {
        id: "m-1",
        conversation_id: "conv-10",
        role: "user",
        content: "Hello",
        created_at: "2026-01-01T00:00:01Z",
      },
      {
        id: "m-2",
        conversation_id: "conv-10",
        role: "assistant",
        content: "Hi!",
        created_at: "2026-01-01T00:00:02Z",
      },
    ]);

    await useChatStore.getState().openConversation("conv-10");

    const state = useChatStore.getState();
    expect(state.activeConversationId).toBe("conv-10");
    expect(state.messages["conv-10"]).toHaveLength(2);
    expect(state.messages["conv-10"][0].content).toBe("Hello");
  });

  it("openConversation does not re-fetch if messages already cached", async () => {
    seedConversations([
      {
        id: "conv-11",
        routine_id: "r-1",
        title: "Cached",
        created_at: "2026-01-01T00:00:00Z",
      },
    ]);

    // Pre-populate messages in store
    useChatStore.setState({
      messages: {
        "conv-11": [
          { id: "pre-1", role: "user", content: "Cached msg", created_at: "" },
        ],
      },
    });

    await useChatStore.getState().openConversation("conv-11");

    // Should still have only the pre-seeded message (no API re-fetch overwrote it)
    const msgs = useChatStore.getState().messages["conv-11"];
    expect(msgs).toHaveLength(1);
    expect(msgs[0].content).toBe("Cached msg");
  });

  // ── appendUserMessage ──────────────────────────────────────────────────────

  it("appendUserMessage adds a user message optimistically", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().appendUserMessage("Hello AI");

    const msgs = useChatStore.getState().messages[conv.id];
    expect(msgs).toHaveLength(1);
    expect(msgs[0].role).toBe("user");
    expect(msgs[0].content).toBe("Hello AI");
  });

  it("appendUserMessage does nothing when no active conversation", () => {
    useChatStore.setState({ activeConversationId: null, messages: {} });
    useChatStore.getState().appendUserMessage("Should be ignored");
    expect(Object.keys(useChatStore.getState().messages)).toHaveLength(0);
  });

  // ── token streaming ────────────────────────────────────────────────────────

  it("startStreaming sets streaming=true and clears pendingTokens", () => {
    useChatStore.setState({ pendingTokens: "stale", streaming: false });
    useChatStore.getState().startStreaming();
    const s = useChatStore.getState();
    expect(s.streaming).toBe(true);
    expect(s.pendingTokens).toBe("");
  });

  it("appendToken accumulates text in pendingTokens", () => {
    useChatStore.getState().startStreaming();
    useChatStore.getState().appendToken("Hello ");
    useChatStore.getState().appendToken("world");
    expect(useChatStore.getState().pendingTokens).toBe("Hello world");
  });

  it("finalizeAssistantMessage appends assembled text as an assistant message", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().startStreaming();
    useChatStore.getState().appendToken("Good ");
    useChatStore.getState().appendToken("morning!");
    useChatStore.getState().finalizeAssistantMessage();

    const state = useChatStore.getState();
    expect(state.streaming).toBe(false);
    expect(state.pendingTokens).toBe("");

    const msgs = state.messages[conv.id];
    const assistant = msgs.find((m) => m.role === "assistant");
    expect(assistant).toBeDefined();
    expect(assistant.content).toBe("Good morning!");
  });

  it("finalizeAssistantMessage does nothing when pendingTokens is empty", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");
    useChatStore.getState().startStreaming();
    // No tokens appended
    useChatStore.getState().finalizeAssistantMessage();

    const state = useChatStore.getState();
    expect(state.streaming).toBe(false);
    // No assistant message added
    const msgs = state.messages[conv.id];
    const assistant = msgs.find((m) => m.role === "assistant");
    expect(assistant).toBeUndefined();
  });

  // ── error handling ─────────────────────────────────────────────────────────

  it("setStreamingError sets streaming=false and stores error", () => {
    useChatStore.setState({ streaming: true, pendingTokens: "partial" });
    useChatStore.getState().setStreamingError("Connection failed");
    const state = useChatStore.getState();
    expect(state.streaming).toBe(false);
    expect(state.pendingTokens).toBe("");
    expect(state.error).toBe("Connection failed");
  });

  it("clearError resets the error field", () => {
    useChatStore.setState({ error: "Some error" });
    useChatStore.getState().clearError();
    expect(useChatStore.getState().error).toBeNull();
  });
});
