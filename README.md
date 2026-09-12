# MCPMeter

**Measure the real cost of Model Context Protocol (MCP) servers.**

MCPMeter is a local-first, transport-aware measurement proxy for MCP. It answers a practical question:

> How much traffic, tokenized payload, latency, and tool overhead does an MCP server add — and is that cost worth it?

MCPMeter deliberately does **not** pretend that bytes observed on the MCP wire are identical to the final context seen by a model.

## Status

**v0.1 development build — stdio measurement core is implemented.**

Current features:

- transparent stdio proxy for newline-delimited MCP JSON-RPC
- asynchronous observation so tokenization is kept off the forwarding path
- exact client→server / server→client byte accounting
- serialized payload token counting with named tokenizer profiles
- `tools/list` catalog/schema token measurement
- `tools/call` counting and tool-name attribution
- request/response correlation by JSON-RPC id
- boundary-to-boundary latency samples with p50/p95/p99 reporting
- JSON-RPC batch observation
- append-friendly JSONL traces
- SHA-256 payload fingerprints
- raw payload persistence **off by default**
- deterministic built-in MCP fixture server
- end-to-end proxy smoke test
- CI with rustfmt, clippy, and tests

Streamable HTTP measurement, provider-reported usage adapters, and A/B agent benchmarking are planned after the stdio core stabilizes.

## Install from source

```bash
cargo install --path .
```

Or during development:

```bash
cargo run -- --help
```

## Quick start

Put MCPMeter in front of any stdio MCP server:

```bash
mcp-meter proxy --trace mcpmeter.jsonl -- your-mcp-server arg1 arg2
```

Then point the MCP host at **MCPMeter** instead of directly at the server. MCPMeter launches the real server as its child process and forwards stdin/stdout unchanged.

Generate a report:

```bash
mcp-meter report mcpmeter.jsonl
```

Machine-readable report:

```bash
mcp-meter report mcpmeter.jsonl --json
```

A trace file may contain multiple runs. `report` selects the newest run by default; use `--run-id` to select a specific run.

## Tokenizer profiles

```bash
mcp-meter proxy --tokenizer o200k-base -- your-server
mcp-meter proxy --tokenizer cl100k-base -- your-server
mcp-meter proxy --tokenizer bytes4-estimate -- your-server
```

- `o200k-base`: exact tokenization of the observed serialized payload under the selected `o200k_base` encoding.
- `cl100k-base`: exact tokenization of the observed serialized payload under `cl100k_base`.
- `bytes4-estimate`: explicit heuristic (`ceil(UTF-8 bytes / 4)`) for environments where only a rough estimate is wanted.

These numbers describe **observed MCP payloads under the selected tokenizer**. They are not automatically the same as billable/model-context tokens because an MCP host may transform, cache, truncate, or reserialize tool definitions and results before sending them to a model.

## Built-in fixture

MCPMeter includes a hidden deterministic fixture server for development and regression tests:

```bash
mcp-meter proxy --tokenizer bytes4-estimate -- mcp-meter fixture
```

It exposes `add`, `echo`, and `sleep_ms` tools and understands both modern `server/discover` and the legacy `initialize` flow for compatibility testing.

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
2. **Serialized token metrics** — token count of the exact observed JSON text (excluding the newline transport delimiter) under a named tokenizer.
3. **Schema/catalog metrics** — token count of a canonical compact serialization of the `tools` array returned by `tools/list`.
4. **Model-context estimates** — future host-aware estimates, always labeled as estimates.
5. **Provider usage** — future provider-reported usage, only labeled actual when independently supplied by the provider/host.

## Why observation is asynchronous

The proxy copies each frame into an observer channel and immediately continues forwarding. Tokenization, hashing, JSON classification, and trace writing happen on the observer thread rather than in the primary forwarding path.

Latency timestamps are captured at the proxy boundary before forwarding and are correlated by JSON-RPC id. This reduces measurement distortion compared with tokenizing synchronously before sending the request onward.

## Safety

MCP traffic can contain source code, filesystem paths, credentials, private messages, database content, or other sensitive material.

By default MCPMeter stores **metrics + SHA-256 fingerprints, not raw payloads**.

Raw capture requires an explicit flag:

```bash
mcp-meter proxy --capture-payloads -- your-server
```

Treat resulting traces as sensitive. See [`SECURITY.md`](SECURITY.md).

## Repository map

```text
src/
  main.rs       CLI
  proxy.rs      stdio forwarding + observer channel
  observer.rs   JSON-RPC classification/correlation
  tokenizer.rs  tokenizer profiles
  event.rs      durable trace event schema
  report.rs     aggregation + percentile reporting
  fixture.rs    deterministic test MCP server

tests/
  proxy_smoke.rs

docs/
  ARCHITECTURE.md
  ROADMAP.md
```

## Design principles

1. **Measure, do not guess.** Exact and estimated metrics remain distinct.
2. **Transparent proxying.** Measurement should alter MCP behavior as little as practical.
3. **Local first.** Captured traffic can be highly sensitive.
4. **Safe logs by default.** Raw arguments/results are opt-in only.
5. **Transport aware.** stdio first; HTTP later without redefining metric semantics.
6. **Comparable runs.** Output is designed for later A/B agent comparisons.
7. **Single-purpose core.** MCPMeter measures; it is not an MCP orchestration framework.

## License

MIT — see [`LICENSE`](LICENSE).
