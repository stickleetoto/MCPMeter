use crate::event::MeasurementEvent;
use anyhow::{Context, Result};
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RunIndexEntry {
    pub run_id: String,
    pub tokenizer: String,
    pub token_count_estimated: bool,
    pub first_ts_unix_ns: u128,
    pub last_ts_unix_ns: u128,
    pub messages: u64,
    pub wire_bytes: Option<u64>,
    pub payload_bytes: Option<u64>,
    pub serialized_tokens: u64,
    pub tool_calls: u64,
    pub error_events: u64,
}

pub fn list_runs(path: &Path) -> Result<Vec<RunIndexEntry>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("failed reading line {}", index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: MeasurementEvent = serde_json::from_str(&line)
            .with_context(|| format!("invalid trace event on line {}", index + 1))?;
        events.push(event);
    }

    Ok(aggregate_runs(&events))
}

pub fn print_text(runs: &[RunIndexEntry]) {
    if runs.is_empty() {
        println!("No runs found.");
        return;
    }

    println!(
        "{:<30} {:<18} {:>8} {:>12} {:>12} {:>12} {:>8} {:>8}",
        "Run ID", "Tokenizer", "Messages", "Tokens", "Payload", "Wire", "Calls", "Errors"
    );
    println!("{}", "-".repeat(119));
    for run in runs {
        let tokenizer = if run.token_count_estimated {
            format!("{}*", run.tokenizer)
        } else {
            run.tokenizer.clone()
        };
        println!(
            "{:<30} {:<18} {:>8} {:>12} {:>12} {:>12} {:>8} {:>8}",
            run.run_id,
            tokenizer,
            run.messages,
            run.serialized_tokens,
            optional_u64(run.payload_bytes),
            optional_u64(run.wire_bytes),
            run.tool_calls,
            run.error_events
        );
    }
    println!("\n* token count is estimated");
}

fn aggregate_runs(events: &[MeasurementEvent]) -> Vec<RunIndexEntry> {
    let mut runs: BTreeMap<String, RunIndexEntry> = BTreeMap::new();

    for event in events {
        let entry = runs
            .entry(event.run_id.clone())
            .or_insert_with(|| RunIndexEntry {
                run_id: event.run_id.clone(),
                tokenizer: event.tokenizer.clone(),
                token_count_estimated: event.token_count_estimated,
                first_ts_unix_ns: event.ts_unix_ns,
                last_ts_unix_ns: event.ts_unix_ns,
                messages: 0,
                wire_bytes: Some(0),
                payload_bytes: Some(0),
                serialized_tokens: 0,
                tool_calls: 0,
                error_events: 0,
            });

        entry.first_ts_unix_ns = entry.first_ts_unix_ns.min(event.ts_unix_ns);
        entry.last_ts_unix_ns = entry.last_ts_unix_ns.max(event.ts_unix_ns);
        entry.messages += 1;
        add_optional(&mut entry.wire_bytes, event.wire_bytes);
        add_optional(&mut entry.payload_bytes, event.payload_bytes);
        entry.serialized_tokens += event.serialized_tokens;
        entry.tool_calls += event.tool_call_count;
        entry.error_events += u64::from(!event.ok);
        entry.token_count_estimated |= event.token_count_estimated;
        if entry.tokenizer != event.tokenizer {
            entry.tokenizer = "mixed".to_string();
            entry.token_count_estimated = true;
        }
    }

    let mut result: Vec<RunIndexEntry> = runs.into_values().collect();
    result.sort_by_key(|entry| Reverse(entry.last_ts_unix_ns));
    result
}

fn add_optional(total: &mut Option<u64>, value: Option<u64>) {
    *total = match (*total, value) {
        (Some(total), Some(value)) => Some(total.saturating_add(value)),
        _ => None,
    };
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Direction, TransportKind};

    fn event(run_id: &str, ts: u128, tokens: u64) -> MeasurementEvent {
        MeasurementEvent {
            schema_version: 5,
            run_id: run_id.to_string(),
            ts_unix_ns: ts,
            transport: TransportKind::Stdio,
            http_mcp_protocol_version: None,
            http_mcp_method: None,
            http_mcp_name: None,
            direction: Direction::ClientToServer,
            kind: "request".to_string(),
            wire_bytes: Some(10),
            payload_bytes: Some(9),
            serialized_tokens: tokens,
            tokenizer: "o200k_base".to_string(),
            token_count_estimated: false,
            payload_sha256: "00".repeat(32),
            raw_payload: None,
            methods: Vec::new(),
            tools: Vec::new(),
            request_count: 1,
            response_count: 0,
            notification_count: 0,
            tool_call_count: 1,
            tools_exposed: None,
            schema_tokens: None,
            latencies_us: Vec::new(),
            ok: true,
            parse_error: None,
        }
    }

    #[test]
    fn aggregates_and_orders_runs_by_latest_event() {
        let events = vec![event("old", 1, 3), event("new", 3, 7), event("old", 2, 5)];
        let runs = aggregate_runs(&events);

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].run_id, "new");
        assert_eq!(runs[1].run_id, "old");
        assert_eq!(runs[1].serialized_tokens, 8);
        assert_eq!(runs[1].wire_bytes, Some(20));
        assert_eq!(runs[1].payload_bytes, Some(18));
        assert_eq!(runs[1].messages, 2);
    }

    #[test]
    fn unavailable_metric_stays_unavailable() {
        let first = event("run", 1, 1);
        let mut second = event("run", 2, 1);
        second.wire_bytes = None;
        let runs = aggregate_runs(&[first, second]);
        assert_eq!(runs[0].wire_bytes, None);
        assert_eq!(runs[0].payload_bytes, Some(18));
    }
}
