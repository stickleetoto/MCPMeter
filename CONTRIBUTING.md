# Contributing to MCPMeter

Thanks for helping improve MCPMeter.

## Principles

Changes should preserve these invariants:

1. Do not label an estimate as exact provider/model usage.
2. Keep the forwarding path as transparent and low-overhead as practical.
3. Raw MCP payload persistence remains opt-in.
4. Measurement failures should not silently corrupt MCP traffic.
5. New trace fields require a schema-version decision and documentation.
6. Tests should prefer deterministic fixtures over external services.

## Development

```bash
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

The repository CI runs the same quality gates on Linux and Windows.

## Testing MCP behavior

The project includes a deterministic fixture server:

```bash
cargo run -- proxy --tokenizer bytes4-estimate -- cargo run -- fixture
```

For transport changes, add an end-to-end regression test that exercises the proxy boundary instead of testing only the parser.

## Trace compatibility

The JSONL event format is documented in [`docs/TRACE_FORMAT.md`](docs/TRACE_FORMAT.md).

When changing it:

- prefer additive optional fields when backward compatibility is possible;
- bump `schema_version` when existing readers would misinterpret a new event;
- keep exact, derived, estimated, and provider-reported metrics distinguishable;
- never add raw secrets to default traces.

## Pull requests

Keep changes focused. Include:

- what changed;
- why the measurement remains trustworthy;
- tests covering new behavior;
- any trace-format or security implications.

Before opening a PR, run the full quality gate above.
