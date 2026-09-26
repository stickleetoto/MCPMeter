# MCPMeter trace format

MCPMeter writes append-friendly JSON Lines (`.jsonl`) traces.

The current emitted event schema version is **5**. Schema v2 added an explicit `transport` discriminator. Schema v3 added explicitly scoped `payload_bytes` so application payload size is not conflated with transport framing. Schema v4 makes `wire_bytes` optional so non-stdio transports can represent that metric as unavailable instead of inventing a value. Schema v5 adds optional Streamable HTTP request routing metadata for the allowlisted `MCP-Protocol-Version`, `Mcp-Method`, and `Mcp-Name` request headers.

Schema v1, v2, v3, and v4 traces remain readable. Missing `transport` defaults to `stdio`, and missing `payload_bytes` remains unknown rather than being guessed.

## Event fields

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | integer | Trace event schema version. New events use `5`; v1, v2, v3, and v4 remain readable. |
| `run_id` | string | Identifier shared by all events from one proxy process. |
| `ts_unix_ns` | integer | Wall-clock observation timestamp in Unix nanoseconds. |
| `transport` | string | `stdio` or `streamable_http`. Missing in v1 and defaults to `stdio` when read. |
| `http_mcp_protocol_version` | string, optional | HTTP request routing metadata from the allowlisted `MCP-Protocol-Version` header. Omitted when unavailable or redacted. |
| `http_mcp_method` | string, optional | HTTP request routing metadata from the allowlisted `Mcp-Method` header. Omitted when unavailable or redacted. |
| `http_mcp_name` | string, optional | HTTP request routing metadata from the allowlisted `Mcp-Name` header. Omitted when unavailable or redacted. |
| `direction` | string | `client_to_server` or `server_to_client`. |
| `kind` | string | Best-effort JSON-RPC classification such as `tools_call_request`, `tools_call_response`, `tools_list_request`, `tools_list_response`, `notification`, `batch`, or `malformed`. |
| `wire_bytes` | integer, optional | Exact bytes in the observed **stdio frame**, including its newline transport delimiter when present. This field's stdio meaning must not be reused for HTTP body bytes. |
| `payload_bytes` | integer, optional | Exact bytes in the MCP application payload at the observed boundary. For new stdio events this excludes the newline delimiter. Missing in schema v1/v2 and therefore unknown for old traces. |
| `serialized_tokens` | integer | Token count for the observed UTF-8 payload after removing the stdio transport newline, using the selected tokenizer profile. |
| `tokenizer` | string | Tokenizer profile used for `serialized_tokens`. |
| `token_count_estimated` | boolean | `true` when the tokenizer profile itself is heuristic. |
| `payload_sha256` | string | SHA-256 fingerprint of the complete observed frame. |
| `raw_payload` | string, optional | Raw frame/body. Absent by default and present only when capture is explicitly enabled. Configured JSON-key redaction is applied before persistence; when a payload cannot be parsed as JSON under an active payload-field rule, raw capture is omitted fail-closed. |
| `methods` | array | JSON-RPC methods observed or correlated in this frame. May be omitted by configured metadata redaction. |
| `tools` | array | Tool names observed or correlated in this frame. May be omitted by configured metadata redaction. |
| `request_count` | integer | Number of JSON-RPC request objects observed in the frame. |
| `response_count` | integer | Number of JSON-RPC response objects observed in the frame. |
| `notification_count` | integer | Number of JSON-RPC notifications observed in the frame. |
| `tool_call_count` | integer | Number of `tools/call` requests observed in the frame. |
| `tools_exposed` | integer, optional | Number of tools returned by a correlated `tools/list` response. |
| `schema_tokens` | integer, optional | Token count of a canonical compact serialization of the returned `tools` array. |
| `latencies_us` | array | Correlated request-to-response stdio-boundary latencies in microseconds. Batch frames can contain multiple samples. |
| `ok` | boolean | `false` for malformed frames and JSON-RPC error responses. |
| `parse_error` | string, optional | Parse error detail when the observed payload is malformed. |

The three `http_mcp_*` fields are HTTP request routing metadata only. They are emitted only when the corresponding allowlisted routing value is available and are otherwise omitted.

## Configurable trace redaction

Both `proxy` and `http-proxy` accept repeatable, subtractive redaction options:

```bash
mcp-meter proxy \
  --redact-metadata tools \
  --redact-payload-field api_key \
  --capture-payloads \
  -- your-server

mcp-meter http-proxy \
  --redact-metadata http-name \
  --redact-payload-field access_token \
  --capture-payloads \
  --upstream http://127.0.0.1:9000
```

`--redact-metadata <FIELD>` supports `methods`, `tools`, `http-protocol-version`, `http-method`, `http-name`, `parse-error`, and `all`. Matching metadata is omitted from the persisted event. The option may be repeated.

`--redact-payload-field <JSON_KEY>` matches JSON object keys case-insensitively and recursively. Matching values in `raw_payload` are replaced with `"[REDACTED]"`. This option may also be repeated. It does not enable raw capture: `--capture-payloads` remains required. If a raw payload is not valid JSON while one or more payload-field rules are active, MCPMeter omits `raw_payload` for that event rather than persisting content it could not safely redact.

Redaction is applied after measurement/classification but immediately before every stdio or HTTP/SSE trace write. Byte counts, token counts, schema metrics, and `payload_sha256` therefore continue to describe the original observed MCP payload; only persisted metadata/raw capture is reduced. These rules do not change schema version 5.

Safe defaults are unchanged when no rules are configured. Raw payload persistence remains off by default, and configuration cannot opt in values that MCPMeter does not capture by default. In particular, `Authorization`, `Cookie`, `Set-Cookie`, `Proxy-Authorization`, `Mcp-Param-*`, and arbitrary HTTP headers remain outside trace routing metadata. Configuration errors identify the option and supported rule shape without echoing the supplied value.

## Provider-reported usage adapter contract

MCPM-301 introduces provider-usage adapter interface version **1** without changing the schema-v5 MCP measurement event. Current proxy trace events continue to describe the observed MCP transport/payload boundary and do not synthesize provider usage.

An adapter may normalize usage only when a provider response independently reports it. The normalized `ProviderReportedUsage` envelope contains:

| Field | Type | Meaning |
| --- | --- | --- |
| `interface_version` | integer | MCPMeter provider-usage adapter interface version. MCPM-301 defines version `1`. |
| `origin` | string | Always `provider_reported`; this distinguishes the record from MCP serialized-token measurements and model-context estimates. |
| `adapter` | string | Stable adapter identifier. |
| `adapter_version` | integer | Positive adapter-specific mapping version. |
| `provider` | string | Stable provider identifier supplied by the adapter. |
| `model` | string, optional | Provider/model identity when the provider payload reports or otherwise supplies it to the adapter. |
| `input_tokens` | integer, optional | Provider-reported input token count. |
| `output_tokens` | integer, optional | Provider-reported output token count. |
| `total_tokens` | integer, optional | Provider-reported total token count. |

At least one token counter must be reported for an adapter result to be accepted. Missing counters remain absent: MCPMeter does not fill them from `serialized_tokens`, `schema_tokens`, payload bytes, another provider counter, or an estimated model context. In particular, `total_tokens` is not automatically recomputed from input/output values.

The adapter consumes a provider payload supplied by its caller; it does not make provider requests itself. Adapter errors identify contract/field names without including raw provider payloads or rejected field values.

This interface is intentionally separate from the current `MeasurementEvent` trace record. Persisting or reporting provider usage alongside benchmark runs is future integration work and must preserve the same source labeling rather than relabeling MCP observations as provider usage.

## Exact versus derived metrics

MCPMeter intentionally keeps unlike quantities separate.

### Exact at the MCP stdio boundary

- frame bytes
- MCP application payload bytes, separately from the stdio delimiter
- message direction
- observed JSON text
- selected-tokenizer token count of that exact JSON text
- request/response/notification counts after parsing
- request-to-response elapsed time between MCPMeter boundary timestamps

### Derived but deterministic

- `kind` classification
- correlated method/tool name on responses
- `tools_exposed`
- `schema_tokens`, because the tools array is compactly reserialized before tokenization
- percentile summaries generated by `mcp-meter report`

### Not measured by the stdio proxy

- the final prompt or context assembled by an MCP host
- provider billing/usage tokens unless independently supplied through a provider-usage adapter
- hidden model/system tokens
- host-side caching, truncation, deduplication, or schema transformation
- HTTP/TLS/network framing bytes

Those quantities must not be inferred from `serialized_tokens` or `wire_bytes` without an independently defined adapter or observation layer.

## Schema compatibility

Schema v5 preserves existing stdio measurements while allowing non-stdio traces to omit unavailable wire-byte metrics and HTTP request events to carry narrowly scoped routing metadata:

- writers emit `transport: "stdio"` for stdio events;
- v1 files without `transport` deserialize as stdio;
- v1/v2 files without `payload_bytes` deserialize with that metric unavailable;
- v1-v3 numeric `wire_bytes` values deserialize as present exact stdio measurements;
- non-stdio events may omit `wire_bytes` when MCPMeter has not measured an actual network boundary;
- new stdio events set `payload_bytes` to the frame size excluding the newline delimiter;
- the meanings of `wire_bytes`, `serialized_tokens`, and `latencies_us` are unchanged for stdio;
- schema v5 Streamable HTTP request events may include `http_mcp_protocol_version`, `http_mcp_method`, and `http_mcp_name`;
- the HTTP routing metadata fields are omitted when their values are unavailable, and stdio or HTTP response events do not synthesize them;
- future HTTP body/SSE accounting uses `payload_bytes` or more-specific HTTP fields rather than pretending application bytes are network wire bytes.

## Streamable HTTP schema rule

The HTTP trace extension must name its byte and timing boundaries explicitly. In particular:

- HTTP body/SSE payload bytes are application-boundary bytes, not automatically network wire bytes;
- response headers, first body byte, complete JSON body, SSE message events, and stream close are distinct timing boundaries;
- `http_mcp_protocol_version`, `http_mcp_method`, and `http_mcp_name` are HTTP request routing metadata only and are omitted when unavailable;
- `Authorization`, `Cookie`, `Set-Cookie`, `Proxy-Authorization`, `Mcp-Param-*`, and arbitrary headers are not persisted as routing metadata, and redaction configuration cannot make them eligible for capture;
- long-lived streams are observed incrementally and are never buffered solely to produce a trace.

See [`HTTP_MEASUREMENT.md`](HTTP_MEASUREMENT.md).

## Trace size limits and rotation

Trace rotation is opt-in on both producing commands:

```bash
mcp-meter proxy --trace mcpmeter.jsonl --trace-max-bytes 10485760 -- your-server

mcp-meter http-proxy \
  --trace mcpmeter.jsonl \
  --trace-max-bytes 10485760 \
  --upstream http://127.0.0.1:9000
```

Without `--trace-max-bytes`, MCPMeter keeps the existing behavior and appends to exactly the configured `--trace` path.

With a positive byte threshold, MCPMeter selects the active segment when the proxy process starts. The base file is segment 0. If the latest existing segment is smaller than the threshold, MCPMeter reopens that segment in append mode. If its size is equal to or greater than the threshold, MCPMeter selects the next numbered segment, preserving the original extension: `mcpmeter.jsonl`, `mcpmeter.1.jsonl`, `mcpmeter.2.jsonl`, and so on. Existing segments are never renamed, truncated, or deleted.

The threshold is a rollover boundary checked before a process opens its trace, not a hard per-record truncation cap. A segment may therefore finish larger than the configured value; MCPMeter never splits a JSONL record or discards trace data merely to stay under the threshold. On a later restart, the latest numbered segment is rediscovered and either appended to or advanced according to the same boundary, so rotation remains append-safe across process restarts. A value of zero is rejected.

Every segment remains an ordinary schema-v5 JSONL trace; rotation does not introduce a new trace format.

## Run boundaries

A new `run_id` is generated each time a proxy run starts. Multiple runs may be appended to one JSONL file.

Use:

```bash
mcp-meter runs mcpmeter.jsonl
```

to list runs, and:

```bash
mcp-meter report mcpmeter.jsonl --run-id <RUN_ID>
```

to select one.

## Comparing runs

A baseline and candidate can be compared directly:

```bash
mcp-meter compare baseline.jsonl candidate.jsonl
```

Or two runs in one append-only trace:

```bash
mcp-meter compare mcpmeter.jsonl mcpmeter.jsonl \
  --baseline-run-id <BASELINE> \
  --candidate-run-id <CANDIDATE>
```

Token comparisons are rejected when the selected runs used different tokenizer profiles.

## Privacy

Even without `raw_payload`, a trace still contains metadata such as tool names, method names, timings, sizes, and payload fingerprints. Treat traces as potentially sensitive.

Raw payload capture is intentionally opt-in. See [`../SECURITY.md`](../SECURITY.md).
