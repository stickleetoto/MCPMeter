use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn proxy_records_tools_list_and_call() {
    let exe = env!("CARGO_BIN_EXE_mcp-meter");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let trace = std::env::temp_dir().join(format!(
        "mcpmeter-smoke-{}-{unique}.jsonl",
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
        .expect("spawn proxy");

    let mut stdin = child.stdin.take().expect("proxy stdin");
    let stdout = child.stdout.take().expect("proxy stdout");
    let mut reader = BufReader::new(stdout);

    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n")
        .unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let response: Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["tools"].as_array().unwrap().len(), 3);

    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"add\",\"arguments\":{\"a\":20,\"b\":22}}}\n")
        .unwrap();
    stdin.flush().unwrap();

    line.clear();
    reader.read_line(&mut line).unwrap();
    let response: Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(response["result"]["content"][0]["text"], "42");

    drop(stdin);
    let status = child.wait().expect("wait for proxy");
    assert!(status.success());

    let trace_text = fs::read_to_string(&trace).expect("read trace");
    let events: Vec<Value> = trace_text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();

    assert!(events
        .iter()
        .any(|event| event["kind"] == "tools_list_response"));
    assert!(events
        .iter()
        .any(|event| event["kind"] == "tools_call_response"));
    assert!(events.iter().any(|event| event["tools_exposed"] == 3));
    assert!(events.iter().any(|event| event["tool_call_count"] == 1));
    assert!(events
        .iter()
        .all(|event| event.get("raw_payload").is_none()));

    let _ = fs::remove_file(trace);
}
