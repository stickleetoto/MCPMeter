# MCPMeter

Measure the real cost of Model Context Protocol (MCP) servers.

MCPMeter is a transport-aware measurement and benchmarking tool for MCP. It is designed to answer a simple question:

> How much context, traffic, latency, and tool overhead does an MCP server add — and is that cost worth it?

## Goals

MCPMeter will measure MCP overhead without pretending that every byte observed on the wire is identical to the final model context.

It separates four classes of metrics:

- **Wire metrics** — exact bytes and message counts observed at the MCP boundary.
- **Serialized token metrics** — tokens produced by tokenizing MCP schemas, requests, and responses.
- **Model-context estimates** — explicitly marked estimates when the host may transform MCP data before sending it to a model.
- **Provider usage** — actual provider-reported token usage when such data is available from the host/API.

## v0.1 Scope

The first milestone is intentionally small:

- stdio MCP proxy
- `tools/list` schema measurement
- `tools/call` request/response measurement
- wall-clock and per-call latency
- byte counts
- tokenizer profiles / estimators
- JSONL trace output
- basic CLI summary report
- safe-by-default redaction

HTTP/Streamable HTTP support, richer reports, A/B agent benchmarks, and efficiency scoring come later.

## Example report

```text
Server: Yekaterina
Transport: stdio

Tools exposed            3
Schema tokens          412
Tool calls              14
Request tokens        1,102
Response tokens       3,481
Observed MCP tokens   4,995
Wire bytes           31,842
Latency p50          2.1 ms
Latency p95          4.8 ms
```

Token figures must always state which tokenizer/profile produced them. Estimated model-context values must never be presented as exact provider usage.

## Design principles

1. **Measure, do not guess.** Exact and estimated metrics are kept separate.
2. **Transparent proxying.** Measurement should change MCP behavior as little as possible.
3. **Local first.** Captured MCP traffic may contain sensitive data.
4. **Safe logs by default.** Raw tool arguments/results are not persisted unless explicitly enabled.
5. **Transport aware.** stdio comes first; additional MCP transports are added without changing metric semantics.
6. **Comparable runs.** Output should support A/B comparisons between MCP-enabled and baseline agent runs.
7. **Single-purpose core.** MCPMeter measures; it does not become an MCP orchestration framework.

## Metric vocabulary

| Metric | Meaning |
| --- | --- |
| `wire_bytes_in/out` | Exact serialized traffic crossing the measured MCP boundary |
| `schema_tokens` | Tokenized representation of exposed tool schemas |
| `request_tokens` | Tokenized tool-call request payload |
| `response_tokens` | Tokenized tool result payload |
| `latency_ms` | Observed request/response wall time |
| `model_context_estimate` | Estimated model-visible context cost; never labeled exact |
| `provider_usage` | Provider-reported usage, when independently available |

## Safety

MCP traffic can contain source code, filesystem paths, credentials, private messages, database content, or other sensitive material. MCPMeter therefore aims to store **metrics and hashes by default**, not raw payloads.

See [`SECURITY.md`](SECURITY.md) before using capture/debug modes on real workloads.

## Project status

**Pre-alpha / initial design.** The repository is being bootstrapped around the v0.1 measurement contract.

## License

MIT — see [`LICENSE`](LICENSE).
