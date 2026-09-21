use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn direct_json_http_proxy_forwards_and_measures_tool_call() {
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
        "mcpmeter-http-{}-{unique}.jsonl",
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
            "POST /mcp?case=smoke HTTP/1.1\r\nHost: {proxy_addr}\r\nContent-Type: application/json\r\nMCP-Protocol-Version: 2026-07-28\r\nMcp-Method: tools/call\r\nMcp-Name: add\r\nMcp-Param-query: fixture-param-marker\r\nAuthorization: fixture-auth-marker\r\nCookie: sid=fixture-cookie-marker\r\nProxy-Authorization: fixture-proxy-marker\r\nX-App-Secret: fixture-app-marker\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ),
    );

    assert!(
        response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200"),
        "unexpected response: {response}"
    );
    let (headers, response_body) = split_response(&response);
    let headers_lower = headers.to_ascii_lowercase();
    assert!(headers_lower.contains("x-fixture-target: /mcp?case=smoke"));
    assert!(headers_lower.contains("x-fixture-mcp-method: tools/call"));
    assert!(headers_lower.contains("x-fixture-authorization: present"));

    let response_json: Value =
        serde_json::from_str(response_body).expect("parse proxied fixture response");
    assert_eq!(response_json["id"], 7);
    assert_eq!(response_json["result"]["content"][0]["text"], "42");

    wait_for_trace_events(&trace, 2);

    let trace_text = fs::read_to_string(&trace).expect("read HTTP trace");
    let events: Vec<Value> = trace_text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("parse trace event"))
        .collect();

    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|event| event["transport"] == "streamable_http"));
    assert!(events
        .iter()
        .all(|event| event.get("wire_bytes").is_none() || event["wire_bytes"].is_null()));
    assert!(events
        .iter()
        .all(|event| event["payload_bytes"].as_u64().unwrap() > 0));
    assert!(events
        .iter()
        .all(|event| event["serialized_tokens"].as_u64().unwrap() > 0));
    assert!(events
        .iter()
        .all(|event| event.get("raw_payload").is_none()));

    let request_event = events
        .iter()
        .find(|event| event["direction"] == "client_to_server")
        .expect("request trace event");
    assert_eq!(request_event["schema_version"], 5);
    assert_eq!(request_event["kind"], "tools_call_request");
    assert_eq!(request_event["tools"][0], "add");
    assert_eq!(request_event["payload_bytes"], body.len() as u64);
    assert_eq!(
        request_event["http_mcp_protocol_version"],
        "2026-07-28"
    );
    assert_eq!(request_event["http_mcp_method"], "tools/call");
    assert_eq!(request_event["http_mcp_name"], "add");

    let trace_lower = trace_text.to_ascii_lowercase();
    for forbidden_header in [
        "authorization",
        "cookie",
        "set-cookie",
        "proxy-authorization",
        "mcp-param-",
        "x-app-secret",
    ] {
        assert!(
            !trace_lower.contains(forbidden_header),
            "trace unexpectedly serialized header {forbidden_header}: {trace_text}"
        );
    }
    for forbidden_value in [
        "fixture-param-marker",
        "fixture-auth-marker",
        "sid=fixture-cookie-marker",
        "fixture-proxy-marker",
        "fixture-app-marker",
    ] {
        assert!(
            !trace_text.contains(forbidden_value),
            "trace unexpectedly serialized sensitive marker: {trace_text}"
        );
    }

    let response_event = events
        .iter()
        .find(|event| event["direction"] == "server_to_client")
        .expect("response trace event");
    assert_eq!(response_event["kind"], "tools_call_response");
    assert_eq!(response_event["tools"][0], "add");
    assert_eq!(response_event["latencies_us"].as_array().unwrap().len(), 1);
    assert!(response_event.get("http_mcp_protocol_version").is_none());
    assert!(response_event.get("http_mcp_method").is_none());
    assert!(response_event.get("http_mcp_name").is_none());

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
