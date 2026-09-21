use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn sse_http_proxy_streams_and_classifies_json_rpc_events() {
    let exe = env!("CARGO_BIN_EXE_mcp-meter");
    let upstream_port = free_port();
    let proxy_port = free_port();
    let upstream_addr = format!("127.0.0.1:{upstream_port}");
    let proxy_addr = format!("127.0.0.1:{proxy_port}");

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let trace = std::env::temp_dir().join(format!(
        "mcpmeter-sse-{}-{unique}.jsonl",
        std::process::id()
    ));

    let mut fixture = Command::new(exe)
        .arg("http-fixture")
        .arg("--listen")
        .arg(&upstream_addr)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn HTTP fixture");
    wait_for_port(&upstream_addr, &mut fixture);

    let mut proxy = Command::new(exe)
        .arg("http-proxy")
        .arg("--listen")
        .arg(&proxy_addr)
        .arg("--upstream")
        .arg(format!("http://{upstream_addr}"))
        .arg("--trace")
        .arg(&trace)
        .arg("--tokenizer")
        .arg("bytes4-estimate")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn HTTP proxy");
    wait_for_port(&proxy_addr, &mut proxy);

    let body = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"add","arguments":{"a":20,"b":22}}}"#;
    let response = send_http(
        &proxy_addr,
        &format!(
            "POST /mcp?case=sse HTTP/1.0\r\nHost: {proxy_addr}\r\nContent-Type: application/json\r\nAccept: text/event-stream\r\nMCP-Protocol-Version: 2026-07-28\r\nMcp-Method: tools/call\r\nMcp-Name: add\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ),
    );

    assert!(
        response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200"),
        "unexpected response: {response}"
    );

    let (headers, response_body) = split_response(&response);
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-type: text/event-stream")
    );
    assert!(response_body.contains(": keepalive"));
    assert!(response_body.contains("notifications/progress"));
    assert!(response_body.contains("\"id\":7"));
    assert!(response_body.contains("\"text\":\"42\""));

    wait_for_trace_events(&trace, 3);

    let trace_text = fs::read_to_string(&trace).expect("read SSE trace");
    let events: Vec<Value> = trace_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("parse trace event"))
        .collect();

    assert_eq!(events.len(), 3);
    assert!(events
        .iter()
        .all(|event| event["transport"] == "streamable_http"));
    assert!(events
        .iter()
        .all(|event| event.get("wire_bytes").is_none() || event["wire_bytes"].is_null()));

    let request_event = events
        .iter()
        .find(|event| event["direction"] == "client_to_server")
        .expect("request event");
    assert_eq!(request_event["kind"], "tools_call_request");
    assert_eq!(request_event["tools"][0], "add");

    let notification_event = events
        .iter()
        .find(|event| event["kind"] == "notification")
        .expect("SSE notification event");
    assert_eq!(notification_event["methods"][0], "notifications/progress");
    assert!(notification_event["serialized_tokens"].as_u64().unwrap() > 0);
    assert!(notification_event["payload_bytes"].as_u64().unwrap() > 0);

    let response_event = events
        .iter()
        .find(|event| event["kind"] == "tools_call_response")
        .expect("SSE tool response event");
    assert_eq!(response_event["tools"][0], "add");
    assert_eq!(response_event["methods"][0], "tools/call");
    assert_eq!(response_event["latencies_us"].as_array().unwrap().len(), 1);
    assert!(response_event["serialized_tokens"].as_u64().unwrap() > 0);
    assert!(response_event["payload_bytes"].as_u64().unwrap() > 0);
    assert!(response_event.get("raw_payload").is_none());

    cleanup(&mut proxy);
    cleanup(&mut fixture);
    let _ = fs::remove_file(trace);
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral port")
        .local_addr()
        .unwrap()
        .port()
}

fn wait_for_port(addr: &str, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        if let Some(status) = child.try_wait().expect("check child status") {
            panic!("child exited before listening on {addr}: {status}");
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for child to listen on {addr}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn send_http(addr: &str, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("connect to HTTP proxy");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream.write_all(request.as_bytes()).expect("write request");
    stream.flush().expect("flush request");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read complete HTTP response");
    response
}

fn split_response(response: &str) -> (&str, &str) {
    response
        .split_once("\r\n\r\n")
        .expect("HTTP response header/body separator")
}

fn wait_for_trace_events(path: &std::path::Path, expected: usize) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let count = fs::read_to_string(path)
            .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
            .unwrap_or(0);
        if count >= expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected} trace events"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn cleanup(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
}
