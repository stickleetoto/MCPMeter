# MCPMeter

[![CI](https://github.com/stickleetoto/MCPMeter/actions/workflows/ci.yml/badge.svg)](https://github.com/stickleetoto/MCPMeter/actions/workflows/ci.yml)

**Measure the real cost of Model Context Protocol (MCP) servers.**

MCPMeter is a local-first measurement proxy for MCP. It answers a practical question:

> How much traffic, tokenized payload, latency, and tool overhead does an MCP server add — and is that cost worth it?

MCPMeter deliberately does **not** pretend that bytes observed on the MCP wire are identical to the final context seen by a model.

## Status

**v0.1 development build — the stdio measurement core is operational.**

Current features:

- transparent stdio proxy for newline-delimited MCP JSON-RPC
- asynchronous observation so tokenization, hashing, parsing, and trace writing stay off the primary forwarding path
- exact client→server / server→client byte accounting
- `o200k_base`, `cl100k_base`, and explicitly heuristic `bytes4_estimate` token profiles
- `tools/list` catalog/schema token measurement
- `tools/call` counting and tool-name attribution
- request/response correlation by JSON-RPC id
- boundary-to-boundary latency samples with p50/p95/p99/max reporting
- per-tool request/response token, wire-byte, error, and latency costs
- truthful batch accounting: shared batch payload cost is reported as unattributed instead of being guessed per tool
- JSON-RPC batch observation
- append-friendly, versioned JSONL traces
- SHA-256 payload fingerprints
- raw payload persistence **off by default**
- `report`, `tools`, `runs`, and baseline/candidate `compare` commands
- text and machine-readable JSON output
- deterministic built-in MCP fixture server
- unit and end-to-end CLI/proxy/error-path regression tests
- Linux and Windows CI quality gates

Streamable HTTP measurement, provider-reported usage adapters, deeper agent benchmarking, and richer exports are planned after the stdio core stabilizes.

## Install from source

```bash
cargo install --path .
```

During development:

```bash
cargo run -- --help
```

## Quick start

Put MCPMeter in front of any stdio MCP server:

```bash
mcp-meter proxy --trace mcpmeter.jsonl -- your-mcp-server arg1 arg2
```

Point the MCP host at **MCPMeter** instead of directly at the server. MCPMeter launches the real server as a child process and forwards stdin/stdout unchanged while measurement happens on a separate observer path.

Generate a report for the newest run:

```bash
mcp-meter report mcpmeter.jsonl
```

Show cost by individual tool:

```bash
mcp-meter tools mcpmeter.jsonl
```

List all runs stored in an append-only trace:

```bash
mcp-meter runs mcpmeter.jsonl
```

Select one run explicitly:

```bash
mcp-meter report mcpmeter.jsonl --run-id <RUN_ID>
mcp-meter tools mcpmeter.jsonl --run-id <RUN_ID>
```

Compare two traces:

```bash
mcp-meter compare baseline.jsonl candidate.jsonl
```

Or compare two runs stored in one trace:

```bash
mcp-meter compare mcpmeter.jsonl mcpmeter.jsonl \
  --baseline-run-id <BASELINE_RUN_ID> \
  --candidate-run-id <CANDIDATE_RUN_ID>
```

Every reporting command supports machine-readable JSON:

```bash
mcp-meter report mcpmeter.jsonl --json
mcp-meter tools mcpmeter.jsonl --json
mcp-meter runs mcpmeter.jsonl --json
mcp-meter compare baseline.jsonl candidate.jsonl --json
```

## Yekaterina A/B example

A practical Yekaterina experiment can use controlled agent runs with MCPMeter inserted in front of Yekaterina:

```text
Agent / Codex / GPT
        │
        ▼
     MCPMeter
        │
        ▼
    Yekaterina
```

Run the same task under controlled conditions, append each measured session to a trace, use `runs` to identify exact run ids, use `tools` to see which Yekaterina tools consumed the traffic, and use `compare` to calculate candidate-minus-baseline deltas for serialized MCP tokens, wire bytes, tool calls, errors, schema tokens, exposed tools, and p50/p95/p99 latency.

`compare` rejects token comparisons when baseline and candidate used different tokenizer profiles.

## Per-tool accounting

For a normal, non-batched `tools/call`, MCPMeter attributes the observed request and correlated response cost to that tool:

```text
Tool            Calls   Req tok   Resp tok   Total tok    p95 ms   Errors
---------------------------------------------------------------------------
yk.compute         14      1102       3481        4583      4.800        0
```

For a JSON-RPC batch containing multiple tools, the serialized batch frame has shared syntax and framing. MCPMeter therefore **does not invent a per-tool token split**. Tool call counts are retained, while the shared payload cost is surfaced separately as `unattributed_batch_request_tokens`, `unattributed_batch_response_tokens`, and matching wire-byte fields.

## Tokenizer profiles

```bash
mcp-meter proxy --tokenizer o200k-base -- your-server
mcp-meter proxy --tokenizer cl100k-base -- your-server
mcp-meter proxy --tokenizer bytes4-estimate -- your-server
```

- `o200k-base`: tokenization of the observed serialized payload under `o200k_base`.
- `cl100k-base`: tokenization of the observed serialized payload under `cl100k_base`.
- `bytes4-estimate`: explicit heuristic (`ceil(UTF-8 bytes / 4)`).

These numbers describe **observed MCP payloads under the selected tokenizer**. They are not automatically the same as billable/model-context tokens because an MCP host may transform, cache, truncate, deduplicate, or reserialize tool definitions and results before sending them to a model.

## Built-in fixture

MCPMeter includes a hidden deterministic fixture server for development and regression tests:

```bash
mcp-meter proxy --tokenizer bytes4-estimate -- mcp-meter fixture
```

It exposes `add`, `echo`, and `sleep_ms` tools and supports modern `server/discover` plus the legacy `initialize` flow for compatibility testing.

## Example report

```text
Run:                 1789190000000000000-1234
Tokenizer:           o200k_base
Messages:            31
Requests/responses:  15 / 15
Notifications:       1
Tool calls:          14
Unique tools called: 1
Tools:               yk.compute
Tools exposed:       3
Schema tokens:       412
Wire bytes C→S/S→C:  12140 / 19702
Wire bytes total:    31842
Tokens C→S/S→C:      1102 / 3481
Serialized tokens:   4583
Error events:        0
Latency p50:         2.100 ms
Latency p95:         4.800 ms
Latency p99:         4.800 ms
Latency max:         5.200 ms
```

## Measurement contract

MCPMeter keeps these concepts separate:

1. **Wire metrics** — exact bytes and message counts observed at the MCP boundary.
2. **Serialized token metrics** — token count of the exact observed JSON text, excluding the newline transport delimiter, under a named tokenizer.
3. **Schema/catalog metrics** — token count of a canonical compact serialization of the `tools` array returned by `tools/list`.
4. **Model-context estimates** — future host-aware estimates, always labeled as estimates.
5. **Provider usage** — future provider-reported usage, only labeled actual when independently supplied by the provider or host.

See [`docs/TRACE_FORMAT.md`](docs/TRACE_FORMAT.md) for the event schema and exact/derived metric boundary.

## Why observation is asynchronous

The proxy captures a boundary timestamp, copies each frame into an observer channel, and continues forwarding. Tokenization, hashing, JSON classification, and trace writing happen on the observer thread rather than the primary forwarding path.

Request/response latency is correlated by JSON-RPC id using the boundary timestamps, not the later time at which the observer processes the event. This reduces measurement distortion.

## Safety

MCP traffic can contain source code, filesystem paths, credentials, private messages, database content, or other sensitive material.

By default MCPMeter stores **metrics + SHA-256 fingerprints, not raw payloads**.

Raw capture requires an explicit flag:

```bash
mcp-meter proxy --capture-payloads -- your-server
```

Treat resulting traces as sensitive. Even default traces contain metadata such as tool names, timings, sizes, and payload fingerprints. See [`SECURITY.md`](SECURITY.md).

## Repository map

```text
src/
  main.rs       CLI routing
  proxy.rs      stdio forwarding + asynchronous observer channel
  observer.rs   JSON-RPC classification/correlation
  tokenizer.rs  tokenizer profiles
  event.rs      durable trace event schema
  trace.rs      shared JSONL reader + run selection
  report.rs     run aggregation + percentile reporting
  tool_cost.rs  per-tool cost attribution
  runs.rs       append-only trace run index
  compare.rs    baseline/candidate delta engine
  fixture.rs    deterministic MCP fixture server

tests/
  proxy_smoke.rs
  cli_workflow.rs
  error_paths.rs

docs/
  ARCHITECTURE.md
  ROADMAP.md
  TRACE_FORMAT.md
```

## Development

```bash
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`CHANGELOG.md`](CHANGELOG.md).

## Design principles

1. **Measure, do not guess.** Exact and estimated metrics remain distinct.
2. **Transparent proxying.** Measurement should alter MCP behavior as little as practical.
3. **Local first.** Captured traffic can be highly sensitive.
4. **Safe logs by default.** Raw arguments/results are opt-in only.
5. **Transport aware.** stdio first; HTTP later without redefining metric semantics.
6. **Comparable runs.** Baseline/candidate output is a first-class feature.
7. **Single-purpose core.** MCPMeter measures; it is not an MCP orchestration framework.

## License

MIT — see [`LICENSE`](LICENSE).
