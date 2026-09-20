use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use tiny_http::{Header, Response, Server, StatusCode};

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
