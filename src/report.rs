use crate::event::Direction;
use crate::trace::{read_events, select_run};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct RunReport {
    pub run_id: String,
    pub tokenizer: String,
    pub token_count_estimated: bool,
    pub messages: u64,
    pub requests: u64,
    pub responses: u64,
    pub notifications: u64,
    pub tool_calls: u64,
    pub unique_tools: Vec<String>,
    pub tools_exposed: Option<u64>,
    pub schema_tokens: Option<u64>,
    pub wire_bytes_client_to_server: u64,
    pub wire_bytes_server_to_client: u64,
    pub wire_bytes_total: u64,
    pub serialized_tokens_client_to_server: u64,
    pub serialized_tokens_server_to_client: u64,
    pub serialized_tokens_total: u64,
    pub error_events: u64,
    pub latency_samples: u64,
    pub latency_p50_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    pub latency_p99_ms: Option<f64>,
    pub latency_max_ms: Option<f64>,
}

pub fn build_report(path: &Path, requested_run: Option<&str>) -> Result<RunReport> {
    let events = read_events(path)?;
    let (run_id, selected) = select_run(&events, requested_run)?;

    let tokenizer = selected[0].tokenizer.clone();
    let token_count_estimated = selected.iter().any(|event| event.token_count_estimated);
    let mut unique_tools = BTreeSet::new();
    let mut latencies_us = Vec::new();
    let mut report = RunReport {
        run_id,
        tokenizer,
        token_count_estimated,
        messages: 0,
        requests: 0,
        responses: 0,
        notifications: 0,
        tool_calls: 0,
        unique_tools: Vec::new(),
        tools_exposed: None,
        schema_tokens: None,
        wire_bytes_client_to_server: 0,
        wire_bytes_server_to_client: 0,
        wire_bytes_total: 0,
        serialized_tokens_client_to_server: 0,
        serialized_tokens_server_to_client: 0,
        serialized_tokens_total: 0,
        error_events: 0,
        latency_samples: 0,
        latency_p50_ms: None,
        latency_p95_ms: None,
        latency_p99_ms: None,
        latency_max_ms: None,
    };

    for event in selected {
        report.messages += 1;
        report.requests += event.request_count;
        report.responses += event.response_count;
        report.notifications += event.notification_count;
        report.tool_calls += event.tool_call_count;
        report.wire_bytes_total += event.wire_bytes;
        report.serialized_tokens_total += event.serialized_tokens;
        if !event.ok {
            report.error_events += 1;
        }

        match event.direction {
            Direction::ClientToServer => {
                report.wire_bytes_client_to_server += event.wire_bytes;
                report.serialized_tokens_client_to_server += event.serialized_tokens;
            }
            Direction::ServerToClient => {
                report.wire_bytes_server_to_client += event.wire_bytes;
                report.serialized_tokens_server_to_client += event.serialized_tokens;
            }
        }

        for tool in &event.tools {
            unique_tools.insert(tool.clone());
        }
        if let Some(count) = event.tools_exposed {
            report.tools_exposed = Some(count);
        }
        if let Some(tokens) = event.schema_tokens {
            report.schema_tokens = Some(tokens);
        }
        latencies_us.extend(event.latencies_us.iter().copied());
    }

    latencies_us.sort_unstable();
    report.latency_samples = latencies_us.len() as u64;
    report.latency_p50_ms = percentile_ms(&latencies_us, 0.50);
    report.latency_p95_ms = percentile_ms(&latencies_us, 0.95);
    report.latency_p99_ms = percentile_ms(&latencies_us, 0.99);
    report.latency_max_ms = latencies_us.last().map(|v| *v as f64 / 1000.0);
    report.unique_tools = unique_tools.into_iter().collect();

    Ok(report)
}

pub fn print_text(report: &RunReport) {
    println!("Run:                 {}", report.run_id);
    println!(
        "Tokenizer:           {}{}",
        report.tokenizer,
        if report.token_count_estimated {
            " (estimated)"
        } else {
            ""
        }
    );
    println!("Messages:            {}", report.messages);
    println!(
        "Requests/responses:  {} / {}",
        report.requests, report.responses
    );
    println!("Notifications:       {}", report.notifications);
    println!("Tool calls:          {}", report.tool_calls);
    println!("Unique tools called: {}", report.unique_tools.len());
    if !report.unique_tools.is_empty() {
        println!("Tools:               {}", report.unique_tools.join(", "));
    }
    if let Some(count) = report.tools_exposed {
        println!("Tools exposed:       {count}");
    }
    if let Some(tokens) = report.schema_tokens {
        println!("Schema tokens:       {tokens}");
    }
    println!(
        "Wire bytes C→S/S→C:  {} / {}",
        report.wire_bytes_client_to_server, report.wire_bytes_server_to_client
    );
    println!("Wire bytes total:    {}", report.wire_bytes_total);
    println!(
        "Tokens C→S/S→C:      {} / {}",
        report.serialized_tokens_client_to_server, report.serialized_tokens_server_to_client
    );
    println!("Serialized tokens:   {}", report.serialized_tokens_total);
    println!("Error events:        {}", report.error_events);
    if let Some(p50) = report.latency_p50_ms {
        println!("Latency p50:         {p50:.3} ms");
        println!(
            "Latency p95:         {:.3} ms",
            report.latency_p95_ms.unwrap_or(p50)
        );
        println!(
            "Latency p99:         {:.3} ms",
            report.latency_p99_ms.unwrap_or(p50)
        );
        println!(
            "Latency max:         {:.3} ms",
            report.latency_max_ms.unwrap_or(p50)
        );
    } else {
        println!("Latency:             no correlated samples");
    }
}

fn percentile_ms(sorted_us: &[u64], percentile: f64) -> Option<f64> {
    if sorted_us.is_empty() {
        return None;
    }
    let rank = (percentile * sorted_us.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted_us.len() - 1);
    Some(sorted_us[index] as f64 / 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles_are_stable() {
        let values = vec![1_000, 2_000, 3_000, 4_000, 5_000];
        assert_eq!(percentile_ms(&values, 0.50), Some(3.0));
        assert_eq!(percentile_ms(&values, 0.95), Some(5.0));
    }
}
