# MCPBench Proposal

> Status: proposal  
> Intended home: MCPMeter  
> Scope: controlled benchmarking built on top of MCPMeter's existing measurement contract

## Summary

MCPMeter already answers a narrow and important question:

> What did this MCP transport actually cost at the boundary we observed?

MCPBench should answer the next question:

> Under a controlled workload, how does this MCP server compare with another server or another version of itself?

The benchmark layer should be added **without weakening MCPMeter's existing measurement semantics**. Raw measurement remains authoritative. Benchmark scores and grades are derived views over explicitly named profiles.

## Why this belongs in MCPMeter

MCPMeter already has the foundations a benchmark system needs:

- deterministic run traces
- exact stdio byte accounting
- transport-aware HTTP payload accounting
- serialized token metrics under named tokenizer profiles
- tool/schema measurement
- per-tool latency and error metrics
- baseline/candidate comparison
- deterministic fixture infrastructure
- machine-readable reports
- local-first operation

Creating a separate benchmark engine would duplicate the hardest part: producing trustworthy measurements.

The proposed architecture is therefore:

```text
MCP server
    |
    v
MCPMeter measurement
    |
    +--> raw trace / exact metrics
    |
    v
MCPBench profile
    |
    +--> workload definition
    +--> metric normalization
    +--> thresholds
    +--> optional score/grade
    |
    v
benchmark report
```

## Design principle

**Measure first. Score second. Never merge the two.**

A benchmark report must always preserve access to the underlying measurements.

For example:

```text
Schema tokens              1,842
Serialized tokens/run      4,991
p50 latency                12.2 ms
p95 latency                41.8 ms
Error rate                 0.0%

Profile: mcpbench-local-v1
Performance grade: A
Token efficiency grade: B
```

The grade is a policy result. The measurements are facts about the observed run.

## Non-goals

MCPBench should not initially:

- declare one universal "best MCP server"
- hide raw metrics behind a single score
- treat serialized MCP tokens as provider-billed or model-context tokens
- combine unrelated hardware results without environment metadata
- require cloud telemetry
- execute arbitrary benchmark code from untrusted profiles
- replace the official MCP conformance suite

Protocol conformance and performance benchmarking are related but distinct.

## Relationship to official MCP conformance

The official MCP conformance project should remain the authority for protocol conformance.

MCPBench may **import or reference** conformance results, but it should not silently reinvent them.

A future combined report could look like:

```text
Protocol
  MCP 2026-07-28 required conformance: PASS
  MCP 2025-11-25 compatibility:       PASS

Efficiency
  schema tokens:       1,842
  avg run tokens:      4,991
  p95 tool latency:    41.8 ms

Reliability
  benchmark success:   100%
  MCP errors:          0

Environment
  OS:                  Windows 11
  CPU:                 ...
  transport:           streamable-http
```

## Proposed benchmark object

A benchmark should be described by a local, reviewable manifest.

Example:

```toml
version = 1
name = "calculator-basic"
profile = "mcpbench-local-v1"

[server]
transport = "stdio"
command = "calculator-mcp"
args = []

[tokenizer]
profile = "o200k-base"

[[case]]
name = "list-tools"
operation = "tools/list"

[[case]]
name = "single-add"
operation = "tools/call"
tool = "add"
arguments = { a = 2, b = 3 }
expected = { type = "success" }

[[case]]
name = "large-input"
operation = "tools/call"
tool = "sum"
arguments_file = "fixtures/large-array.json"
expected = { type = "success" }
```

The first version should prefer deterministic MCP-level workloads. Agent-level task benchmarking belongs later.

## Proposed metrics

### Transport and serialization

Directly derived from MCPMeter:

- client -> server payload bytes
- server -> client payload bytes
- exact stdio wire bytes where available
- serialized request tokens
- serialized response tokens
- schema/catalog tokens
- message counts
- batch/unattributed payload cost

### Latency

- cold start time
- request p50
- request p95
- request p99
- maximum latency
- optional time-to-first-byte for HTTP
- optional stream event latency where semantics are explicit

### Reliability

- successful cases / total cases
- MCP error responses
- transport failures
- malformed responses
- timeouts

### Surface efficiency

- number of advertised tools
- total schema tokens
- median schema tokens/tool
- tokens per successful benchmark case
- tool calls per successful benchmark case

### Environment metadata

Every published result should include enough metadata to avoid misleading comparisons:

- MCPMeter version
- benchmark profile version
- protocol version
- transport
- tokenizer profile
- OS
- architecture
- CPU model when available
- memory when available
- server version / commit when available
- warm/cold run policy

## Profiles, not universal scoring

Scores should be optional and profile-specific.

Example profiles:

- `mcpbench-local-v1`
- `mcpbench-ci-v1`
- `mcpbench-token-efficiency-v1`

A result must name the exact profile that generated a score.

Do not publish a context-free number such as:

```text
MCPBench score: 92
```

Prefer:

```text
Profile: mcpbench-local-v1
Latency: A
Reliability: A
Token efficiency: B
```

The first implementation should ship raw benchmark comparisons before shipping grades.

## Proposed CLI

Phase 1:

```bash
mcp-meter bench run benchmark.toml --trace run.jsonl
mcp-meter bench report run.jsonl
mcp-meter bench compare baseline.jsonl candidate.jsonl
```

Possible later commands:

```bash
mcp-meter bench validate benchmark.toml
mcp-meter bench export result.json --format markdown
mcp-meter bench badge result.json
```

## Result format

Benchmark results should use a versioned machine-readable schema independent from the raw trace schema.

Example:

```json
{
  "schema_version": 1,
  "benchmark": "calculator-basic",
  "profile": "mcpbench-local-v1",
  "protocol_version": "2026-07-28",
  "transport": "stdio",
  "tokenizer": "o200k_base",
  "cases": {
    "passed": 3,
    "failed": 0
  },
  "metrics": {
    "schema_tokens": 340,
    "serialized_tokens": 812,
    "latency_p50_ms": 3.1,
    "latency_p95_ms": 4.8,
    "errors": 0
  }
}
```

## Publication and badges

A later milestone can support reproducible benchmark artifacts in GitHub Actions.

Example README output:

```text
MCPBench
Protocol: 2026-07-28
Cases: 24/24
Schema: 1,842 tokens
p95: 41.8 ms
```

Badges should expose concrete measurements or a named profile grade, never an unexplained global score.

## Suggested implementation sequence

### B001 — Benchmark manifest

- define versioned benchmark manifest
- validate deterministic MCP operations
- no arbitrary shell hooks inside case definitions

### B002 — Benchmark runner

- run deterministic `tools/list` / `tools/call` cases
- capture each case through the existing MCPMeter observer path
- record environment metadata

### B003 — Benchmark report

- case pass/fail
- schema-token cost
- serialized token cost
- latency distribution
- errors/timeouts
- JSON output

### B004 — Baseline/candidate comparison

- compare benchmark result schemas
- reject incompatible tokenizer/profile comparisons
- show absolute and percentage deltas where mathematically meaningful

### B005 — CI artifact workflow

- stable JSON result
- Markdown summary
- optional badge generation

### B006 — Named scoring profiles

Only after raw result semantics have stabilized:

- versioned thresholds
- per-category grades
- no universal score by default

## Open questions

1. Should benchmark manifests launch servers, connect to already-running servers, or support both?
2. How should warm-up runs be represented?
3. Which environment fields are mandatory for public comparisons?
4. Should official MCP conformance output be embedded or linked as a separate artifact?
5. How should nondeterministic tool results be asserted without turning MCPMeter into a general test framework?

## Recommendation

Proceed with MCPBench as a **bounded benchmarking layer inside MCPMeter**, beginning with deterministic MCP-level workloads and raw comparative reports.

Do not begin with a leaderboard or global score.

The useful first milestone is simpler:

> Given the same benchmark manifest, produce two trustworthy result files and explain exactly how the candidate changed in latency, serialization cost, schema cost, and reliability.
