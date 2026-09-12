use anyhow::Result;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::thread;
use std::time::Duration;

pub fn run() -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let value: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": format!("Parse error: {error}")}
                });
                writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
                stdout.flush()?;
                continue;
            }
        };

        match value {
            Value::Array(items) => {
                let responses: Vec<Value> = items.iter().filter_map(handle_message).collect();
                if !responses.is_empty() {
                    writeln!(stdout, "{}", serde_json::to_string(&responses)?)?;
                    stdout.flush()?;
                }
            }
            other => {
                if let Some(response) = handle_message(&other) {
                    writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
                    stdout.flush()?;
                }
            }
        }
    }

    Ok(())
}

fn handle_message(message: &Value) -> Option<Value> {
    let object = message.as_object()?;
    let method = object.get("method")?.as_str()?;
    let id = object.get("id").cloned()?;

    let result = match method {
        "server/discover" => json!({
            "resultType": "complete",
            "supportedVersions": ["2026-07-28", "2025-06-18"],
            "capabilities": {"tools": {}},
            "_meta": {
                "io.modelcontextprotocol/serverInfo": {
                    "name": "mcpmeter-fixture",
                    "version": env!("CARGO_PKG_VERSION")
                }
            },
            "instructions": "Deterministic fixture server for MCPMeter tests.",
            "ttlMs": 60000,
            "cacheScope": "public"
        }),
        "initialize" => {
            let requested = object
                .get("params")
                .and_then(|v| v.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or("2025-06-18");
            json!({
                "protocolVersion": requested,
                "capabilities": {"tools": {}},
                "serverInfo": {
                    "name": "mcpmeter-fixture",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({
            "tools": [
                {
                    "name": "add",
                    "description": "Add two numbers deterministically.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "a": {"type": "number"},
                            "b": {"type": "number"}
                        },
                        "required": ["a", "b"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "echo",
                    "description": "Return the supplied text.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"text": {"type": "string"}},
                        "required": ["text"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "sleep_ms",
                    "description": "Sleep for a bounded number of milliseconds for latency tests.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "ms": {"type": "integer", "minimum": 0, "maximum": 5000}
                        },
                        "required": ["ms"],
                        "additionalProperties": false
                    }
                }
            ],
            "ttlMs": 60000,
            "cacheScope": "public"
        }),
        "tools/call" => {
            let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            return Some(tool_response(id, name, &arguments));
        }
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("Method not found: {method}")}
            }));
        }
    };

    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn tool_response(id: Value, name: &str, arguments: &Value) -> Value {
    let text = match name {
        "add" => {
            let a = arguments.get("a").and_then(Value::as_f64).unwrap_or(0.0);
            let b = arguments.get("b").and_then(Value::as_f64).unwrap_or(0.0);
            format_number(a + b)
        }
        "echo" => arguments
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        "sleep_ms" => {
            let ms = arguments
                .get("ms")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(5_000);
            thread::sleep(Duration::from_millis(ms));
            format!("slept {ms} ms")
        }
        _ => {
            return json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{"type": "text", "text": format!("Unknown tool: {name}")}],
                    "isError": true
                }
            });
        }
    };

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{"type": "text", "text": text}],
            "isError": false
        }
    })
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_add_is_deterministic() {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "add", "arguments": {"a": 2, "b": 3}}
        });
        let response = handle_message(&request).unwrap();
        assert_eq!(response["result"]["content"][0]["text"], "5");
    }

    #[test]
    fn discovery_advertises_current_protocol() {
        let request = json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{}});
        let response = handle_message(&request).unwrap();
        assert_eq!(response["result"]["supportedVersions"][0], "2026-07-28");
    }
}
