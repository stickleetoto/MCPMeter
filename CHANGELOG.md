# Changelog

All notable changes to MCPMeter will be documented here.

The project follows semantic versioning once a release is tagged.

## Unreleased

### Added

- Rust single-binary CLI.
- Transparent newline-delimited stdio MCP proxy.
- Asynchronous observation path for tokenization, hashing, parsing, and trace writing.
- Exact bidirectional wire-byte accounting.
- `o200k_base`, `cl100k_base`, and explicit `bytes4_estimate` tokenizer profiles.
- JSON-RPC request, response, notification, batch, and malformed-message observation.
- `tools/list` exposed-tool and canonical schema-token measurement.
- `tools/call` counting and request/response correlation by JSON-RPC id.
- Boundary latency samples and p50/p95/p99/max summaries.
- Append-friendly JSONL trace format with schema versioning.
- SHA-256 payload fingerprints with raw payload persistence disabled by default.
- `report` command with text and JSON output.
- `runs` command for append-only trace navigation.
- `compare` command for baseline/candidate A/B measurement with tokenizer compatibility checks.
- Deterministic fixture MCP server with `add`, `echo`, and `sleep_ms` tools.
- Unit tests plus end-to-end stdio proxy and CLI workflow tests.
- Linux and Windows CI quality gates.
- Security, architecture, roadmap, trace-format, and contribution documentation.
