import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, beforeEach, vi } from "vitest";
import ChatPanel from "./ChatPanel";
import { useChatStore } from "../../stores/chatStore";
import { useAuthStore } from "../../stores/authStore";
import { useBlockStore } from "../../stores/blockStore";
import { useRuleStore } from "../../stores/ruleStore";
import { resetMockState } from "../../test/mocks/handlers";

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

function renderPanel(routineId = "routine-1") {
  return render(<ChatPanel routineId={routineId} />);
}

describe("ChatPanel", () => {
  beforeEach(() => {
    resetMockState();
    resetStore();
    useAuthStore.setState({
      user: { id: "u1", email: "a@b.com", name: "A", planner_context: null },
      token: "token-1",
      refreshToken: "refresh-1",
    });
  });

  it("renders the panel with AI Assistant header", () => {
    renderPanel();
    expect(screen.getByText("AI Assistant")).toBeInTheDocument();
  });

  it("renders empty-state prompt when no messages", () => {
    renderPanel();
    expect(screen.getByText(/ask the ai to help/i)).toBeInTheDocument();
  });

  it("renders the composer textarea and send button", () => {
    renderPanel();
    expect(
      screen.getByRole("textbox", { name: /message input/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /send message/i }),
    ).toBeInTheDocument();
  });

  it("user types a message and it appears as a user bubble", async () => {
    const user = userEvent.setup();
    renderPanel();

    const input = screen.getByRole("textbox", { name: /message input/i });
    await user.type(input, "Add a morning run block");
    await user.keyboard("{Enter}");

    // Optimistic user bubble should appear
    await waitFor(() => {
      expect(screen.getByText("Add a morning run block")).toBeInTheDocument();
    });
  });

  it("shows a streaming assistant bubble after sending", async () => {
    const user = userEvent.setup();
    renderPanel();

    const input = screen.getByRole("textbox", { name: /message input/i });
    await user.type(input, "Help me plan");
    await user.keyboard("{Enter}");

    // After the MSW SSE handler resolves, we should see finalized assistant message
    await waitFor(
      () => {
        // The mock emits: "Sure! I can help you with that routine. Let me know what changes you would like to make."
        expect(screen.getByText(/sure!/i)).toBeInTheDocument();
      },
      { timeout: 3000 },
    );
  });

  it("shows Stop button (not Send) while streaming, and textarea is disabled", async () => {
    // Manually set streaming state
    useChatStore.setState({ streaming: true });
    renderPanel();

    // Send is replaced by Stop when streaming
    expect(
      screen.queryByRole("button", { name: /send message/i }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /stop streaming/i }),
    ).toBeInTheDocument();

    // Textarea is still disabled
    expect(
      screen.getByRole("textbox", { name: /message input/i }),
    ).toBeDisabled();
  });

  it("toggles the conversation list sidebar", async () => {
    const user = userEvent.setup();
    renderPanel();

    const toggleBtn = screen.getByRole("button", {
      name: /toggle conversation list/i,
    });
    // Sidebar is hidden by default
    expect(screen.queryByText(/conversations/i)).not.toBeInTheDocument();

    await user.click(toggleBtn);
    // Now sidebar should be visible
    expect(screen.getByText("Conversations")).toBeInTheDocument();
  });

  it("opens the planner context editor when Context button is clicked", async () => {
    const user = userEvent.setup();
    renderPanel();

    await user.click(
      screen.getByRole("button", { name: /edit planner context/i }),
    );
    expect(
      screen.getByRole("dialog", { name: /edit planner context/i }),
    ).toBeInTheDocument();
  });

  it("shows error alert when store has an error", () => {
    useChatStore.setState({ error: "Something went wrong" });
    renderPanel();
    expect(screen.getByRole("alert")).toHaveTextContent(
      /something went wrong/i,
    );
  });

  it("clears error when dismiss button is clicked", async () => {
    const user = userEvent.setup();
    useChatStore.setState({ error: "Some error" });
    renderPanel();

    await user.click(screen.getByRole("button", { name: /dismiss error/i }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("calls onClose when close button is clicked", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<ChatPanel routineId="r-1" onClose={onClose} />);

    await user.click(screen.getByRole("button", { name: /close chat panel/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("shows a 'New conversation' button in sidebar", async () => {
    const user = userEvent.setup();
    renderPanel();

    await user.click(
      screen.getByRole("button", { name: /toggle conversation list/i }),
    );
    expect(
      screen.getByRole("button", { name: /new conversation/i }),
    ).toBeInTheDocument();
  });

  it("creates a new conversation when New button is clicked", async () => {
    const user = userEvent.setup();
    renderPanel();

    await user.click(
      screen.getByRole("button", { name: /toggle conversation list/i }),
    );
    await user.click(screen.getByRole("button", { name: /new conversation/i }));

    await waitFor(() => {
      expect(useChatStore.getState().activeConversationId).not.toBeNull();
    });
  });

  it("tool-call SSE path completes and assistant message contains the result", async () => {
    const user = userEvent.setup();
    renderPanel();

    const input = screen.getByRole("textbox", { name: /message input/i });
    await user.type(input, "create a block for me");
    await user.keyboard("{Enter}");

    // The mock emits tool_call + tool_result then finishes the stream.
    // After finalization, the assembled assistant message must be visible and
    // tool-call chips must be cleared (stale-chip-on-reopen fix).
    await waitFor(
      () => {
        expect(
          screen.getByText(/the block has been added/i),
        ).toBeInTheDocument();
      },
      { timeout: 3000 },
    );

    // Chips are cleared after finalization — none should remain in the DOM.
    expect(screen.queryByText("Created a block")).not.toBeInTheDocument();
    // Store must also reflect the cleared state.
    const convId = useChatStore.getState().activeConversationId;
    expect(useChatStore.getState().toolCalls[convId]).toEqual({});
  });

  it("routine_updated triggers fetchByRoutine on blockStore and ruleStore", async () => {
    const user = userEvent.setup();

    const fetchBlocks = vi
      .spyOn(useBlockStore.getState(), "fetchByRoutine")
      .mockResolvedValue(undefined);
    const fetchRules = vi
      .spyOn(useRuleStore.getState(), "fetchByRoutine")
      .mockResolvedValue(undefined);

    renderPanel("routine-1");

    const input = screen.getByRole("textbox", { name: /message input/i });
    await user.type(input, "create a block for me");
    await user.keyboard("{Enter}");

    await waitFor(
      () => {
        expect(fetchBlocks).toHaveBeenCalledWith("routine-1");
        expect(fetchRules).toHaveBeenCalledWith("routine-1");
      },
      { timeout: 3000 },
    );

    fetchBlocks.mockRestore();
    fetchRules.mockRestore();
  });

  it("renders ProviderToggle in header when providers are loaded", () => {
    useAuthStore.setState({
      providers: { available: ["gemini", "claude"], selected: "gemini" },
    });
    renderPanel();

    expect(
      screen.getByRole("group", { name: /llm provider/i }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /gemini/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /claude/i })).toBeInTheDocument();
  });

  // ── Stream-cancel (Task 1) ─────────────────────────────────────────────────

  it("shows Stop button instead of Send while streaming", () => {
    useChatStore.setState({ streaming: true });
    renderPanel();

    expect(
      screen.getByRole("button", { name: /stop streaming/i }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /send message/i }),
    ).not.toBeInTheDocument();
  });

  it("clicking Stop ends streaming and does not show an error banner", async () => {
    const user = userEvent.setup();
    useChatStore.setState({
      streaming: true,
      activeConversationId: "conv-x",
      conversations: [
        { id: "conv-x", routine_id: "routine-1", title: null, created_at: "" },
      ],
    });
    renderPanel();

    const stopBtn = screen.getByRole("button", { name: /stop streaming/i });
    await user.click(stopBtn);

    await waitFor(() => {
      expect(useChatStore.getState().streaming).toBe(false);
    });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  // ── Error retry (Task 2) ───────────────────────────────────────────────────

  it("shows Retry button inside the error banner when a prior user message exists", () => {
    useChatStore.setState({
      error: "Something went wrong",
      activeConversationId: "conv-r",
      messages: {
        "conv-r": [
          {
            id: "m-1",
            role: "user",
            content: "Please add a block",
            created_at: "",
          },
        ],
      },
    });
    renderPanel();

    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /retry last message/i }),
    ).toBeInTheDocument();
  });

  it("Retry button is hidden when no prior user message exists", () => {
    useChatStore.setState({
      error: "Something went wrong",
      activeConversationId: "conv-r2",
      messages: { "conv-r2": [] },
    });
    renderPanel();

    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /retry last message/i }),
    ).not.toBeInTheDocument();
  });

  it("clicking Retry resends the last user message text", async () => {
    const user = userEvent.setup();
    useChatStore.setState({
      error: "Connection error",
      activeConversationId: "conv-retry",
      conversations: [
        {
          id: "conv-retry",
          routine_id: "routine-1",
          title: null,
          created_at: "",
        },
      ],
      messages: {
        "conv-retry": [
          {
            id: "u-1",
            role: "user",
            content: "Help me plan my week",
            created_at: "",
          },
        ],
      },
    });
    renderPanel();

    const retryBtn = screen.getByRole("button", {
      name: /retry last message/i,
    });
    await user.click(retryBtn);

    // Error banner clears immediately on click
    await waitFor(() => {
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });

    // The retry sends as an optimistic user bubble, then eventually resolves
    await waitFor(
      () => {
        // The mock SSE should resolve and assistant reply appear
        expect(screen.getByText(/sure!/i)).toBeInTheDocument();
      },
      { timeout: 3000 },
    );
  });

  // ── handleRetry clears error first (Fix 2) ───────────────────────────────

  it("handleRetry clears the error state before resending", async () => {
    const user = userEvent.setup();
    useChatStore.setState({
      error: "Connection error",
      activeConversationId: "conv-fix2",
      conversations: [
        {
          id: "conv-fix2",
          routine_id: "routine-1",
          title: null,
          created_at: "",
        },
      ],
      messages: {
        "conv-fix2": [
          {
            id: "u-fix2",
            role: "user",
            content: "Help me plan",
            created_at: "",
          },
        ],
      },
    });
    renderPanel();

    // Error banner is visible
    expect(screen.getByRole("alert")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: /retry last message/i }),
    );

    // Error must be null immediately after click (clearError runs first)
    expect(useChatStore.getState().error).toBeNull();
    // Banner is gone from DOM
    await waitFor(() => {
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });
  });

  // ── handleRetry does not duplicate user message (Fix 3) ──────────────────

  it("handleRetry does not add a second user bubble", async () => {
    const user = userEvent.setup();
    useChatStore.setState({
      error: "Stream error",
      activeConversationId: "conv-nodup",
      conversations: [
        {
          id: "conv-nodup",
          routine_id: "routine-1",
          title: null,
          created_at: "",
        },
      ],
      messages: {
        "conv-nodup": [
          {
            id: "u-nodup",
            role: "user",
            content: "Add a block please",
            created_at: "",
          },
        ],
      },
    });
    renderPanel();

    await user.click(
      screen.getByRole("button", { name: /retry last message/i }),
    );

    // Wait for the stream to finalize so messages stabilize
    await waitFor(
      () => {
        expect(screen.getByText(/sure!/i)).toBeInTheDocument();
      },
      { timeout: 3000 },
    );

    // There must be exactly ONE "Add a block please" bubble
    const bubbles = screen.getAllByText("Add a block please");
    expect(bubbles).toHaveLength(1);
  });

  // ── Undo shortcut (Task 3) ─────────────────────────────────────────────────

  it("renders Undo button in header", () => {
    renderPanel();
    expect(
      screen.getByRole("button", { name: /undo last action/i }),
    ).toBeInTheDocument();
  });

  it("Undo button is disabled when streaming is true", () => {
    useChatStore.setState({
      streaming: true,
      activeConversationId: "conv-u",
    });
    renderPanel();

    expect(
      screen.getByRole("button", { name: /undo last action/i }),
    ).toBeDisabled();
  });

  it("Undo button is disabled when there is no active conversation", () => {
    useChatStore.setState({
      streaming: false,
      activeConversationId: null,
    });
    renderPanel();

    expect(
      screen.getByRole("button", { name: /undo last action/i }),
    ).toBeDisabled();
  });

  it("clicking Undo sends the undo phrase through the chat", async () => {
    const user = userEvent.setup();
    useChatStore.setState({
      streaming: false,
      activeConversationId: "conv-undo",
      conversations: [
        {
          id: "conv-undo",
          routine_id: "routine-1",
          title: null,
          created_at: "",
        },
      ],
      messages: { "conv-undo": [] },
    });
    renderPanel();

    const undoBtn = screen.getByRole("button", { name: /undo last action/i });
    await user.click(undoBtn);

    // The undo phrase should appear as a user bubble
    await waitFor(() => {
      expect(screen.getByText("desfazer a última ação")).toBeInTheDocument();
    });
  });
});
