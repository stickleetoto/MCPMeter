use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn runs_tools_and_compare_work_end_to_end() {
    let exe = env!("CARGO_BIN_EXE_mcp-meter");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let trace = std::env::temp_dir().join(format!(
        "mcpmeter-workflow-{}-{unique}.jsonl",
        std::process::id()
    ));

    record_session(exe, &trace, 1, "bytes4-estimate");
    record_session(exe, &trace, 2, "bytes4-estimate");

    let output = Command::new(exe)
        .arg("runs")
        .arg(&trace)
        .arg("--json")
        .output()
        .expect("run list command");
    assert!(output.status.success());

    let runs: Value = serde_json::from_slice(&output.stdout).expect("parse run list");
    let runs = runs.as_array().expect("run list array");
    assert_eq!(runs.len(), 2);

    let candidate_id = runs[0]["run_id"].as_str().unwrap();
    let baseline_id = runs[1]["run_id"].as_str().unwrap();

    let output = Command::new(exe)
        .arg("tools")
        .arg(&trace)
        .arg("--run-id")
        .arg(candidate_id)
        .arg("--json")
        .output()
        .expect("run per-tool cost command");
    assert!(output.status.success());

    let tool_cost: Value = serde_json::from_slice(&output.stdout).expect("parse tool cost");
    assert_eq!(tool_cost["tokenizer"], "bytes4_estimate");
    let tools = tool_cost["tools"].as_array().expect("tools array");
    let add = tools
        .iter()
        .find(|tool| tool["tool"] == "add")
        .expect("add tool cost");
    assert_eq!(add["calls"], 2);
    assert!(add["request_tokens"].as_u64().unwrap() > 0);
    assert!(add["response_tokens"].as_u64().unwrap() > 0);
    assert_eq!(tool_cost["unattributed_batch_request_tokens"], 0);
    assert_eq!(tool_cost["unattributed_batch_response_tokens"], 0);

    let output = Command::new(exe)
        .arg("compare")
        .arg(&trace)
        .arg(&trace)
        .arg("--baseline-run-id")
        .arg(baseline_id)
        .arg("--candidate-run-id")
        .arg(candidate_id)
        .arg("--json")
        .output()
        .expect("run compare command");
    assert!(output.status.success());

    let comparison: Value = serde_json::from_slice(&output.stdout).expect("parse comparison");
    assert_eq!(comparison["tokenizer"], "bytes4_estimate");
    assert_eq!(comparison["tool_calls"]["baseline"], 1);
    assert_eq!(comparison["tool_calls"]["candidate"], 2);
    assert_eq!(comparison["tool_calls"]["delta"], 1);
    assert!(comparison["serialized_tokens"]["delta"].as_i64().unwrap() > 0);
    assert_eq!(comparison["schema_tokens"]["delta"], 0);
    assert_eq!(comparison["tools_exposed"]["delta"], 0);
    assert_eq!(
        comparison["schema_tokens_per_tool"]["delta"].as_f64(),
        Some(0.0)
    );

    let _ = fs::remove_file(trace);
}

#[test]
fn compare_rejects_different_tokenizers() {
    let exe = env!("CARGO_BIN_EXE_mcp-meter");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let baseline = std::env::temp_dir().join(format!(
        "mcpmeter-compare-baseline-{}-{unique}.jsonl",
        std::process::id()
    ));
    let candidate = std::env::temp_dir().join(format!(
        "mcpmeter-compare-candidate-{}-{unique}.jsonl",
        std::process::id()
    ));

    record_session(exe, &baseline, 1, "bytes4-estimate");
    record_session(exe, &candidate, 1, "cl100k-base");

    let output = Command::new(exe)
        .arg("compare")
        .arg(&baseline)
        .arg(&candidate)
        .arg("--json")
        .output()
        .expect("run incompatible compare command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("different tokenizers"));

    let _ = fs::remove_file(baseline);
    let _ = fs::remove_file(candidate);
}

#[test]
fn compare_rejects_different_estimation_profiles() {
    let exe = env!("CARGO_BIN_EXE_mcp-meter");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let baseline = std::env::temp_dir().join(format!(
        "mcpmeter-profile-baseline-{}-{unique}.jsonl",
        std::process::id()
    ));
    let candidate = std::env::temp_dir().join(format!(
        "mcpmeter-profile-candidate-{}-{unique}.jsonl",
        std::process::id()
    ));

    record_session(exe, &baseline, 1, "bytes4-estimate");
    record_session(exe, &candidate, 1, "bytes4-estimate");
    rewrite_estimation_profile(&candidate, false);

    let output = Command::new(exe)
        .arg("compare")
        .arg(&baseline)
        .arg(&candidate)
        .arg("--json")
        .output()
        .expect("run profile-incompatible compare command");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("different tokenizer profiles"));

    let _ = fs::remove_file(baseline);
    let _ = fs::remove_file(candidate);
}

fn record_session(exe: &str, trace: &Path, tool_calls: u64, tokenizer: &str) {
    let mut child = Command::new(exe)
        .arg("proxy")
        .arg("--trace")
        .arg(trace)
        .arg("--tokenizer")
        .arg(tokenizer)
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

    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\",\"params\":{}}\n")
        .unwrap();
    stdin.flush().unwrap();
    read_response(&mut reader);

    for index in 0..tool_calls {
        let request = format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{},\"method\":\"tools/call\",\"params\":{{\"name\":\"add\",\"arguments\":{{\"a\":{},\"b\":1}}}}}}\n",
            index + 10,
            index
        );
        stdin.write_all(request.as_bytes()).unwrap();
        stdin.flush().unwrap();
        read_response(&mut reader);
    }

    drop(stdin);
    let status = child.wait().expect("wait for measured fixture");
    assert!(status.success());
}

fn rewrite_estimation_profile(trace: &Path, estimated: bool) {
    let input = fs::read_to_string(trace).expect("read trace for profile rewrite");
    let mut output = String::new();

    for line in input.lines() {
        let mut event: Value = serde_json::from_str(line).expect("parse trace event");
        event["token_count_estimated"] = Value::Bool(estimated);
        output.push_str(&serde_json::to_string(&event).expect("serialize trace event"));
        output.push('\n');
    }

    fs::write(trace, output).expect("write rewritten trace");
}

fn read_response(reader: &mut BufReader<impl std::io::Read>) {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let _: Value = serde_json::from_str(line.trim()).expect("valid fixture response");
}
