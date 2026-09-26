# MCPMeter Operator Roadmap

This branch is the isolated real-project lane for DevSeat Minimal Operator long-run testing.

Rules for this lane:

- `main` is not the worker PR base.
- Worker PRs target `operator/integration-b003`.
- Implemented-but-unverified items remain `[ ]`.
- Mark `[x]` only after batch verification passes.
- Reuse existing durable Issues/branches/PRs when they already represent the same work.
- Pre-existing PR #21 (`docs: propose MCPBench benchmarking layer`) is external to this operator batch. Do not duplicate or rewrite its scope unless explicitly requested.

The underlying product roadmap remains in `docs/ROADMAP.md`.

## B003-A — Finish v0.2 reporting/storage work

- [x] MCPM-201 Add schema-cost comparison across MCP servers/runs while preserving tokenizer/profile compatibility rules. <!-- devseat: id=MCPM-201 writes=src/compare.rs,src/report.rs,tests/cli_workflow.rs,docs/ROADMAP.md -->
- [x] MCPM-202 Add configurable redaction rules for trace metadata/payload capture without weakening safe defaults. <!-- devseat: id=MCPM-202 writes=src/main.rs,src/event.rs,src/proxy.rs,src/http_proxy.rs,tests/error_paths.rs,docs/TRACE_FORMAT.md,docs/ROADMAP.md -->
- [x] MCPM-203 Add optional trace rotation / size limits with append-safe behavior and focused regression coverage. <!-- devseat: id=MCPM-203 writes=src/trace.rs,src/main.rs,tests/cli_workflow.rs,docs/TRACE_FORMAT.md -->

## B003-B — Begin v0.3 agent benchmarking

Start only after B003-A is batch-verified.

- [ ] MCPM-301 Define a versioned provider-usage adapter interface that keeps provider-reported usage distinct from observed MCP serialized tokens. <!-- devseat: id=MCPM-301 writes=src/provider_usage.rs,src/event.rs,docs/ARCHITECTURE.md,docs/TRACE_FORMAT.md -->
- [ ] MCPM-302 Add a model-context estimate adapter interface with explicit estimated labeling and no provider-billing claims. <!-- devseat: id=MCPM-302 writes=src/context_estimate.rs,src/event.rs,docs/ARCHITECTURE.md,docs/TRACE_FORMAT.md -->
- [ ] MCPM-303 Add a deterministic local A/B task-runner skeleton that records baseline/candidate run identities without becoming an orchestration framework. <!-- devseat: id=MCPM-303 writes=src/ab_runner.rs,src/main.rs,tests/cli_workflow.rs,docs/ARCHITECTURE.md -->
- [ ] MCPM-304 Add task success/test-result hooks as optional benchmark evidence, separate from transport measurements. <!-- devseat: id=MCPM-304 depends=MCPM-303 writes=src/ab_runner.rs,src/event.rs,tests/cli_workflow.rs -->
- [ ] MCPM-305 Add explicit MCP efficiency metrics derived from existing cost/latency/success evidence, with machine-readable output. <!-- devseat: id=MCPM-305 depends=MCPM-301,MCPM-302,MCPM-304 writes=src/report.rs,src/compare.rs,tests/cli_workflow.rs -->
- [ ] MCPM-306 Add batch/tool-selection analysis over recorded runs without inventing attribution for shared batch payload cost. <!-- devseat: id=MCPM-306 writes=src/tool_cost.rs,src/report.rs,tests/cli_workflow.rs -->

## Verification boundary

Use the repository-native validation once per completed batch/section:

```text
cargo fmt --all -- --check
cargo check --all-targets --all-features
cargo test --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

Workers do not run these checks. The operator runs them at the batch boundary.
