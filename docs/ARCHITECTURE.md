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

The proxy forwards protocol traffic and emits measurement events. Measurement and persistence are deliberately separated from forwarding so reporting failures should not corrupt the MCP session.

## Core components

### `proxy`

Owns transport lifecycle and transparent forwarding. v0.1 targets stdio first.

### `observer`

Classifies MCP messages and extracts measurement-safe metadata such as method, tool name, sizes, timestamps, and success/error state.

### `tokenizer`

Provides named tokenizer/profile implementations. Every token metric must carry enough metadata to identify how it was produced.

### `metrics`

Aggregates exact byte counts, serialized token counts, call counts, errors, and latency distributions.

### `redaction`

Controls what, if anything, may be persisted from message content. Raw payload persistence is disabled by default.

### `storage`

Writes append-friendly JSONL events so interrupted runs remain inspectable.

### `report`

Produces human-readable CLI summaries and later machine-readable/HTML comparisons.

## Measurement contract

MCPMeter must never collapse these concepts into one number:

1. **wire bytes** — what crossed the observed MCP boundary;
2. **serialized payload tokens** — tokenizer output for observed MCP structures;
3. **model-context estimate** — an estimate after host transformation assumptions;
4. **provider usage** — actual usage reported by a model provider, if independently available.

An estimate must remain visibly labeled as an estimate throughout storage and reporting.

## v0.1 event sketch

```json
{
  "run_id": "example-run",
  "ts_ns": 0,
  "direction": "client_to_server",
  "kind": "tools_call",
  "tool": "yk.compute",
  "wire_bytes": 512,
  "serialized_tokens": 133,
  "tokenizer": "example-profile",
  "latency_ms": 2.4,
  "ok": true
}
```

The exact schema is not frozen yet.

## Reliability rules

- Forwarding takes precedence over reporting.
- A tokenizer/reporting failure should be observable but should not silently alter the proxied MCP message.
- Clock source and units must be explicit.
- Percentiles should be calculated from recorded samples using a documented method.
- Tests should include a deterministic fixture server and known message transcripts.
