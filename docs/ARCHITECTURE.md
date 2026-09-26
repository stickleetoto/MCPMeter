# MCPMeter Architecture

## Data path

```text
MCP Client / Agent
        |
        v
+------------------+
| MCPMeter Proxy   |
|------------------|
| transport        |
| observer         |
| tokenizer        |
| metrics          |
| redaction policy |
+------------------+
        |
        v
    MCP Server
```

The proxy forwards protocol traffic and emits measurement events. Measurement and persistence are deliberately separated from forwarding so reporting failures should not corrupt the MCP session or request stream.

## Core components

### `transport`

Owns lifecycle and transparent forwarding.

- v0.1: newline-delimited stdio frames.
- v0.2: Streamable HTTP reverse proxy, with current MCP `2026-07-28` behavior as the primary target.

Transport code is responsible for defining the exact observation boundary. It must not label an application-layer quantity as a lower-layer network quantity.

### `observer`

Classifies MCP messages and extracts measurement-safe metadata such as method, tool name, sizes, timestamps, and success/error state.

The observer should receive copied/teed data and remain off the forwarding-critical path. Long-lived HTTP/SSE responses must be inspected incrementally rather than buffered to completion.

### `tokenizer`

Provides named tokenizer/profile implementations. Every token metric must carry enough metadata to identify how it was produced.

### `metrics`

Aggregates observed byte counts, serialized token counts, call counts, errors, and latency distributions. Each metric must retain enough scope metadata to say what boundary it measures.

### `provider_usage`

Defines the versioned adapter boundary for provider-reported token usage. Adapter interface v1 accepts a provider response payload that is already available to the caller and may return normalized provider-reported `input_tokens`, `output_tokens`, and/or `total_tokens` together with adapter identity/version, provider identity, and optional model identity.

Provider usage adapters do not inspect MCP transport measurements as a fallback and must not manufacture provider usage from `serialized_tokens`, schema tokens, payload bytes, or model-context estimates. Missing provider counters stay missing; for example, an adapter must not synthesize `total_tokens` by adding other counters unless that total was itself reported by the provider.

The normalized record is explicitly labeled `provider_reported`. Adapter errors describe the failing field or contract condition without embedding the raw provider payload or field value.

### `context_estimate`

Defines the versioned adapter boundary for estimated model-context token counts. Adapter interface v1 accepts caller-supplied context input and returns a normalized `estimated_context_tokens` value together with adapter identity/version, an explicit estimate `basis`, and optional model identity.

The normalized record is always labeled `origin: estimated`. Context-estimate adapters remain separate from both provider-reported usage and observed MCP serialized-token measurements. The adapter wrapper does not reinterpret `serialized_tokens`, provider `input_tokens`/`output_tokens`/`total_tokens`, schema tokens, or payload bytes as model context.

A context estimate is not provider billing evidence. Adapter errors describe the failing field or contract condition without embedding caller-supplied context values.

### `redaction`

Controls what, if anything, may be persisted from message content. Raw payload persistence is disabled by default.

For HTTP, authorization, cookies, and `Mcp-Param-*` values are sensitive and must not be persisted by default. Safe routing metadata such as protocol version, method name, tool/resource name, content type, and status may be recorded when explicitly defined by the trace schema.

### `storage`

Writes append-friendly JSONL events so interrupted runs remain inspectable.

### `report`

Produces human-readable CLI summaries and machine-readable/HTML/CSV outputs. Reports must preserve the distinction between exact, derived, and estimated metrics.

## Measurement contract

MCPMeter must never collapse unlike quantities into one number.

1. **transport-boundary bytes** — bytes exactly observed at the boundary whose scope is named by the metric;
2. **serialized payload tokens** — tokenizer output for observed MCP JSON structures;
3. **model-context estimate** — an estimate after host transformation assumptions;
4. **provider usage** — actual usage reported by a model provider, if independently available.

Provider-reported usage enters through the versioned `provider_usage` adapter contract and remains separate from the MCP observation path. The v1 adapter contract carries its own interface version plus adapter identity/version so changes in provider payload mapping can be identified independently of the MCP trace schema.

Model-context estimates enter through the versioned `context_estimate` adapter contract. Every normalized result is explicitly labeled `estimated` and names its estimation basis. This estimate source is neither provider-reported usage nor an observed MCP serialized-token measurement, and it must not be presented as billable/provider usage.

For stdio, the current `wire_bytes` field is exact for the observed stdio frame, including its newline delimiter when present.

For Streamable HTTP, an MCP JSON body or SSE payload observed by an HTTP library is **not** automatically equal to network wire bytes. HTTP/1 framing, chunking, TLS records, HTTP/2 frames, compression, and lower-layer overhead may differ. HTTP support therefore needs explicitly scoped payload/header/transport metrics rather than silently reusing the stdio meaning of `wire_bytes`.

An estimate must remain visibly labeled as an estimate throughout storage and reporting.

See [`HTTP_MEASUREMENT.md`](HTTP_MEASUREMENT.md) for the v0.2 transport contract.

## Latency contract

A single latency label is only valid when its start and end boundaries are explicit.

### stdio

Current correlated latency measures the elapsed time between the observed request frame boundary and the correlated response frame boundary.

### Streamable HTTP

HTTP introduces multiple useful boundaries:

- request accepted by MCPMeter;
- upstream request dispatch;
- upstream response headers;
- first response body byte;
- complete JSON-RPC response body;
- individual JSON-RPC response event inside SSE;
- final stream close.

Direct JSON responses may expose both time-to-first-byte and request-to-message-complete timings. A long-lived `subscriptions/listen` stream must not be summarized as one ordinary request latency; per-event correlation and stream lifetime are separate metrics.

## Reliability rules

- Forwarding takes precedence over reporting.
- A tokenizer/reporting failure should be observable but should not silently alter proxied traffic.
- Clock source, units, and measurement boundaries must be explicit.
- Percentiles should be calculated from recorded samples using a documented method.
- Long-lived streams must not be buffered solely for measurement.
- Tests should include deterministic fixture servers and known message transcripts.
