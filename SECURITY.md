# Security and Privacy

MCPMeter observes traffic between MCP clients and servers. That traffic may contain sensitive material even when the MCP server itself is harmless.

## Safe defaults

MCPMeter should default to collecting only measurement metadata such as:

- timestamps and latency
- message direction and MCP method
- byte counts
- token counts and tokenizer/profile identifiers
- tool names
- success/error state
- hashes or stable run identifiers where useful

Raw tool arguments, tool results, prompts, resource contents, environment variables, authorization headers, credentials, and filesystem contents should **not** be persisted by default.

## Capture modes

Any future raw-payload capture mode must be explicit opt-in, visibly marked as unsafe for sensitive workloads, and support redaction before persistence.

Recommended redaction targets include:

- authorization and cookie headers
- API keys and bearer tokens
- passwords and secrets
- environment variables
- user-configured JSON paths / keys
- filesystem paths when requested

## Logs

Measurement logs should be suitable for local analysis without requiring upload to a third-party service. Users should be able to delete a run by removing its local trace/report files.

## Threat model

MCPMeter is a measurement proxy, not a security boundary. It must not claim to sanitize or make an untrusted MCP server safe. Its proxy layer should preserve protocol behavior while minimizing additional attack surface.

## Reporting vulnerabilities

Do not publish credentials, sensitive captured payloads, or exploit details containing real secrets in public issues. When a private reporting channel is added, use that channel for security-sensitive reports.
