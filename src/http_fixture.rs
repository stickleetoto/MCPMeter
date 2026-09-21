use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::io::{self, Read};
use tiny_http::{Header, Response, Server, StatusCode};

struct FragmentedReader {
    bytes: Vec<u8>,
    offset: usize,
    max_chunk: usize,
}

impl FragmentedReader {
    fn new(bytes: Vec<u8>, max_chunk: usize) -> Self {
        Self {
            bytes,
            offset: 0,
            max_chunk,
        }
    }
}

impl Read for FragmentedReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.offset >= self.bytes.len() {
            return Ok(0);
        }

        let remaining = self.bytes.len() - self.offset;
        let amount = remaining.min(self.max_chunk).min(buffer.len());
        buffer[..amount].copy_from_slice(&self.bytes[self.offset..self.offset + amount]);
        self.offset += amount;
        Ok(amount)
    }
}

pub fn run(listen: &str) -> Result<()> {
    let server =
        Server::http(listen).map_err(|error| anyhow!("failed to listen on {listen}: {error}"))?;

    for mut request in server.incoming_requests() {
        let target = request.url().to_string();
        let mcp_method = header_value(&request, "Mcp-Method").unwrap_or_default();
        let auth_present = header_value(&request, "Authorization").is_some();

        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .context("failed to read HTTP fixture request")?;

        let parsed = serde_json::from_str::<Value>(&body);

        if target.contains("case=sse") {
            let payload = parsed.unwrap_or(Value::Null);
            let stream = sse_response_for(&payload)?;
            let headers = vec![
                Header::from_bytes("Content-Type", "text/event-stream; charset=utf-8").unwrap(),
                Header::from_bytes("Cache-Control", "no-cache").unwrap(),
                Header::from_bytes("X-Fixture-Target", target.clone()).unwrap(),
                Header::from_bytes(
                    "X-Fixture-Mcp-Method",
                    if mcp_method.is_empty() {
                        "missing"
                    } else {
                        &mcp_method
                    },
                )
                .unwrap(),
                Header::from_bytes(
                    "X-Fixture-Authorization",
                    if auth_present { "present" } else { "missing" },
                )
                .unwrap(),
            ];
            let response = Response::new(
                StatusCode(200),
                headers,
                FragmentedReader::new(stream, 5),
                None,
                None,
            );
            request
                .respond(response)
                .context("failed to send HTTP fixture SSE response")?;
            continue;
        }

        let (status, payload) = match parsed {
            Ok(value) => response_for(value),
            Err(error) => (
                400,
                json!({
                    "jsonrpc": "2.0",
                    "id": Value::Null,
                    "error": {"code": -32700, "message": format!("parse error: {error}")}
                }),
            ),
        };

        let bytes = serde_json::to_vec(&payload)?;
        let mut response = Response::from_data(bytes).with_status_code(StatusCode(status));
        response.add_header(Header::from_bytes("Content-Type", "application/json").unwrap());
        response.add_header(Header::from_bytes("X-Fixture-Target", target).unwrap());
        response.add_header(
            Header::from_bytes(
                "X-Fixture-Mcp-Method",
                if mcp_method.is_empty() {
                    "missing"
                } else {
                    &mcp_method
                },
            )
            .unwrap(),
        );
        response.add_header(
            Header::from_bytes(
                "X-Fixture-Authorization",
                if auth_present { "present" } else { "missing" },
            )
            .unwrap(),
        );

        request
            .respond(response)
            .context("failed to send HTTP fixture response")?;
    }

    Ok(())
}

fn sse_response_for(value: &Value) -> Result<Vec<u8>> {
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    let result_text = match value.pointer("/params/name").and_then(Value::as_str) {
        Some("add") => {
            let a = value
                .pointer("/params/arguments/a")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let b = value
                .pointer("/params/arguments/b")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            (a + b).to_string()
        }
        Some("echo") => value
            .pointer("/params/arguments/text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        _ => "ok".to_string(),
    };

    let notification = serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "method": "notifications/progress",
        "params": {"progress": 1, "message": "진행"}
    }))?;
    let id = serde_json::to_string(&id)?;
    let result_text = serde_json::to_string(&result_text)?;

    Ok(format!(
        ": keepalive\r\n\r\ndata: {notification}\r\n\r\ndata: {{\"jsonrpc\":\"2.0\",\"id\":{id},\r\ndata: \"result\":{{\"content\":[{{\"type\":\"text\",\"text\":{result_text}}}]}}}}\r\n\r\n"
    )
    .into_bytes())
}

fn response_for(value: Value) -> (u16, Value) {
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    match value.get("method").and_then(Value::as_str) {
        Some("tools/list") => (
            200,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        {
                            "name": "add",
                            "description": "Add two integers",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "a": {"type": "integer"},
                                    "b": {"type": "integer"}
                                },
                                "required": ["a", "b"]
                            }
                        },
                        {
                            "name": "echo",
                            "description": "Echo text",
                            "inputSchema": {
                                "type": "object",
                                "properties": {"text": {"type": "string"}},
                                "required": ["text"]
                            }
                        }
                    ]
                }
            }),
        ),
        Some("tools/call") => {
            let name = value
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            match name {
                "add" => {
                    let a = value
                        .pointer("/params/arguments/a")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let b = value
                        .pointer("/params/arguments/b")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    (
                        200,
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{"type": "text", "text": (a + b).to_string()}]
                            }
                        }),
                    )
                }
                "echo" => {
                    let text = value
                        .pointer("/params/arguments/text")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    (
                        200,
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{"type": "text", "text": text}]
                            }
                        }),
                    )
                }
                other => (
                    200,
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32602, "message": format!("unknown tool: {other}")}
                    }),
                ),
            }
        }
        Some(method) => (
            200,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {method}")}
            }),
        ),
        None => (
            400,
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32600, "message": "invalid request"}
            }),
        ),
    }
}

fn header_value(request: &tiny_http::Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.to_string().eq_ignore_ascii_case(name))
        .map(|header| header.value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_fixture_contains_notification_and_response() {
        let stream = sse_response_for(&json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {"name": "add", "arguments": {"a": 20, "b": 22}}
        }))
        .unwrap();
        let text = String::from_utf8(stream).unwrap();

        assert!(text.contains("notifications/progress"));
        assert!(text.contains("\"id\":7"));
        assert!(text.contains("\"text\":\"42\""));
        assert!(text.contains("data: "));
    }

    #[test]
    fn add_fixture_is_deterministic() {
        let (_, response) = response_for(json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {"name": "add", "arguments": {"a": 20, "b": 22}}
        }));
        assert_eq!(response["result"]["content"][0]["text"], "42");
    }
}
