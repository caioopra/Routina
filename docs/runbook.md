# Operational Runbook — AI-Guided Planner

## Structured Logging

### Log Format

The backend supports two log output formats controlled by the `LOG_FORMAT` environment variable.

- **Local development:** omit the variable or set `LOG_FORMAT=text` for human-readable output.
- **Production:** set `LOG_FORMAT=json` to emit newline-delimited JSON, which Axiom (and most log aggregators) can parse without additional configuration.

> Note: the value is case-sensitive. `JSON` will not activate JSON mode — use lowercase `json`.

### Log Level

Log verbosity is controlled via the standard `RUST_LOG` env var using the `tracing` directive syntax:

```
RUST_LOG=info,planner_backend=debug
```

- `info` — application-wide minimum level (HTTP requests, startup events)
- `planner_backend=debug` — enables debug-level spans for the backend crate, which is required to capture `chat.round` span fields

For production, `info` is a safe default. Add `planner_backend=debug` if you need per-round token counts in Axiom.

### Setting Variables on Fly.io (no fly.toml present)

Because there is no `fly.toml` in this repository, set log configuration as Fly.io secrets/env at deploy time:

```bash
fly secrets set LOG_FORMAT=json
fly secrets set RUST_LOG="info,planner_backend=debug"
```

---

## Axiom Log Drain (Fly.io)

Fly.io streams app stdout to ephemeral logs that are lost after an instance restarts. Configuring an Axiom drain persists all structured JSON logs for querying.

### One-Time Setup

```bash
fly secrets set AXIOM_TOKEN=<your-axiom-api-token>

fly logs drain create axiom \
  --token <your-axiom-api-token> \
  --dataset planner-prod
```

Replace `<your-axiom-api-token>` with an Axiom API token that has ingest permission on the `planner-prod` dataset.

### Verify

After the next `fly deploy`, open Axiom and query the `planner-prod` dataset. Events should appear within 2 minutes of the first HTTP request hitting the app. If the dataset is empty after 5 minutes, see the troubleshooting section below.

---

## Key Structured Log Fields

These fields are emitted as JSON keys when `LOG_FORMAT=json` is active. Use them in Axiom APL queries.

### `chat.turn` span

Emitted once per top-level chat request.

| Field | Type | Description |
|---|---|---|
| `user_id` | string (UUID) | Authenticated user |
| `conversation_id` | string (UUID) | Conversation context |
| `routine_id` | string (UUID) | Routine being edited |
| `provider` | string | LLM provider: `gemini` or `claude` |
| `model` | string | Exact model identifier |
| `input_tokens` | integer | Total input tokens across all rounds |
| `output_tokens` | integer | Total output tokens across all rounds |
| `estimated_cost_usd` | float | Estimated cost in USD |

### `chat.round` span

Emitted once per agentic loop iteration (each LLM call).

| Field | Type | Description |
|---|---|---|
| `round` | integer | Loop iteration index (1-based) |
| `finish_reason` | string | LLM stop reason: `Stop`, `ToolUse`, `Error`, etc. |
| `input_tokens` | integer | Input tokens for this round |
| `output_tokens` | integer | Output tokens for this round |

> Requires `RUST_LOG` to include the span target at DEBUG level (e.g., `planner_backend=debug`).

### `chat.tool_call` span

Emitted once per tool invocation within a round.

| Field | Type | Description |
|---|---|---|
| `tool_name` | string | Name of the tool called |
| `tool_call_id` | string | Unique call ID from the LLM |
| `duration_ms` | integer | Execution time in milliseconds |
| `success` | boolean | Whether the tool returned without error |

---

## Suggested Axiom Dashboard Panels

The following APL queries form a starting-point dashboard. Field names assume the JSON structure emitted by `tracing-subscriber` with the `JsonLayer`. Adjust field paths after reviewing the first batch of real events in Axiom.

### 1. Active Turns per Hour

```apl
planner-prod
| where name == "chat.turn"
| summarize count() by bin(timestamp, 1h)
```

### 2. P95 Latency by Route

```apl
planner-prod
| where name == "HTTP request"
| summarize percentile(duration_ms, 95) by http.route
```

### 3. Token Spend per Day

```apl
planner-prod
| where name == "chat.turn"
| summarize sum(estimated_cost_usd) by bin(timestamp, 1d)
```

### 4. Tool-Call Failure Rate

```apl
planner-prod
| where name == "chat.tool_call"
| summarize countif(success == false) / count() by tool_name
```

### 5. HTTP Error Rate per Hour

```apl
planner-prod
| where name == "HTTP request" and http.status_code >= 400
| summarize count() by bin(timestamp, 1h)
```

### 6. Provider Error Rate per Hour

```apl
planner-prod
| where name == "chat.round" and finish_reason == "Error"
| summarize count() by bin(timestamp, 1h)
```

### 7. Turns per Hour by Provider

```apl
planner-prod
| where name == "chat.turn"
| summarize count() by provider, bin(timestamp, 1h)
```

### 8. Tool-Call Volume Breakdown

```apl
planner-prod
| where name == "chat.tool_call"
| summarize count() by tool_name
```

---

## Troubleshooting

### Logs not appearing in Axiom

1. Confirm the drain is registered:
   ```bash
   fly logs drain list
   ```
   The output should include an entry with type `axiom`.

2. Confirm `AXIOM_TOKEN` is set:
   ```bash
   fly secrets list
   ```

3. Confirm the token has ingest permission on the `planner-prod` dataset in the Axiom UI.

4. Redeploy to pick up any secret changes:
   ```bash
   fly deploy
   ```

### JSON parsing errors in Axiom

Ensure `LOG_FORMAT=json` is set (lowercase). Verify with:

```bash
fly ssh console -C "printenv LOG_FORMAT"
```

Expected output: `json`

### Missing span fields (e.g., `chat.round` fields absent)

`chat.round` is a child span recorded at DEBUG level. Ensure `RUST_LOG` includes the crate target at debug:

```bash
fly secrets set RUST_LOG="info,planner_backend=debug"
fly deploy
```

After redeployment, trigger a chat request and check Axiom for events with `name == "chat.round"`.
