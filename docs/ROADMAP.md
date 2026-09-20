# MCPMeter Roadmap

## v0.1 — Measurement Core

Goal: produce trustworthy measurements for a stdio MCP server with minimal behavioral interference.

### P0

- [x] Transparent stdio proxy
- [x] JSON-RPC newline framing / byte-preserving forwarding
- [x] `tools/list` observation
- [x] `tools/call` observation
- [x] exact byte accounting
- [x] request/response boundary latency measurement
- [x] tokenizer abstraction
- [x] schema-token and serialized request/response payload token metrics
- [x] append-friendly JSONL run trace
- [x] CLI summary report
- [x] safe-by-default traces with raw payload persistence disabled by default
- [x] deterministic fixture MCP server for tests

### P1

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

### Reporting

- [x] baseline vs candidate run comparison (landed early in v0.1)
- [x] per-tool cost report (landed early in v0.1)
- [x] HTML report
- [x] CSV export
- [ ] schema-cost comparison across MCP servers
- [ ] configurable redaction rules
- [ ] optional trace rotation / size limits

### Streamable HTTP

Target the current MCP `2026-07-28` stateless transport model first. Legacy 2025-era traffic may be relayed transparently where practical, but new measurement semantics must not depend on session state.

- [x] define HTTP byte/timing measurement contract
- [ ] define backward-compatible trace schema for transport-specific metrics
- [ ] add HTTP reverse-proxy CLI/config
- [ ] preserve method, path/query, status, MCP headers, auth headers, and content type while forwarding
- [ ] observe direct JSON request/response payloads
- [ ] forward and observe SSE incrementally without full-stream buffering
- [ ] classify JSON-RPC messages carried in SSE `data:` events
- [ ] record safe modern routing metadata such as `Mcp-Method` / `Mcp-Name`
- [ ] deterministic local HTTP fixture
- [ ] Linux and Windows HTTP regression coverage
- [ ] document legacy 2025-era compatibility

See [`HTTP_MEASUREMENT.md`](HTTP_MEASUREMENT.md).

## v0.3 — Agent Benchmarking

- [ ] provider-reported usage adapters where available
- [ ] model-context estimate adapters
- [ ] A/B task runner
- [ ] task success/test-result hooks
- [ ] MCP efficiency metrics
- [ ] batch/tool-selection analysis

## Non-goals for early versions

- MCP orchestration
- replacing MCP clients
- claiming exact model-visible token usage without provider evidence
- cloud collection as a requirement
- storing raw private workloads by default
- TLS interception / MITM
