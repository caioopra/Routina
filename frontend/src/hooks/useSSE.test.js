import { renderHook, act, waitFor } from "@testing-library/react";
import { describe, it, expect, beforeEach, vi } from "vitest";
import { useSSE } from "./useSSE";
import { useAuthStore } from "../stores/authStore";

// Helper: create a ReadableStream that emits the given SSE string
function makeSSEStream(sseText) {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(sseText));
      controller.close();
    },
  });
}

function mockFetchSSE(sseText, status = 200) {
  global.fetch = vi.fn().mockResolvedValueOnce({
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? "OK" : "Error",
    body: makeSSEStream(sseText),
    text: async () => "Error body",
  });
}

describe("useSSE", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAuthStore.setState({
      token: "token-test",
      user: null,
      refreshToken: null,
    });
  });

  it("starts with idle status", () => {
    const { result } = renderHook(() => useSSE("/api/chat/message"));
    expect(result.current.status).toBe("idle");
    expect(result.current.tokens).toEqual([]);
    expect(result.current.error).toBeNull();
  });

  it("accumulates tokens and reaches done status", async () => {
    const sseText = [
      'event: token\ndata: "Hello "\n',
      "\n",
      'event: token\ndata: "world"\n',
      "\n",
      "event: done\ndata: {}\n",
      "\n",
    ].join("");

    mockFetchSSE(sseText);

    const { result } = renderHook(() => useSSE("/api/chat/message"));

    await act(async () => {
      result.current.start({ message: "test" });
    });

    await waitFor(() => {
      expect(result.current.status).toBe("done");
    });

    expect(result.current.tokens).toContain("Hello ");
    expect(result.current.tokens).toContain("world");
  });

  it("calls onEvent callback for each event", async () => {
    const sseText = [
      'event: provider\ndata: {"name":"gemini"}\n',
      "\n",
      'event: token\ndata: "Hi"\n',
      "\n",
      "event: done\ndata: {}\n",
      "\n",
    ].join("");

    mockFetchSSE(sseText);

    const events = [];
    const { result } = renderHook(() => useSSE("/api/chat/message"));

    await act(async () => {
      result.current.start(
        { message: "test" },
        {
          onEvent: (type, data) => events.push({ type, data }),
        },
      );
    });

    await waitFor(() => expect(result.current.status).toBe("done"));

    expect(events.some((e) => e.type === "provider")).toBe(true);
    expect(events.some((e) => e.type === "token")).toBe(true);
    expect(events.some((e) => e.type === "done")).toBe(true);
  });

  it("calls onDone callback when stream ends", async () => {
    const sseText = "event: done\ndata: {}\n\n";
    mockFetchSSE(sseText);

    let doneCalled = false;
    const { result } = renderHook(() => useSSE("/api/chat/message"));

    await act(async () => {
      result.current.start(
        { message: "test" },
        {
          onDone: () => {
            doneCalled = true;
          },
        },
      );
    });

    await waitFor(() => expect(result.current.status).toBe("done"));
    expect(doneCalled).toBe(true);
  });

  it("sets error status on HTTP error", async () => {
    global.fetch = vi.fn().mockResolvedValueOnce({
      ok: false,
      status: 500,
      statusText: "Internal Server Error",
      text: async () => "Server error",
    });

    const { result } = renderHook(() => useSSE("/api/chat/message"));

    await act(async () => {
      result.current.start({ message: "test" });
    });

    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.error).toMatch(/500/);
  });

  it("includes Authorization header from auth store", async () => {
    const sseText = "event: done\ndata: {}\n\n";
    mockFetchSSE(sseText);

    const { result } = renderHook(() => useSSE("/api/chat/message"));

    await act(async () => {
      result.current.start({ message: "test" });
    });

    await waitFor(() => expect(result.current.status).toBe("done"));

    const fetchCall = global.fetch.mock.calls[0];
    expect(fetchCall[1].headers["Authorization"]).toBe("Bearer token-test");
  });

  it("cancel() aborts in-flight request", async () => {
    // Create a stream that never closes (infinite)
    const encoder = new TextEncoder();
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(encoder.encode('event: token\ndata: "chunk"\n\n'));
        // Don't close — simulates a long stream
      },
    });

    global.fetch = vi.fn().mockResolvedValueOnce({
      ok: true,
      status: 200,
      body: stream,
    });

    const { result } = renderHook(() => useSSE("/api/chat/message"));

    act(() => {
      result.current.start({ message: "test" });
    });

    // Cancel immediately
    act(() => {
      result.current.cancel();
    });

    // Should revert to idle (not error) when cancelled
    await waitFor(() => {
      expect(result.current.status).toBe("idle");
    });
  });

  it("handles event: error in stream", async () => {
    const sseText =
      'event: error\ndata: {"message":"Provider unavailable"}\n\n';
    mockFetchSSE(sseText);

    const { result } = renderHook(() => useSSE("/api/chat/message"));

    await act(async () => {
      result.current.start({ message: "test" });
    });

    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.error).toContain("Provider unavailable");
  });

  // ── cancel/done race (Fix 1) ───────────────────────────────────────────────

  it("cancel() before done fires: onDone is NOT called", async () => {
    // Build a stream that emits a token chunk and then a done event as two
    // separate enqueue calls so we can interleave cancel() between them.
    const encoder = new TextEncoder();
    let resolveSecondChunk;
    const secondChunkReady = new Promise((res) => {
      resolveSecondChunk = res;
    });

    const stream = new ReadableStream({
      async start(controller) {
        controller.enqueue(encoder.encode('event: token\ndata: "hello"\n\n'));
        // Wait until the test calls resolveSecondChunk (after cancel)
        await secondChunkReady;
        controller.enqueue(encoder.encode("event: done\ndata: {}\n\n"));
        controller.close();
      },
    });

    global.fetch = vi.fn().mockResolvedValueOnce({
      ok: true,
      status: 200,
      body: stream,
    });

    const onDone = vi.fn();
    const { result } = renderHook(() => useSSE("/api/chat/message"));

    act(() => {
      result.current.start({ message: "test" }, { onDone });
    });

    // Let the first token chunk be processed
    await waitFor(() => expect(result.current.tokens).toHaveLength(1));

    // Cancel before the done chunk arrives
    act(() => {
      result.current.cancel();
    });

    // Now release the done chunk
    resolveSecondChunk();

    // Give microtasks a moment to settle
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });

    // onDone must NOT have been called
    expect(onDone).not.toHaveBeenCalled();
    // Status must be idle (set by cancel), not done
    expect(result.current.status).toBe("idle");
  });
});
