use crate::report::RunReport;
use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ComparisonReport {
    pub baseline_run_id: String,
    pub candidate_run_id: String,
    pub tokenizer: String,
    pub token_count_estimated: bool,
    pub serialized_tokens: U64Delta,
    pub wire_bytes: U64Delta,
    pub tool_calls: U64Delta,
    pub error_events: U64Delta,
    pub schema_tokens: Option<U64Delta>,
    pub tools_exposed: Option<U64Delta>,
    pub latency_p50_ms: Option<F64Delta>,
    pub latency_p95_ms: Option<F64Delta>,
    pub latency_p99_ms: Option<F64Delta>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct U64Delta {
    pub baseline: u64,
    pub candidate: u64,
    pub delta: i128,
    pub percent: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct F64Delta {
    pub baseline: f64,
    pub candidate: f64,
    pub delta: f64,
    pub percent: Option<f64>,
}

pub fn compare_reports(baseline: &RunReport, candidate: &RunReport) -> Result<ComparisonReport> {
    if baseline.tokenizer != candidate.tokenizer {
        bail!(
            "cannot compare token metrics from different tokenizers: {} vs {}",
            baseline.tokenizer,
            candidate.tokenizer
        );
    }

    Ok(ComparisonReport {
        baseline_run_id: baseline.run_id.clone(),
        candidate_run_id: candidate.run_id.clone(),
        tokenizer: baseline.tokenizer.clone(),
        token_count_estimated: baseline.token_count_estimated || candidate.token_count_estimated,
        serialized_tokens: delta_u64(
            baseline.serialized_tokens_total,
            candidate.serialized_tokens_total,
        ),
        wire_bytes: delta_u64(baseline.wire_bytes_total, candidate.wire_bytes_total),
        tool_calls: delta_u64(baseline.tool_calls, candidate.tool_calls),
        error_events: delta_u64(baseline.error_events, candidate.error_events),
        schema_tokens: zip_u64(baseline.schema_tokens, candidate.schema_tokens),
        tools_exposed: zip_u64(baseline.tools_exposed, candidate.tools_exposed),
        latency_p50_ms: zip_f64(baseline.latency_p50_ms, candidate.latency_p50_ms),
        latency_p95_ms: zip_f64(baseline.latency_p95_ms, candidate.latency_p95_ms),
        latency_p99_ms: zip_f64(baseline.latency_p99_ms, candidate.latency_p99_ms),
    })
}

pub fn print_text(report: &ComparisonReport) {
    println!("Baseline:  {}", report.baseline_run_id);
    println!("Candidate: {}", report.candidate_run_id);
    println!(
        "Tokenizer: {}{}",
        report.tokenizer,
        if report.token_count_estimated {
            " (estimated)"
        } else {
            ""
        }
    );
    println!();
    println!(
        "{:<22} {:>12} {:>12} {:>12} {:>10}",
        "Metric", "Baseline", "Candidate", "Delta", "Delta %"
    );
    println!("{}", "-".repeat(72));
    print_u64_row("Serialized tokens", &report.serialized_tokens);
    print_u64_row("Wire bytes", &report.wire_bytes);
    print_u64_row("Tool calls", &report.tool_calls);
    print_u64_row("Error events", &report.error_events);
    if let Some(delta) = &report.schema_tokens {
        print_u64_row("Schema tokens", delta);
    }
    if let Some(delta) = &report.tools_exposed {
        print_u64_row("Tools exposed", delta);
    }
    if let Some(delta) = &report.latency_p50_ms {
        print_f64_row("Latency p50 ms", delta);
    }
    if let Some(delta) = &report.latency_p95_ms {
        print_f64_row("Latency p95 ms", delta);
    }
    if let Some(delta) = &report.latency_p99_ms {
        print_f64_row("Latency p99 ms", delta);
    }
}

fn delta_u64(baseline: u64, candidate: u64) -> U64Delta {
    U64Delta {
        baseline,
        candidate,
        delta: i128::from(candidate) - i128::from(baseline),
        percent: percent_delta(baseline as f64, candidate as f64),
    }
}

fn delta_f64(baseline: f64, candidate: f64) -> F64Delta {
    F64Delta {
        baseline,
        candidate,
        delta: candidate - baseline,
        percent: percent_delta(baseline, candidate),
    }
}

fn zip_u64(baseline: Option<u64>, candidate: Option<u64>) -> Option<U64Delta> {
    Some(delta_u64(baseline?, candidate?))
}

fn zip_f64(baseline: Option<f64>, candidate: Option<f64>) -> Option<F64Delta> {
    Some(delta_f64(baseline?, candidate?))
}

fn percent_delta(baseline: f64, candidate: f64) -> Option<f64> {
    if baseline == 0.0 {
        None
    } else {
        Some(((candidate - baseline) / baseline) * 100.0)
    }
}

fn percent_text(percent: Option<f64>) -> String {
    percent
        .map(|value| format!("{value:+.2}%"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn print_u64_row(label: &str, delta: &U64Delta) {
    println!(
        "{label:<22} {:>12} {:>12} {:>+12} {:>10}",
        delta.baseline,
        delta.candidate,
        delta.delta,
        percent_text(delta.percent)
    );
}

fn print_f64_row(label: &str, delta: &F64Delta) {
    println!(
        "{label:<22} {:>12.3} {:>12.3} {:>+12.3} {:>10}",
        delta.baseline,
        delta.candidate,
        delta.delta,
        percent_text(delta.percent)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(run_id: &str, tokenizer: &str, tokens: u64) -> RunReport {
        RunReport {
            run_id: run_id.to_string(),
            tokenizer: tokenizer.to_string(),
            token_count_estimated: false,
            messages: 0,
            requests: 0,
            responses: 0,
            notifications: 0,
            tool_calls: 4,
            unique_tools: Vec::new(),
            tools_exposed: Some(3),
            schema_tokens: Some(100),
            wire_bytes_client_to_server: 0,
            wire_bytes_server_to_client: 0,
            wire_bytes_total: 1_000,
            serialized_tokens_client_to_server: 0,
            serialized_tokens_server_to_client: 0,
            serialized_tokens_total: tokens,
            error_events: 0,
            latency_samples: 1,
            latency_p50_ms: Some(2.0),
            latency_p95_ms: Some(3.0),
            latency_p99_ms: Some(4.0),
            latency_max_ms: Some(4.0),
        }
    }

    #[test]
    fn candidate_delta_is_candidate_minus_baseline() {
        let baseline = report("a", "o200k_base", 1_000);
        let candidate = report("b", "o200k_base", 800);
        let comparison = compare_reports(&baseline, &candidate).unwrap();

        assert_eq!(comparison.serialized_tokens.delta, -200);
        assert_eq!(comparison.serialized_tokens.percent, Some(-20.0));
    }

    #[test]
    fn refuses_mismatched_tokenizers() {
        let baseline = report("a", "o200k_base", 1_000);
        let candidate = report("b", "cl100k_base", 800);
        assert!(compare_reports(&baseline, &candidate).is_err());
    }

    #[test]
    fn zero_baseline_has_no_percent_delta() {
        let delta = delta_u64(0, 5);
        assert_eq!(delta.percent, None);
    }
}
