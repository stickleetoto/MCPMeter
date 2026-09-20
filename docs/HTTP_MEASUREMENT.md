# Streamable HTTP measurement contract

This document defines what MCPMeter may truthfully claim when it measures MCP over HTTP.

The primary protocol target is MCP `2026-07-28`, where requests are stateless and self-contained. Compatibility with handshake/session-era traffic remains useful, but it must not weaken the metric definitions below.

## Protocol behavior MCPMeter must preserve

A transparent HTTP proxy must forward modern MCP request metadata without rewriting its meaning, including:

- HTTP method and path/query;
- `MCP-Protocol-Version`;
- `Mcp-Method`;
- `Mcp-Name` when present;
- `Mcp-Param-*` headers;
- authorization and tracing headers;
- request and response content types;
- upstream response status and relevant response headers.

MCPMeter is an observer, not an MCP gateway. It should not synthesize missing protocol headers, repair malformed requests, negotiate authorization, or reinterpret a server response.

## Byte accounting

### stdio

The existing v1 `wire_bytes` field is exact for each observed stdio frame, including the newline delimiter when present.

### HTTP

At the HTTP application boundary, MCPMeter can exactly count the bytes it receives for:

- a request body;
- a direct response body;
- each streamed response chunk;
- reconstructed SSE event payloads when parsing succeeds.

Those are **application payload bytes**.

They are not necessarily equal to bytes transmitted on the network. Depending on the connection, the network representation can include or transform:

- HTTP/1 request/status lines and headers;
- chunked-transfer framing;
- HTTP/2 or HTTP/3 frame overhead and header compression;
- content encoding/compression;
- TLS records;
- TCP/IP or QUIC overhead.

Therefore the first HTTP trace schema must use names such as `request_body_bytes`, `response_body_bytes`, `stream_chunk_bytes`, or another explicitly scoped equivalent. It must not silently reuse `wire_bytes` for body sizes.

If a future lower-level adapter measures actual network bytes, that adapter must identify the layer and protocol it measured.

## Token accounting

Serialized-token metrics apply only to JSON text MCPMeter actually observes.

For a direct JSON response, tokenization may run on the complete JSON body after it is copied for observation.

For SSE, MCPMeter may tokenize complete JSON values reconstructed from `data:` event payloads. It must not tokenize arbitrary transport chunks as though chunk boundaries were MCP message boundaries.

Tokenizer work stays off the forwarding-critical path where practical.

## Timing

HTTP has several useful clocks. They must not be collapsed into one ambiguous `latency`.

Recommended boundaries:

- `request_received`: MCPMeter has accepted the downstream request;
- `upstream_dispatch`: forwarding to the upstream server starts;
- `response_headers`: upstream status/headers are available;
- `first_response_body_byte`: first response body bytes arrive;
- `message_complete`: a complete direct JSON-RPC response, or a complete JSON-RPC message reconstructed from SSE, is available;
- `stream_closed`: the upstream response stream ends.

For a normal direct JSON-RPC exchange, reports may expose request-to-first-byte and request-to-message-complete samples separately.

For a long-lived `subscriptions/listen` response, stream lifetime is not ordinary RPC latency. JSON-RPC response messages that appear inside the stream may still be correlated individually by request id.

## SSE forwarding

A Streamable HTTP proxy must not wait for the entire SSE response before replying to the client.

The forwarding path should:

1. pass upstream response status and headers downstream;
2. forward body chunks as they arrive;
3. tee/copy the chunks to an observer path;
4. reconstruct SSE events incrementally;
5. inspect complete `data:` event payloads when they contain JSON;
6. tolerate comments, keepalive frames, split UTF-8 boundaries, and events that span multiple transport chunks;
7. keep forwarding even if observation, tokenization, or trace writing fails.

## Header metadata and privacy

Safe-by-default traces may record selected non-secret metadata such as:

- protocol version;
- `Mcp-Method`;
- `Mcp-Name`;
- content type;
- HTTP status;
- presence/count information for selected header classes.

Default traces must not persist values from:

- `Authorization`;
- `Cookie` / `Set-Cookie`;
- `Proxy-Authorization`;
- `Mcp-Param-*`;
- arbitrary application headers that may contain secrets.

Raw HTTP body capture remains opt-in and carries the same warning as stdio raw payload capture.

## Compatibility eras

### 2026-07-28

This is the primary implementation target. It is stateless at the protocol layer, uses per-request protocol metadata, and carries long-lived change notifications through `subscriptions/listen`.

### 2025-era traffic

The reverse proxy should avoid breaking legacy Streamable HTTP traffic when it can transparently relay it, including session headers and legacy streams. Legacy-specific semantic analysis can be added after the modern measurement path is stable.

## Acceptance rule

An HTTP metric may ship only when its name and documentation answer:

> Exactly which boundary did MCPMeter observe to produce this number?

If that answer is ambiguous, the metric is not ready.
