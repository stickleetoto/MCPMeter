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
