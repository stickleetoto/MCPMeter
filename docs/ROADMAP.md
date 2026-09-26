# MCPMeter Roadmap

MCPMeter measures the real cost and behavior of MCP transports without pretending that observed transport payloads are identical to model-visible or billable tokens.

## v0.1 — Measurement Core ✅

Goal: produce trustworthy measurements for a stdio MCP server with minimal behavioral interference.

### Core proxy and observation

- [x] transparent stdio proxy
- [x] JSON-RPC newline framing / byte-preserving forwarding
- [x] `tools/list` observation
- [x] `tools/call` observation
- [x] exact stdio byte accounting
- [x] request/response boundary latency measurement
- [x] tokenizer abstraction
- [x] schema-token and serialized request/response payload token metrics
- [x] append-friendly JSONL run trace
- [x] safe-by-default traces with raw payload persistence disabled by default
- [x] deterministic fixture MCP server

### Reporting and regression baseline

- [x] CLI summary report
- [x] p50 / p95 / p99 / max latency report
- [x] per-tool cost breakdown with explicit batch-attribution semantics
- [x] run metadata and stable run selection
- [x] `runs` command for append-only trace navigation
- [x] tokenizer/profile metadata in every event
- [x] machine-readable JSON reports
- [x] regression fixtures for malformed/error responses
- [x] baseline/candidate comparison for same-tokenizer runs
- [x] Linux and Windows CI

## v0.2 — Transport and Reporting

Goal: extend the trustworthy measurement contract beyond stdio while improving comparison and export workflows.

### Reporting

Already landed:

- [x] baseline vs candidate run comparison
- [x] per-tool cost report
- [x] HTML report
- [x] CSV export
- [x] cumulative serialized-token ledger

Remaining:

- [ ] schema-cost comparison across MCP servers
  - MCPM-201 extends `compare` with schema-token deltas and schema tokens per exposed tool when both runs contain compatible `tools/list` measurements. Token-derived comparisons require the same tokenizer and the same exact/estimated tokenizer profile; mixed-profile runs and incompatible pairs fail explicitly instead of being normalized.
- [ ] configurable redaction rules
- [ ] optional trace rotation / size limits

### Streamable HTTP

Target the current MCP `2026-07-28` stateless transport model first. Legacy 2025-era traffic may be relayed transparently where practical, but new measurement semantics must not depend on session state.

Landed:

- [x] define HTTP byte/timing measurement contract
- [x] add backward-compatible transport discrimination to the trace schema
- [x] separate HTTP application payload bytes from exact stdio wire bytes
- [x] make wire-byte fields transport-aware / optional where exact wire measurement is unavailable
- [x] add dedicated Streamable HTTP reverse-proxy CLI/config
- [x] preserve request method, path/query, relevant MCP/auth headers, response status/headers, and content type while forwarding
- [x] observe direct JSON request/response payloads without mutating them
- [x] define latency semantics for direct JSON responses and long-lived streams
- [x] keep HTTP raw-payload capture explicit opt-in only
- [x] deterministic local HTTP fixture coverage
- [x] Linux and Windows HTTP regression coverage
- [x] record safe modern routing metadata such as `Mcp-Method` / `Mcp-Name` without persisting secrets
- [x] forward and observe SSE incrementally without buffering the full response
- [x] classify JSON-RPC messages carried in SSE `data:` events when possible
- [x] document legacy 2025-era Streamable HTTP behavior and limitations

See [`HTTP_MEASUREMENT.md`](HTTP_MEASUREMENT.md) and parent tracking issue #4.

### Recommended v0.2 implementation order

Keep each change bounded and independently reviewable:

Completed transport sequence:

1. safe `Mcp-Method` / `Mcp-Name` routing metadata
2. incremental SSE forwarding/observation
3. SSE `data:` JSON-RPC classification
4. legacy 2025-era compatibility documentation

Next reporting work:

1. schema-cost comparison across MCP servers
2. configurable redaction rules
3. optional trace rotation / size limits

Do not weaken these invariants while completing v0.2:

- exact stdio wire bytes remain distinct from HTTP application payload bytes
- observed serialized-token counts are not labeled provider/model-context usage
- auth credentials and other secrets are not persisted as routing metadata
- long-lived SSE responses are never buffered to completion before forwarding
- raw payload persistence remains opt-in

## v0.3 — Agent Benchmarking

Goal: connect transport measurements to controlled agent-level experiments without turning MCPMeter into an orchestration framework.

- [ ] provider-reported usage adapters where available
- [ ] model-context estimate adapters
- [ ] A/B task runner
- [ ] task success/test-result hooks
- [ ] MCP efficiency metrics
- [ ] batch/tool-selection analysis

## Later candidates

Only after the v0.2 transport contract is stable:

- richer cross-run trend/history views
- host-aware context-cost adapters
- export formats required by real benchmarking workflows
- additional MCP transports when their measurement semantics can remain explicit

## Non-goals for early versions

- MCP orchestration
- replacing MCP clients
- claiming exact model-visible token usage without provider evidence
- cloud collection as a requirement
- storing raw private workloads by default
- TLS interception / MITM
