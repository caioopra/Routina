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

  it("disables the send button while streaming", async () => {
    // Manually set streaming state
    useChatStore.setState({ streaming: true });
    renderPanel();

    const sendBtn = screen.getByRole("button", { name: /send message/i });
    expect(sendBtn).toBeDisabled();
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
});
