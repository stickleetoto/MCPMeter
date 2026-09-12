use crate::event::{Direction, MeasurementEvent};
use crate::tokenizer::TokenizerProfile;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct PendingRequest {
    method: String,
    tool: Option<String>,
    started: Instant,
}

pub type PendingMap = HashMap<String, PendingRequest>;

#[derive(Default)]
struct Facts {
    methods: Vec<String>,
    tools: Vec<String>,
    request_count: u64,
    response_count: u64,
    notification_count: u64,
    tool_call_count: u64,
    tools_exposed: Option<u64>,
    schema_tokens: Option<u64>,
    latencies_us: Vec<u64>,
    ok: bool,
}

pub fn observe_payload(
    payload: &[u8],
    direction: Direction,
    run_id: &str,
    tokenizer: &TokenizerProfile,
    capture_payloads: bool,
    pending: &mut PendingMap,
) -> MeasurementEvent {
    let ts_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut hasher = Sha256::new();
    hasher.update(payload);
    let payload_sha256 = format!("{:x}", hasher.finalize());

    let text_result = std::str::from_utf8(payload);
    let token_text = text_result
        .ok()
        .map(strip_transport_newline)
        .unwrap_or_default();
    let serialized_tokens = tokenizer.count(token_text) as u64;

    let mut facts = Facts {
        ok: true,
        ..Facts::default()
    };

    let mut parse_error = None;
    let mut root_is_batch = false;

    match text_result {
        Ok(text) => match serde_json::from_str::<Value>(strip_transport_newline(text)) {
            Ok(value) => {
                root_is_batch = value.is_array();
                inspect_value(&value, direction, tokenizer, pending, &mut facts);
            }
            Err(error) => {
                facts.ok = false;
                parse_error = Some(error.to_string());
            }
        },
        Err(error) => {
            facts.ok = false;
            parse_error = Some(format!("invalid UTF-8: {error}"));
        }
    }

    let kind = classify_kind(root_is_batch, parse_error.is_some(), &facts);

    MeasurementEvent {
        schema_version: 1,
        run_id: run_id.to_string(),
        ts_unix_ns,
        direction,
        kind,
        wire_bytes: payload.len() as u64,
        serialized_tokens,
        tokenizer: tokenizer.name().to_string(),
        token_count_estimated: tokenizer.is_estimate(),
        payload_sha256,
        raw_payload: if capture_payloads {
            text_result.ok().map(ToOwned::to_owned)
        } else {
            None
        },
        methods: facts.methods,
        tools: facts.tools,
        request_count: facts.request_count,
        response_count: facts.response_count,
        notification_count: facts.notification_count,
        tool_call_count: facts.tool_call_count,
        tools_exposed: facts.tools_exposed,
        schema_tokens: facts.schema_tokens,
        latencies_us: facts.latencies_us,
        ok: facts.ok,
        parse_error,
    }
}

fn inspect_value(
    value: &Value,
    direction: Direction,
    tokenizer: &TokenizerProfile,
    pending: &mut PendingMap,
    facts: &mut Facts,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                inspect_value(item, direction, tokenizer, pending, facts);
            }
        }
        Value::Object(object) => {
            if let Some(method) = object.get("method").and_then(Value::as_str) {
                facts.methods.push(method.to_string());
                let tool = if method == "tools/call" {
                    object
                        .get("params")
                        .and_then(|v| v.get("name"))
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                } else {
                    None
                };

                if let Some(tool_name) = &tool {
                    facts.tools.push(tool_name.clone());
                    facts.tool_call_count += 1;
                }

                if let Some(id) = object.get("id") {
                    facts.request_count += 1;
                    if matches!(direction, Direction::ClientToServer) {
                        pending.insert(
                            id_key(id),
                            PendingRequest {
                                method: method.to_string(),
                                tool,
                                started: Instant::now(),
                            },
                        );
                    }
                } else {
                    facts.notification_count += 1;
                }
                return;
            }

            if object.contains_key("id")
                && (object.contains_key("result") || object.contains_key("error"))
            {
                facts.response_count += 1;
                if object.contains_key("error") {
                    facts.ok = false;
                }

                if matches!(direction, Direction::ServerToClient) {
                    if let Some(id) = object.get("id") {
                        if let Some(request) = pending.remove(&id_key(id)) {
                            facts.methods.push(request.method.clone());
                            if let Some(tool) = request.tool {
                                facts.tools.push(tool);
                            }
                            let micros = request.started.elapsed().as_micros();
                            facts.latencies_us.push(micros.min(u64::MAX as u128) as u64);

                            if request.method == "tools/list" {
                                if let Some(tools) = object
                                    .get("result")
                                    .and_then(|v| v.get("tools"))
                                {
                                    if let Some(array) = tools.as_array() {
                                        let count = array.len() as u64;
                                        *facts.tools_exposed.get_or_insert(0) += count;
                                    }
                                    if let Ok(canonical) = serde_json::to_string(tools) {
                                        let count = tokenizer.count(&canonical) as u64;
                                        *facts.schema_tokens.get_or_insert(0) += count;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {
            facts.ok = false;
        }
    }
}

fn classify_kind(batch: bool, malformed: bool, facts: &Facts) -> String {
    if malformed {
        return "malformed".to_string();
    }
    if batch {
        return "batch".to_string();
    }
    if facts.request_count == 1 {
        return match facts.methods.first().map(String::as_str) {
            Some("tools/list") if facts.response_count > 0 => "tools_list_response",
            Some("tools/list") => "tools_list_request",
            Some("tools/call") if facts.response_count > 0 => "tools_call_response",
            Some("tools/call") => "tools_call_request",
            _ if facts.response_count > 0 => "response",
            _ => "request",
        }
        .to_string();
    }
    if facts.response_count == 1 {
        return match facts.methods.first().map(String::as_str) {
            Some("tools/list") => "tools_list_response",
            Some("tools/call") => "tools_call_response",
            _ => "response",
        }
        .to_string();
    }
    if facts.notification_count == 1 {
        return "notification".to_string();
    }
    "message".to_string()
}

fn id_key(id: &Value) -> String {
    serde_json::to_string(id).unwrap_or_else(|_| "null".to_string())
}

fn strip_transport_newline(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlates_tool_call_response() {
        let tokenizer = TokenizerProfile::Bytes4Estimate;
        let mut pending = PendingMap::new();
        let request = br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"add","arguments":{"a":1,"b":2}}}
"#;
        let response = br#"{"jsonrpc":"2.0","id":7,"result":{"content":[{"type":"text","text":"3"}]}}
"#;

        let req = observe_payload(
            request,
            Direction::ClientToServer,
            "test",
            &tokenizer,
            false,
            &mut pending,
        );
        assert_eq!(req.tool_call_count, 1);
        assert_eq!(req.tools, vec!["add"]);
        assert_eq!(pending.len(), 1);

        let res = observe_payload(
            response,
            Direction::ServerToClient,
            "test",
            &tokenizer,
            false,
            &mut pending,
        );
        assert_eq!(res.kind, "tools_call_response");
        assert_eq!(res.latencies_us.len(), 1);
        assert!(pending.is_empty());
    }

    #[test]
    fn measures_tool_catalog_schema() {
        let tokenizer = TokenizerProfile::Bytes4Estimate;
        let mut pending = PendingMap::new();
        let request = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n";
        let response = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[{\"name\":\"a\",\"inputSchema\":{\"type\":\"object\"}},{\"name\":\"b\",\"inputSchema\":{\"type\":\"object\"}}]}}\n";

        observe_payload(
            request,
            Direction::ClientToServer,
            "test",
            &tokenizer,
            false,
            &mut pending,
        );
        let res = observe_payload(
            response,
            Direction::ServerToClient,
            "test",
            &tokenizer,
            false,
            &mut pending,
        );

        assert_eq!(res.tools_exposed, Some(2));
        assert!(res.schema_tokens.unwrap() > 0);
    }
}
