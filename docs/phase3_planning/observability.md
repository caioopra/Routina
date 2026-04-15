# Phase 3 Observability Plan

## Stack Choice: Axiom for Everything (MVP)

**Decision: Axiom as the single ingestion target for logs, metrics (events-as-metrics), and structured spans. OTel traces deferred to Phase 4.**

Rationale:

- Fly.io has a native log drain that ships stdout to Axiom with zero sidecar configuration.
- Axiom's free tier covers 500 GB ingest/month and 30-day retention — far beyond projected solo-dev scale.
- Axiom treats every JSON log line as a queryable event: you get ad-hoc filtering, aggregation, and saved dashboards without running a separate Prometheus + Grafana stack.
- Grafana Cloud Loki requires a separate Prometheus deployment for metrics (two moving parts). Honeycomb is excellent for traces but expensive and unnecessary before OTel is wired. Fly native logs are ephemeral (no history, no query).
- `axum-prometheus` → Grafana Cloud Mimir is rejected at MVP: 10k series limit sounds generous but cardinality risks from conversation IDs would burn it quickly, and it adds a scrape endpoint to secure.

**Per-signal breakdown:**

| Signal | Tool | Rationale |
|---|---|---|
| Logs | Axiom (via Fly log drain) | Zero infra, queryable |
| Metrics | Axiom (events-as-metrics APL queries) | No second system |
| Traces | stdout only (Phase 4: OTel → Axiom/Honeycomb) | Deferred — not blocking |

---

## What Goes Where

### Structured tracing spans (`chat.turn`, `chat.round`, `chat.tool_call`)

These already carry the right fields (`provider`, `model`, `tool_name`, `round_index`, `turn_id`). They are **traces today, logs in MVP**.

Action: switch `tracing-subscriber` to JSON output (`LOG_FORMAT=json` env var gates it). Each span emits a structured JSON line on close. Axiom indexes every field. No OTel exporter needed until Phase 4.

Mapping: `chat.turn` → log with `duration_ms`, `provider`, `model`, `status`. `chat.round` → log with `round_index`, `tool_calls_count`. `chat.tool_call` → log with `tool_name`, `duration_ms`, `success`.

### Route-level request counts / durations / statuses

Add `tower-http`'s built-in `TraceLayer` (already a dep) with structured JSON fields. Each request becomes a log line with `method`, `path`, `status`, `latency_ms`. Axiom aggregates these into request-rate and p95 latency panels via APL.

Do **not** add `axum-prometheus` at MVP. It adds a `/metrics` scrape endpoint to secure and a second aggregation system to maintain.

### LLM cost/usage

Covered by the ai-governance brief (DB rollup in `messages` table). The DB is the source of truth. Axiom gets a log event per `chat.turn` with `input_tokens` and `output_tokens` fields emitted from `ProviderEvent::Done`. Cost panels query Axiom directly; no separate pipeline needed.

### User actions / audit events

Covered by the security brief. Out of scope here.

---

## Required Rust Dependencies

**Add:**

```toml
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }  # already present — enable json feature
```

The `json` feature is the only required addition. `tracing-subscriber` is already a dep; enabling the `json` feature unlocks `.json()` formatting.

**Do not add at MVP:**
- `tracing-opentelemetry` — deferred to Phase 4
- `opentelemetry_sdk` / `opentelemetry-otlp` — deferred to Phase 4
- `axum-prometheus` — rejected (see above)
- `metrics` / `metrics-exporter-prometheus` — rejected
- `tracing-loki` — rejected (Loki is not the chosen aggregator)

Phase 4 additions (when OTel is ready): `tracing-opentelemetry = "0.27"`, `opentelemetry_sdk = "0.27"`, `opentelemetry-otlp = { version = "0.27", features = ["grpc-tonic"] }`. Axiom accepts OTLP/gRPC natively on port 4317.

---

## Implementation: JSON Logging Switch

In `main.rs`, gate on `LOG_FORMAT`:

```rust
match std::env::var("LOG_FORMAT").as_deref() {
    Ok("json") => {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }
    _ => {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }
}
```

Set `LOG_FORMAT=json` in fly.toml `[env]` block. Local dev keeps text output.

---

## MVP Dashboard (8 Panels)

All panels use Axiom APL queries against the ingested log stream. Refresh: 1-minute auto-refresh.

**Panel 1 — Active chat turns (right now)**
- Source: Axiom, `chat.turn` span events where `status = "open"` or last-seen within 60s.
- Query shape: count of `turn_id` with no close event in last 60s.
- Cadence: 30s refresh.

**Panel 2 — p95 chat turn latency by provider (last 1h)**
- Source: Axiom, `chat.turn` closed events, `duration_ms` field.
- Query shape: `percentile(duration_ms, 95)` grouped by `provider`.
- Cadence: 1m.

**Panel 3 — Token spend today (input + output by provider)**
- Source: Axiom, `chat.turn` events with `input_tokens`, `output_tokens`.
- Query shape: `sum(input_tokens + output_tokens)` grouped by `provider`, filtered to today.
- Cadence: 5m.

**Panel 4 — Tool-call failure rate (last 24h)**
- Source: Axiom, `chat.tool_call` events, `success` field.
- Query shape: `count where success=false / count total`, grouped by `tool_name`.
- Cadence: 5m.

**Panel 5 — HTTP error rate per route (last 1h)**
- Source: Axiom, request trace events, `status` and `path` fields.
- Query shape: `count where status >= 400` grouped by `path`, as % of total per path.
- Cadence: 1m.

**Panel 6 — LLM provider error rate (last 24h)**
- Source: Axiom, `chat.round` events where `status = "error"` grouped by `provider`.
- Cadence: 5m.

**Panel 7 — Chat turns per hour (last 7 days)**
- Source: Axiom, `chat.turn` close events, time-bucketed.
- Query shape: `count` grouped by `bin(1h)`.
- Cadence: 15m.

**Panel 8 — Tool-call breakdown (last 24h)**
- Source: Axiom, `chat.tool_call` events.
- Query shape: `count` grouped by `tool_name`, sorted descending.
- Cadence: 5m.

---

## Fly.io Deployment Specifics

**Log drain setup (one-time CLI command):**

```bash
fly logs drain create \
  --type axiom \
  --url "https://api.axiom.co/v1/datasets/planner-prod/ingest" \
  --headers "Authorization=Bearer ${AXIOM_TOKEN},X-Axiom-Dataset=planner-prod"
```

Fly ships every stdout/stderr line from the app machine to Axiom. No sidecar, no egress config beyond opening the drain. Axiom's ingest endpoint is HTTPS port 443 — no firewall changes needed on fly.io.

**Metrics endpoint:** Do not expose `/metrics` at MVP (no Prometheus). If added in Phase 4, protect it with a shared secret check (`Authorization: Bearer <METRICS_SECRET>`) rather than IP allowlisting — Fly's anycast IPs make scraper IP allowlisting fragile.

**OTel exporter (Phase 4):** Use OTLP/gRPC (`opentelemetry-otlp` with `grpc-tonic` feature) targeting Axiom's OTLP endpoint `https://api.axiom.co:4317`. Set via `OTEL_EXPORTER_OTLP_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS`. gRPC preferred over HTTP for lower per-span overhead.

---

## New Secrets

Set with `fly secrets set KEY=value`:

| Secret | Description |
|---|---|
| `AXIOM_TOKEN` | Axiom API token with ingest+query permissions |
| `LOG_FORMAT` | Set to `json` in production (not secret, but set in `fly.toml [env]`) |

Phase 4 additions (OTel):

| Secret | Description |
|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `https://api.axiom.co:4317` |
| `OTEL_EXPORTER_OTLP_HEADERS` | `Authorization=Bearer <token>,X-Axiom-Dataset=planner-traces` |

`LOG_FORMAT=json` goes in `fly.toml [env]` (non-sensitive). All tokens go in `fly secrets`.

Add to `.env.example`:

```
LOG_FORMAT=json          # "json" for production, omit for local text output
AXIOM_TOKEN=             # Axiom API token (production only)
```

---

## Cardinality Risks

**Safe labels (low cardinality, bounded):**

- `provider` — 2 values: `gemini`, `claude`
- `model` — ~4 values; grows slowly
- `tool_name` — ~10 values; bounded by tool registry
- `status` — `ok`, `error`, `timeout`
- `method` — `GET`, `POST`, `PATCH`, `DELETE`
- `path` (route template, not raw path) — ~15 routes; use `matched_path` from Axum, never the raw URI

**Dangerous labels (high cardinality, never use as metric dimensions):**

- `user_id` — unbounded at multi-user scale; keep in log fields only, not metric labels
- `conversation_id` / `turn_id` — unbounded; log fields only
- `request_id` — unbounded; log fields only

**Rule of thumb:** A label is safe if its cardinality is bounded at deploy time (enum-like). Any ID that grows with users or time is log-only.

For Axiom this matters less than Prometheus (no series limit), but grouping dashboards by `user_id` on a 100-user dataset will still produce unreadable charts. Reserve per-user breakdowns for ad-hoc queries, not saved panels.

---

## Cost Projection

**Axiom free tier:** 500 GB ingest/month, 30-day retention, unlimited queries.

At 1k chat turns/day:
- Each turn generates ~5 log lines (turn open/close, 1-2 rounds, 1-2 tool calls).
- Each JSON log line ~500 bytes.
- Daily log volume: 1,000 turns × 5 lines × 500 bytes = ~2.5 MB/day.
- Monthly: ~75 MB — 0.015% of the 500 GB free tier.

At 100 active users with 10 turns/day each = 1k turns/day: same number.

At 100M tokens/month: this is a token-spend metric, not a log-volume driver. Axiom stores the counts as fields on existing log lines — no additional volume.

**Break-even:** Axiom's free tier fits comfortably until ~50k turns/day (roughly 5,000 active users at 10 turns/day each). At current trajectory, $0/month for at least 12 months.

**LLM API costs** (separate from observability infra): 100M tokens/month on Gemini 2.5 Flash at ~$0.15/1M input + $0.60/1M output (blended ~$0.30/1M) ≈ $30/month. This is the dominant cost, not the observability stack.

**Projected total observability cost at scale: $0/month.**

---

## Sampling

**Traces (stdout, MVP):** No sampling needed. Spans write to stdout and are shipped as log lines. Volume is proportional to turns (2.5 MB/day at 1k turns — trivial).

**When to start sampling:** If/when OTel tracing is added in Phase 4, begin with head-based sampling at 100% and reduce to 10% only when trace ingest exceeds 1 GB/day — approximately 400k turns/day. That is not a concern for this app at any realistic near-term scale.

**Logs:** Never sample. Log volume is tiny. Sampling logs loses the error events you need most. If volume somehow spikes, add a `RUST_LOG` filter to drop `INFO` and keep `WARN`/`ERROR` before sampling.

---

## Summary

**Top 3 decisions:**

1. **Axiom for all signals (logs + events-as-metrics), OTel traces deferred to Phase 4.** Single log drain from Fly stdout to Axiom covers everything. No Prometheus, no Grafana, no sidecar. One new secret (`AXIOM_TOKEN`). Zero infra to maintain.

2. **8-panel MVP dashboard in Axiom saved views**, querying the JSON-formatted span logs that already exist in Phase 2 (`chat.turn`, `chat.round`, `chat.tool_call`). The only code change is enabling `tracing-subscriber`'s `json` feature and switching the formatter behind `LOG_FORMAT=json`.

3. **$0/month projected observability cost at 100 users / 1k turns/day for 12+ months.** Axiom free tier absorbs the entire log volume. LLM API cost ($30/month at 100M tokens) is the real budget line, not observability infrastructure.
