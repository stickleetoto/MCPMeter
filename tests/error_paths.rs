use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn malformed_request_and_error_response_are_recorded() {
    let exe = env!("CARGO_BIN_EXE_mcp-meter");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let trace = std::env::temp_dir().join(format!(
        "mcpmeter-error-{}-{unique}.jsonl",
        std::process::id()
    ));

    let mut child = Command::new(exe)
        .arg("proxy")
        .arg("--trace")
        .arg(&trace)
        .arg("--tokenizer")
        .arg("bytes4-estimate")
        .arg("--")
        .arg(exe)
        .arg("fixture")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn measured fixture");

    let mut stdin = child.stdin.take().expect("proxy stdin");
    let stdout = child.stdout.take().expect("proxy stdout");
    let mut reader = BufReader::new(stdout);

    stdin.write_all(b"{not-json}\n").unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let response: Value = serde_json::from_str(line.trim()).expect("fixture error response");
    assert_eq!(response["error"]["code"], -32700);

    drop(stdin);
    let status = child.wait().expect("wait for proxy");
    assert!(status.success());

    let trace_text = fs::read_to_string(&trace).expect("read trace");
    let events: Vec<Value> = trace_text
        .lines()
        .map(|event| serde_json::from_str(event).unwrap())
        .collect();

    let malformed = events
        .iter()
        .find(|event| event["kind"] == "malformed")
        .expect("malformed event");
    assert_eq!(malformed["ok"], false);
    assert!(malformed["parse_error"].as_str().is_some());
    assert!(malformed["wire_bytes"].as_u64().unwrap() > 0);
    assert_eq!(
        malformed["payload_bytes"].as_u64().unwrap() + 1,
        malformed["wire_bytes"].as_u64().unwrap()
    );
    assert!(malformed.get("raw_payload").is_none());

    let error_response = events
        .iter()
        .find(|event| event["response_count"] == 1 && event["ok"] == false)
        .expect("error response event");
    assert_eq!(error_response["direction"], "server_to_client");

    let _ = fs::remove_file(trace);
}

#[test]
fn configured_redaction_applies_before_stdio_trace_write() {
    let exe = env!("CARGO_BIN_EXE_mcp-meter");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let trace = std::env::temp_dir().join(format!(
        "mcpmeter-redaction-{}-{unique}.jsonl",
        std::process::id()
    ));
    let secret = "fixture-api-key-secret-marker";

    let mut child = Command::new(exe)
        .arg("proxy")
        .arg("--trace")
        .arg(&trace)
        .arg("--tokenizer")
        .arg("bytes4-estimate")
        .arg("--capture-payloads")
        .arg("--redact-metadata")
        .arg("tools")
        .arg("--redact-payload-field")
        .arg("api_key")
        .arg("--")
        .arg(exe)
        .arg("fixture")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn redacted fixture");

    let mut stdin = child.stdin.take().expect("proxy stdin");
    let stdout = child.stdout.take().expect("proxy stdout");
    let mut reader = BufReader::new(stdout);

    let request = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{{\"name\":\"add\",\"arguments\":{{\"a\":1,\"b\":2,\"api_key\":\"{secret}\"}}}}}}\n"
    );
    stdin.write_all(request.as_bytes()).unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let _: Value = serde_json::from_str(line.trim()).expect("fixture response");

    drop(stdin);
    let status = child.wait().expect("wait for redacted proxy");
    assert!(status.success());

    let trace_text = fs::read_to_string(&trace).expect("read redacted trace");
    assert!(!trace_text.contains(secret));

    let event: Value = trace_text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|event| {
            event["direction"] == "client_to_server" && event["tool_call_count"] == 1
        })
        .expect("redacted tool request event");

    assert!(event.get("tools").is_none());
    assert_eq!(event["methods"][0], "tools/call");
    let raw_payload = event["raw_payload"].as_str().expect("captured payload");
    let raw: Value = serde_json::from_str(raw_payload).expect("redacted raw payload");
    assert_eq!(
        raw["params"]["arguments"]["api_key"],
        Value::String("[REDACTED]".to_string())
    );

    let _ = fs::remove_file(trace);
}

#[test]
fn invalid_redaction_config_does_not_echo_supplied_value() {
    let exe = env!("CARGO_BIN_EXE_mcp-meter");
    let secret = "sk-test-secret-config-marker";

    let output = Command::new(exe)
        .arg("proxy")
        .arg("--redact-metadata")
        .arg(secret)
        .arg("--")
        .arg(exe)
        .arg("fixture")
        .output()
        .expect("run invalid redaction configuration");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported --redact-metadata rule"));
    assert!(!stderr.contains(secret));
}
