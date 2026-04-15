import { useRef, useState, useCallback } from "react";
import { useAuthStore } from "../stores/authStore";

/**
 * useSSE — opens a POST-based SSE stream via fetch, parses event:/data: lines,
 * and exposes tokens + status control.
 *
 * @param {string} url  - The endpoint to POST to (e.g. "/api/chat/message")
 * @returns {{
 *   tokens: string[],
 *   status: 'idle'|'streaming'|'done'|'error',
 *   error: string|null,
 *   start: (body: object, callbacks?: { onEvent?: Function, onDone?: Function }) => void,
 *   cancel: () => void,
 * }}
 */
export function useSSE(url) {
  const [tokens, setTokens] = useState([]);
  const [status, setStatus] = useState("idle");
  const [error, setError] = useState(null);
  const abortRef = useRef(null);
  // cancelledRef is set by cancel() and checked inside the reading loop so that
  // onEvent/onDone callbacks are never fired after the user cancels, even if a
  // "done" event lands in the same microtask tick.
  const cancelledRef = useRef(false);

  const cancel = useCallback(() => {
    cancelledRef.current = true;
    if (abortRef.current) {
      abortRef.current.abort();
      abortRef.current = null;
    }
    setStatus("idle");
  }, []);

  /**
   * start — initiate the SSE stream.
   *
   * @param {object} body       - JSON body for the POST request.
   * @param {object} [callbacks]
   * @param {Function} [callbacks.onEvent]  - Called for every parsed SSE event: (eventType, data) => void
   * @param {Function} [callbacks.onDone]   - Called when the stream ends with a 'done' event.
   */
  const start = useCallback(
    async (body, callbacks = {}) => {
      const { onEvent, onDone } = callbacks;

      // Reset the cancelled flag for this new stream
      cancelledRef.current = false;

      // Cancel any in-flight stream
      if (abortRef.current) {
        abortRef.current.abort();
      }
      const controller = new AbortController();
      abortRef.current = controller;

      setTokens([]);
      setError(null);
      setStatus("streaming");

      const token = useAuthStore.getState().token;
      const headers = {
        "Content-Type": "application/json",
        Accept: "text/event-stream",
      };
      if (token) {
        headers["Authorization"] = `Bearer ${token}`;
      }

      try {
        const response = await fetch(url, {
          method: "POST",
          headers,
          body: JSON.stringify(body),
          signal: controller.signal,
        });

        if (!response.ok) {
          const text = await response.text().catch(() => "");
          throw new Error(
            `HTTP ${response.status}: ${text || response.statusText}`,
          );
        }

        if (!response.body) {
          throw new Error("Response body is null");
        }

        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });

          // SSE blocks are separated by double newlines
          const blocks = buffer.split(/\n\n/);
          // Keep the last (potentially incomplete) chunk in the buffer
          buffer = blocks.pop() ?? "";

          for (const block of blocks) {
            if (!block.trim()) continue;

            let eventType = "message";
            let dataLine = "";

            for (const line of block.split("\n")) {
              if (line.startsWith("event:")) {
                eventType = line.slice(6).trim();
              } else if (line.startsWith("data:")) {
                dataLine = line.slice(5).trim();
              }
            }

            if (!dataLine) continue;

            // Parse data as JSON when possible
            let parsed;
            try {
              parsed = JSON.parse(dataLine);
            } catch {
              parsed = dataLine;
            }

            // Skip callbacks if the stream was cancelled between events
            if (cancelledRef.current) return;

            if (onEvent) {
              onEvent(eventType, parsed);
            }

            if (eventType === "token") {
              const text =
                typeof parsed === "string" ? parsed : (parsed?.data ?? "");
              setTokens((prev) => [...prev, text]);
            } else if (eventType === "done") {
              setStatus("done");
              if (onDone) onDone(parsed);
              return;
            } else if (eventType === "error") {
              const msg =
                typeof parsed === "string"
                  ? parsed
                  : (parsed?.message ?? "Stream error");
              setError(msg);
              setStatus("error");
              return;
            }
          }
        }

        // Stream ended without explicit done event
        if (cancelledRef.current) return;
        setStatus("done");
        if (onDone) onDone(null);
      } catch (err) {
        if (err.name === "AbortError") {
          // Cancelled by the user — do not update error state
          return;
        }
        setError(err.message ?? "SSE connection failed");
        setStatus("error");
      }
    },
    [url],
  );

  return { tokens, status, error, start, cancel };
}

export default useSSE;
