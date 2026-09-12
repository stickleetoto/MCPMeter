# MCPMeter Roadmap

## v0.1 — Measurement Core

Goal: produce trustworthy measurements for a stdio MCP server with minimal behavioral interference.

### P0

- [ ] Transparent stdio proxy
- [ ] JSON-RPC message framing / forwarding
- [ ] `tools/list` observation
- [ ] `tools/call` observation
- [ ] exact byte accounting
- [ ] request/response latency measurement
- [ ] tokenizer abstraction
- [ ] `schema_tokens`, `request_tokens`, `response_tokens`
- [ ] JSONL run trace
- [ ] CLI summary report
- [ ] safe-by-default redaction / no raw payload persistence
- [ ] deterministic fixture MCP server for tests

### P1

- [ ] p50 / p95 / p99 latency report
- [ ] per-tool cost breakdown
- [ ] run metadata and reproducible run IDs
- [ ] tokenizer/profile metadata in every token metric
- [ ] machine-readable JSON report
- [ ] regression fixtures for malformed/error responses

## v0.2 — Transport and Comparison

- [ ] Streamable HTTP transport support
- [ ] baseline vs MCP-enabled run comparison
- [ ] schema-cost comparison across MCP servers
- [ ] HTML report
- [ ] CSV export
- [ ] configurable redaction rules

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
