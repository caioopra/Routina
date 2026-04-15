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
    provider: null,
    toolCalls: {},
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

  it("startStreaming accepts an explicit conversationId and clears its tool calls", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().receiveToolCall({
      conversationId: conv.id,
      id: "tc-pre",
      name: "create_block",
      args: {},
    });

    // Start streaming with the explicit id (simulates race-safe usage from ChatPanel)
    useChatStore.getState().startStreaming(conv.id);

    const { toolCalls, streaming } = useChatStore.getState();
    expect(streaming).toBe(true);
    expect(toolCalls[conv.id]).toEqual({});
  });

  it("appendToken accumulates text in pendingTokens", () => {
    useChatStore.getState().startStreaming();
    useChatStore.getState().appendToken("conv-x", "Hello ");
    useChatStore.getState().appendToken("conv-x", "world");
    expect(useChatStore.getState().pendingTokens).toBe("Hello world");
  });

  it("finalizeAssistantMessage appends assembled text as an assistant message", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().startStreaming(conv.id);
    useChatStore.getState().appendToken(conv.id, "Good ");
    useChatStore.getState().appendToken(conv.id, "morning!");
    useChatStore.getState().finalizeAssistantMessage(conv.id);

    const state = useChatStore.getState();
    expect(state.streaming).toBe(false);
    expect(state.pendingTokens).toBe("");

    const msgs = state.messages[conv.id];
    const assistant = msgs.find((m) => m.role === "assistant");
    expect(assistant).toBeDefined();
    expect(assistant.content).toBe("Good morning!");
  });

  it("finalizeAssistantMessage clears tool-call chips for the conversation", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().receiveToolCall({
      conversationId: conv.id,
      id: "tc-fin",
      name: "create_block",
      args: {},
    });
    useChatStore.getState().receiveToolResult({
      conversationId: conv.id,
      id: "tc-fin",
      success: true,
      data: {},
    });

    useChatStore.getState().startStreaming(conv.id);
    useChatStore.getState().appendToken(conv.id, "Done.");
    useChatStore.getState().finalizeAssistantMessage(conv.id);

    const { toolCalls } = useChatStore.getState();
    expect(toolCalls[conv.id]).toEqual({});
  });

  it("finalizeAssistantMessage with explicit conversationId writes to that conv even when active differs", async () => {
    const conv1 = await useChatStore.getState().createConversation("r-1");
    const conv2 = await useChatStore.getState().createConversation("r-1");
    // conv2 is now active; simulate that stream was started for conv1
    useChatStore.setState({ pendingTokens: "from conv1", streaming: true });

    useChatStore.getState().finalizeAssistantMessage(conv1.id);

    const state = useChatStore.getState();
    expect(state.streaming).toBe(false);
    // Message must land in conv1, not conv2
    const conv1Msgs = state.messages[conv1.id];
    const conv2Msgs = state.messages[conv2.id];
    expect(conv1Msgs.find((m) => m.role === "assistant")).toBeDefined();
    expect(conv2Msgs.find((m) => m.role === "assistant")).toBeUndefined();
  });

  it("finalizeAssistantMessage does nothing when pendingTokens is empty", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");
    useChatStore.getState().startStreaming(conv.id);
    // No tokens appended
    useChatStore.getState().finalizeAssistantMessage(conv.id);

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

  // ── tool-call state ────────────────────────────────────────────────────────

  it("receiveToolCall adds a pending tool call entry", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().receiveToolCall({
      conversationId: conv.id,
      id: "tc-1",
      name: "create_block",
      args: { title: "Test" },
    });

    const { toolCalls } = useChatStore.getState();
    expect(toolCalls[conv.id]["tc-1"]).toMatchObject({
      name: "create_block",
      args: { title: "Test" },
      status: "pending",
      data: null,
    });
  });

  it("receiveToolResult updates status to success", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().receiveToolCall({
      conversationId: conv.id,
      id: "tc-2",
      name: "update_block",
      args: {},
    });

    useChatStore.getState().receiveToolResult({
      conversationId: conv.id,
      id: "tc-2",
      success: true,
      data: { id: "block-42" },
    });

    const { toolCalls } = useChatStore.getState();
    expect(toolCalls[conv.id]["tc-2"].status).toBe("success");
    expect(toolCalls[conv.id]["tc-2"].data).toEqual({ id: "block-42" });
  });

  it("receiveToolResult updates status to error on failure", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().receiveToolCall({
      conversationId: conv.id,
      id: "tc-3",
      name: "delete_block",
      args: {},
    });

    useChatStore.getState().receiveToolResult({
      conversationId: conv.id,
      id: "tc-3",
      success: false,
      data: { error: "Not found" },
    });

    const { toolCalls } = useChatStore.getState();
    expect(toolCalls[conv.id]["tc-3"].status).toBe("error");
    expect(toolCalls[conv.id]["tc-3"].data).toEqual({ error: "Not found" });
  });

  it("receiveToolResult is a no-op when tool call id does not exist", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    // Should not throw
    useChatStore.getState().receiveToolResult({
      conversationId: conv.id,
      id: "non-existent",
      success: true,
      data: {},
    });

    const { toolCalls } = useChatStore.getState();
    expect(toolCalls[conv.id]["non-existent"]).toBeUndefined();
  });

  it("startStreaming clears tool calls for the active conversation (fallback path)", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.getState().receiveToolCall({
      conversationId: conv.id,
      id: "tc-old",
      name: "create_block",
      args: {},
    });

    // No explicit id — falls back to activeConversationId
    useChatStore.getState().startStreaming();

    const { toolCalls } = useChatStore.getState();
    expect(toolCalls[conv.id]).toEqual({});
  });

  it("openConversation clears tool calls for that conversation", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");

    useChatStore.setState({
      toolCalls: {
        [conv.id]: { "tc-stale": { name: "x", status: "success" } },
      },
    });

    await useChatStore.getState().openConversation(conv.id);

    const { toolCalls } = useChatStore.getState();
    expect(toolCalls[conv.id]).toEqual({});
  });

  it("setProvider stores the active provider name", () => {
    useChatStore.getState().setProvider("claude");
    expect(useChatStore.getState().provider).toBe("claude");
  });

  // ── cancelStreaming ────────────────────────────────────────────────────────

  it("cancelStreaming sets streaming=false and clears pendingTokens without setting error", () => {
    useChatStore.setState({
      streaming: true,
      pendingTokens: "partial response so far",
    });
    useChatStore.getState().cancelStreaming();
    const state = useChatStore.getState();
    expect(state.streaming).toBe(false);
    expect(state.pendingTokens).toBe("");
    expect(state.error).toBeNull();
  });

  // ── getLastUserMessage ─────────────────────────────────────────────────────

  it("getLastUserMessage returns the most recent user message", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");
    useChatStore.setState({
      messages: {
        [conv.id]: [
          { id: "m-1", role: "user", content: "First message", created_at: "" },
          {
            id: "m-2",
            role: "assistant",
            content: "Response",
            created_at: "",
          },
          {
            id: "m-3",
            role: "user",
            content: "Second message",
            created_at: "",
          },
        ],
      },
    });

    const msg = useChatStore.getState().getLastUserMessage();
    expect(msg).not.toBeNull();
    expect(msg.content).toBe("Second message");
  });

  it("getLastUserMessage returns null when messages contain only assistant messages", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");
    useChatStore.setState({
      messages: {
        [conv.id]: [
          {
            id: "m-1",
            role: "assistant",
            content: "Hello!",
            created_at: "",
          },
        ],
      },
    });

    const msg = useChatStore.getState().getLastUserMessage();
    expect(msg).toBeNull();
  });

  it("getLastUserMessage returns null when there is no active conversation", () => {
    useChatStore.setState({ activeConversationId: null, messages: {} });
    const msg = useChatStore.getState().getLastUserMessage();
    expect(msg).toBeNull();
  });

  it("getLastUserMessage returns null when messages array is empty", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");
    // messages[conv.id] is already [] from createConversation
    const msg = useChatStore.getState().getLastUserMessage();
    expect(msg).toBeNull();
  });

  // ── popLastUserMessage (Fix 3) ─────────────────────────────────────────────

  it("popLastUserMessage removes the last user message when text matches", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");
    useChatStore.setState({
      messages: {
        [conv.id]: [
          { id: "m-1", role: "user", content: "First", created_at: "" },
          { id: "m-2", role: "user", content: "Second", created_at: "" },
        ],
      },
    });

    useChatStore.getState().popLastUserMessage("Second");

    const msgs = useChatStore.getState().messages[conv.id];
    expect(msgs).toHaveLength(1);
    expect(msgs[0].content).toBe("First");
  });

  it("popLastUserMessage is a no-op when text does not match last user message", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");
    useChatStore.setState({
      messages: {
        [conv.id]: [
          { id: "m-1", role: "user", content: "Original", created_at: "" },
        ],
      },
    });

    useChatStore.getState().popLastUserMessage("Different text");

    const msgs = useChatStore.getState().messages[conv.id];
    expect(msgs).toHaveLength(1);
    expect(msgs[0].content).toBe("Original");
  });

  it("popLastUserMessage is a no-op when there is no active conversation", () => {
    useChatStore.setState({ activeConversationId: null, messages: {} });
    // Should not throw
    useChatStore.getState().popLastUserMessage("anything");
    expect(Object.keys(useChatStore.getState().messages)).toHaveLength(0);
  });

  it("popLastUserMessage skips assistant messages to find the last user message", async () => {
    const conv = await useChatStore.getState().createConversation("r-1");
    useChatStore.setState({
      messages: {
        [conv.id]: [
          { id: "m-1", role: "user", content: "Ask", created_at: "" },
          { id: "m-2", role: "assistant", content: "Reply", created_at: "" },
        ],
      },
    });

    useChatStore.getState().popLastUserMessage("Ask");

    const msgs = useChatStore.getState().messages[conv.id];
    expect(msgs).toHaveLength(1);
    expect(msgs[0].role).toBe("assistant");
  });
});
