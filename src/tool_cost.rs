use crate::event::{Direction, MeasurementEvent};
use crate::trace::{read_events, select_run};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ToolCostSummary {
    pub run_id: String,
    pub tokenizer: String,
    pub token_count_estimated: bool,
    pub batch_request_events: u64,
    pub batch_tool_calls: u64,
    pub non_batch_tool_calls: u64,
    pub batch_cost_attribution: String,
    pub tools: Vec<ToolCost>,
    pub unattributed_batch_request_tokens: u64,
    pub unattributed_batch_response_tokens: u64,
    pub unattributed_batch_request_wire_bytes: Option<u64>,
    pub unattributed_batch_response_wire_bytes: Option<u64>,
    pub unattributed_batch_request_payload_bytes: Option<u64>,
    pub unattributed_batch_response_payload_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ToolCost {
    pub tool: String,
    pub calls: u64,
    pub batch_calls: u64,
    pub non_batch_calls: u64,
    pub request_tokens: u64,
    pub response_tokens: u64,
    pub total_tokens: u64,
    pub request_wire_bytes: Option<u64>,
    pub response_wire_bytes: Option<u64>,
    pub total_wire_bytes: Option<u64>,
    pub request_payload_bytes: Option<u64>,
    pub response_payload_bytes: Option<u64>,
    pub total_payload_bytes: Option<u64>,
    pub error_responses: u64,
    pub latency_samples: u64,
    pub latency_p50_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    pub latency_p99_ms: Option<f64>,
}

struct ToolAccumulator {
    calls: u64,
    batch_calls: u64,
    non_batch_calls: u64,
    request_tokens: u64,
    response_tokens: u64,
    request_wire_bytes: Option<u64>,
    response_wire_bytes: Option<u64>,
    request_payload_bytes: Option<u64>,
    response_payload_bytes: Option<u64>,
    error_responses: u64,
    latencies_us: Vec<u64>,
}

impl Default for ToolAccumulator {
    fn default() -> Self {
        Self {
            calls: 0,
            batch_calls: 0,
            non_batch_calls: 0,
            request_tokens: 0,
            response_tokens: 0,
            request_wire_bytes: Some(0),
            response_wire_bytes: Some(0),
            request_payload_bytes: Some(0),
            response_payload_bytes: Some(0),
            error_responses: 0,
            latencies_us: Vec::new(),
        }
    }
}

pub fn build_tool_cost(path: &Path, requested_run: Option<&str>) -> Result<ToolCostSummary> {
    let events = read_events(path)?;
    let (run_id, selected) = select_run(&events, requested_run)?;
    summarize_selected(run_id, &selected)
}

fn summarize_selected(run_id: String, selected: &[&MeasurementEvent]) -> Result<ToolCostSummary> {
    let tokenizer = selected[0].tokenizer.clone();

    if selected.iter().any(|event| event.tokenizer != tokenizer) {
        bail!("run contains mixed tokenizer profiles: {run_id}");
    }

    let token_count_estimated = selected.iter().any(|event| event.token_count_estimated);
    let mut accumulators: BTreeMap<String, ToolAccumulator> = BTreeMap::new();
    let mut batch_request_events = 0;
    let mut batch_tool_calls = 0;
    let mut non_batch_tool_calls = 0;
    let mut batch_req_tokens = 0;
    let mut batch_res_tokens = 0;
    let mut batch_req_wire = Some(0);
    let mut batch_res_wire = Some(0);
    let mut batch_req_payload = Some(0);
    let mut batch_res_payload = Some(0);

    for event in selected {
        match event.direction {
            Direction::ClientToServer if event.tool_call_count > 0 => {
                if event.kind == "batch" {
                    batch_request_events += 1;
                    batch_tool_calls += event.tool_call_count;
                    record_tool_calls(
                        &mut accumulators,
                        &event.tools,
                        event.tool_call_count,
                        true,
                    );

                    if let Some(tool) = attributable_batch_request_tool(event) {
                        let accumulator = accumulators.entry(tool.to_string()).or_default();
                        accumulator.request_tokens += event.serialized_tokens;
                        add_optional(&mut accumulator.request_wire_bytes, event.wire_bytes);
                        add_optional(&mut accumulator.request_payload_bytes, event.payload_bytes);
                    } else {
                        batch_req_tokens += event.serialized_tokens;
                        add_optional(&mut batch_req_wire, event.wire_bytes);
                        add_optional(&mut batch_req_payload, event.payload_bytes);
                    }
                } else if event.tools.len() == 1 {
                    non_batch_tool_calls += event.tool_call_count;
                    let accumulator = accumulators.entry(event.tools[0].clone()).or_default();
                    accumulator.calls += event.tool_call_count;
                    accumulator.non_batch_calls += event.tool_call_count;
                    accumulator.request_tokens += event.serialized_tokens;
                    add_optional(&mut accumulator.request_wire_bytes, event.wire_bytes);
                    add_optional(&mut accumulator.request_payload_bytes, event.payload_bytes);
                } else {
                    non_batch_tool_calls += event.tool_call_count;
                    record_tool_calls(
                        &mut accumulators,
                        &event.tools,
                        event.tool_call_count,
                        false,
                    );
                    batch_req_tokens += event.serialized_tokens;
                    add_optional(&mut batch_req_wire, event.wire_bytes);
                    add_optional(&mut batch_req_payload, event.payload_bytes);
                }
            }
            Direction::ServerToClient if !event.tools.is_empty() => {
                if event.kind == "batch" {
                    if let Some(tool) = attributable_batch_response_tool(event) {
                        let accumulator = accumulators.entry(tool.to_string()).or_default();
                        accumulator.response_tokens += event.serialized_tokens;
                        add_optional(&mut accumulator.response_wire_bytes, event.wire_bytes);
                        add_optional(&mut accumulator.response_payload_bytes, event.payload_bytes);
                        accumulator.error_responses += u64::from(!event.ok);
                        accumulator
                            .latencies_us
                            .extend(event.latencies_us.iter().copied());
                    } else {
                        batch_res_tokens += event.serialized_tokens;
                        add_optional(&mut batch_res_wire, event.wire_bytes);
                        add_optional(&mut batch_res_payload, event.payload_bytes);
                    }
                } else if event.tools.len() == 1 && event.kind == "tools_call_response" {
                    let accumulator = accumulators.entry(event.tools[0].clone()).or_default();
                    accumulator.response_tokens += event.serialized_tokens;
                    add_optional(&mut accumulator.response_wire_bytes, event.wire_bytes);
                    add_optional(&mut accumulator.response_payload_bytes, event.payload_bytes);
                    accumulator.error_responses += u64::from(!event.ok);
                    accumulator
                        .latencies_us
                        .extend(event.latencies_us.iter().copied());
                } else if event.tools.len() != 1 {
                    batch_res_tokens += event.serialized_tokens;
                    add_optional(&mut batch_res_wire, event.wire_bytes);
                    add_optional(&mut batch_res_payload, event.payload_bytes);
                }
            }
            _ => {}
        }
    }

    let tools = accumulators
        .into_iter()
        .map(|(tool, mut accumulator)| {
            accumulator.latencies_us.sort_unstable();
            ToolCost {
                tool,
                calls: accumulator.calls,
                batch_calls: accumulator.batch_calls,
                non_batch_calls: accumulator.non_batch_calls,
                request_tokens: accumulator.request_tokens,
                response_tokens: accumulator.response_tokens,
                total_tokens: accumulator.request_tokens + accumulator.response_tokens,
                request_wire_bytes: accumulator.request_wire_bytes,
                response_wire_bytes: accumulator.response_wire_bytes,
                total_wire_bytes: sum_optional_pair(
                    accumulator.request_wire_bytes,
                    accumulator.response_wire_bytes,
                ),
                request_payload_bytes: accumulator.request_payload_bytes,
                response_payload_bytes: accumulator.response_payload_bytes,
                total_payload_bytes: sum_optional_pair(
                    accumulator.request_payload_bytes,
                    accumulator.response_payload_bytes,
                ),
                error_responses: accumulator.error_responses,
                latency_samples: accumulator.latencies_us.len() as u64,
                latency_p50_ms: percentile_ms(&accumulator.latencies_us, 0.50),
                latency_p95_ms: percentile_ms(&accumulator.latencies_us, 0.95),
                latency_p99_ms: percentile_ms(&accumulator.latencies_us, 0.99),
            }
        })
        .collect();

    Ok(ToolCostSummary {
        run_id,
        tokenizer,
        token_count_estimated,
        batch_request_events,
        batch_tool_calls,
        non_batch_tool_calls,
        batch_cost_attribution: "only fully recorded single-tool batch messages are attributed; shared or mixed batch message cost remains unattributed".to_string(),
        tools,
        unattributed_batch_request_tokens: batch_req_tokens,
        unattributed_batch_response_tokens: batch_res_tokens,
        unattributed_batch_request_wire_bytes: batch_req_wire,
        unattributed_batch_response_wire_bytes: batch_res_wire,
        unattributed_batch_request_payload_bytes: batch_req_payload,
        unattributed_batch_response_payload_bytes: batch_res_payload,
    })
}

pub fn print_text(summary: &ToolCostSummary) {
    println!("Run:       {}", summary.run_id);
    println!(
        "Tokenizer: {}{}",
        summary.tokenizer,
        if summary.token_count_estimated {
            " (estimated)"
        } else {
            ""
        }
    );
    println!();

    if summary.tools.is_empty() {
        println!("No tool calls found.");
    } else {
        println!(
            "{:<24} {:>7} {:>7} {:>7} {:>11} {:>11} {:>11} {:>10} {:>8}",
            "Tool",
            "Calls",
            "Direct",
            "Batch",
            "Req tok",
            "Resp tok",
            "Total tok",
            "p95 ms",
            "Errors"
        );
        println!("{}", "-".repeat(112));
        for tool in &summary.tools {
            println!(
                "{:<24} {:>7} {:>7} {:>7} {:>11} {:>11} {:>11} {:>10} {:>8}",
                tool.tool,
                tool.calls,
                tool.non_batch_calls,
                tool.batch_calls,
                tool.request_tokens,
                tool.response_tokens,
                tool.total_tokens,
                optional_ms(tool.latency_p95_ms),
                tool.error_responses
            );
        }
    }

    if summary.batch_request_events > 0 {
        println!();
        println!(
            "Batch tool selection: {} request event(s), {} tool call(s)",
            summary.batch_request_events, summary.batch_tool_calls
        );
        println!("Batch cost attribution: {}", summary.batch_cost_attribution);
    }

    if summary.unattributed_batch_request_tokens > 0
        || summary.unattributed_batch_response_tokens > 0
    {
        println!();
        println!("Shared/mixed batch cost not attributed to individual tools:");
        println!(
            "  request:  {} tokens / {} payload bytes / {} wire bytes",
            summary.unattributed_batch_request_tokens,
            optional_u64(summary.unattributed_batch_request_payload_bytes),
            optional_u64(summary.unattributed_batch_request_wire_bytes)
        );
        println!(
            "  response: {} tokens / {} payload bytes / {} wire bytes",
            summary.unattributed_batch_response_tokens,
            optional_u64(summary.unattributed_batch_response_payload_bytes),
            optional_u64(summary.unattributed_batch_response_wire_bytes)
        );
    }
}

fn record_tool_calls(
    accumulators: &mut BTreeMap<String, ToolAccumulator>,
    tools: &[String],
    tool_call_count: u64,
    batch: bool,
) {
    if tools.len() == 1 {
        let accumulator = accumulators.entry(tools[0].clone()).or_default();
        accumulator.calls += tool_call_count;
        if batch {
            accumulator.batch_calls += tool_call_count;
        } else {
            accumulator.non_batch_calls += tool_call_count;
        }
        return;
    }

    for tool in tools {
        let accumulator = accumulators.entry(tool.clone()).or_default();
        accumulator.calls += 1;
        if batch {
            accumulator.batch_calls += 1;
        } else {
            accumulator.non_batch_calls += 1;
        }
    }
}

fn single_recorded_tool(tools: &[String]) -> Option<&str> {
    let first = tools.first()?;
    tools
        .iter()
        .all(|tool| tool == first)
        .then_some(first.as_str())
}

fn attributable_batch_request_tool(event: &MeasurementEvent) -> Option<&str> {
    if event.request_count != event.tool_call_count
        || event.response_count != 0
        || event.notification_count != 0
    {
        return None;
    }
    single_recorded_tool(&event.tools)
}

fn attributable_batch_response_tool(event: &MeasurementEvent) -> Option<&str> {
    if event.request_count != 0
        || event.notification_count != 0
        || event.response_count != event.tools.len() as u64
    {
        return None;
    }
    single_recorded_tool(&event.tools)
}

fn add_optional(total: &mut Option<u64>, value: Option<u64>) {
    *total = match (*total, value) {
        (Some(total), Some(value)) => Some(total.saturating_add(value)),
        _ => None,
    };
}

fn sum_optional_pair(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    Some(left?.saturating_add(right?))
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn optional_ms(value: Option<f64>) -> String {
    value
        .map(|milliseconds| format!("{milliseconds:.3}"))
        .unwrap_or_else(|| "n/a".to_string())
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
    use crate::event::TransportKind;

    fn event(
        direction: Direction,
        kind: &str,
        tools: &[&str],
        tool_call_count: u64,
        tokens: u64,
        wire_bytes: Option<u64>,
        payload_bytes: Option<u64>,
    ) -> MeasurementEvent {
        MeasurementEvent {
            schema_version: 5,
            run_id: "run".to_string(),
            ts_unix_ns: 1,
            transport: TransportKind::Stdio,
            http_mcp_protocol_version: None,
            http_mcp_method: None,
            http_mcp_name: None,
            direction,
            kind: kind.to_string(),
            wire_bytes,
            payload_bytes,
            serialized_tokens: tokens,
            tokenizer: "o200k_base".to_string(),
            token_count_estimated: false,
            payload_sha256: "00".repeat(32),
            raw_payload: None,
            methods: Vec::new(),
            tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
            request_count: if matches!(direction, Direction::ClientToServer) {
                tool_call_count.max(1)
            } else {
                0
            },
            response_count: if matches!(direction, Direction::ServerToClient) {
                tools.len().max(1) as u64
            } else {
                0
            },
            notification_count: 0,
            tool_call_count,
            tools_exposed: None,
            schema_tokens: None,
            latencies_us: if matches!(direction, Direction::ServerToClient) {
                vec![2_000]
            } else {
                Vec::new()
            },
            ok: true,
            parse_error: None,
        }
    }

    #[test]
    fn attributes_non_batch_request_and_response_to_tool() {
        let request = event(
            Direction::ClientToServer,
            "tools_call_request",
            &["add"],
            1,
            10,
            Some(40),
            Some(39),
        );
        let response = event(
            Direction::ServerToClient,
            "tools_call_response",
            &["add"],
            0,
            6,
            Some(24),
            Some(23),
        );
        let summary = summarize_selected("run".to_string(), &[&request, &response]).unwrap();
        let add = &summary.tools[0];
        assert_eq!(add.tool, "add");
        assert_eq!(add.calls, 1);
        assert_eq!(add.batch_calls, 0);
        assert_eq!(add.non_batch_calls, 1);
        assert_eq!(add.total_tokens, 16);
        assert_eq!(add.total_wire_bytes, Some(64));
        assert_eq!(add.total_payload_bytes, Some(62));
        assert_eq!(add.latency_p95_ms, Some(2.0));
    }

    #[test]
    fn unavailable_wire_bytes_do_not_become_zero() {
        let request = event(
            Direction::ClientToServer,
            "tools_call_request",
            &["add"],
            1,
            10,
            None,
            Some(40),
        );
        let response = event(
            Direction::ServerToClient,
            "tools_call_response",
            &["add"],
            0,
            6,
            None,
            Some(24),
        );
        let summary = summarize_selected("run".to_string(), &[&request, &response]).unwrap();
        let add = &summary.tools[0];
        assert_eq!(add.total_wire_bytes, None);
        assert_eq!(add.total_payload_bytes, Some(64));
    }

    #[test]
    fn batch_tokens_remain_unattributed() {
        let batch = event(
            Direction::ClientToServer,
            "batch",
            &["a", "b"],
            2,
            100,
            Some(400),
            Some(399),
        );
        let summary = summarize_selected("run".to_string(), &[&batch]).unwrap();
        assert_eq!(summary.tools.len(), 2);
        assert_eq!(summary.batch_request_events, 1);
        assert_eq!(summary.batch_tool_calls, 2);
        assert_eq!(summary.tools[0].calls, 1);
        assert_eq!(summary.tools[0].batch_calls, 1);
        assert_eq!(summary.tools[1].calls, 1);
        assert_eq!(summary.tools[1].batch_calls, 1);
        assert_eq!(summary.unattributed_batch_request_tokens, 100);
        assert_eq!(summary.tools[0].request_tokens, 0);
    }

    #[test]
    fn same_tool_batch_cost_is_attributed_when_evidence_is_complete() {
        let request = event(
            Direction::ClientToServer,
            "batch",
            &["add", "add"],
            2,
            100,
            Some(400),
            Some(399),
        );
        let response = event(
            Direction::ServerToClient,
            "batch",
            &["add", "add"],
            0,
            60,
            Some(240),
            Some(239),
        );
        let summary = summarize_selected("run".to_string(), &[&request, &response]).unwrap();
        let add = &summary.tools[0];

        assert_eq!(summary.batch_request_events, 1);
        assert_eq!(summary.batch_tool_calls, 2);
        assert_eq!(add.calls, 2);
        assert_eq!(add.batch_calls, 2);
        assert_eq!(add.non_batch_calls, 0);
        assert_eq!(add.total_tokens, 160);
        assert_eq!(add.total_wire_bytes, Some(640));
        assert_eq!(add.total_payload_bytes, Some(638));
        assert_eq!(summary.unattributed_batch_request_tokens, 0);
        assert_eq!(summary.unattributed_batch_response_tokens, 0);
    }

    #[test]
    fn batch_with_non_tool_work_keeps_shared_cost_unattributed() {
        let mut request = event(
            Direction::ClientToServer,
            "batch",
            &["add"],
            1,
            100,
            Some(400),
            Some(399),
        );
        request.request_count = 2;

        let summary = summarize_selected("run".to_string(), &[&request]).unwrap();
        let add = &summary.tools[0];

        assert_eq!(add.calls, 1);
        assert_eq!(add.batch_calls, 1);
        assert_eq!(add.request_tokens, 0);
        assert_eq!(summary.unattributed_batch_request_tokens, 100);
    }
}
