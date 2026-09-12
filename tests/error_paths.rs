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

    let error_response = events
        .iter()
        .find(|event| event["response_count"] == 1 && event["ok"] == false)
        .expect("error response event");
    assert_eq!(error_response["direction"], "server_to_client");

    let _ = fs::remove_file(trace);
}
