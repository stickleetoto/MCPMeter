use crate::event::{Direction, MeasurementEvent};
use crate::trace::{read_events, select_run};
use anyhow::{bail, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct TokenLedger {
    pub run_id: String,
    pub tokenizer: String,
    pub token_count_estimated: bool,
    pub serialized_tokens_client_to_server: u64,
    pub serialized_tokens_server_to_client: u64,
    pub serialized_tokens_total: u64,
    pub schema_tokens_observed: u64,
    pub entries: Vec<TokenLedgerEntry>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct TokenLedgerEntry {
    pub ts_unix_ns: u128,
    pub elapsed_us: u128,
    pub direction: Direction,
    pub kind: String,
    pub methods: Vec<String>,
    pub tools: Vec<String>,
    pub event_tokens: u64,
    pub cumulative_tokens_client_to_server: u64,
    pub cumulative_tokens_server_to_client: u64,
    pub cumulative_tokens_total: u64,
    pub schema_tokens: Option<u64>,
}

pub fn build_token_ledger(path: &Path, requested_run: Option<&str>) -> Result<TokenLedger> {
    let events = read_events(path)?;
    let (run_id, selected) = select_run(&events, requested_run)?;
    build_from_selected(run_id, selected)
}

fn build_from_selected(
    run_id: String,
    mut selected: Vec<&MeasurementEvent>,
) -> Result<TokenLedger> {
    let tokenizer = selected
        .first()
        .map(|event| event.tokenizer.clone())
        .ok_or_else(|| anyhow::anyhow!("selected run contains no events: {run_id}"))?;

    if selected.iter().any(|event| event.tokenizer != tokenizer) {
        bail!("run contains mixed tokenizer profiles: {run_id}");
    }

    selected.sort_by_key(|event| event.ts_unix_ns);
    let first_ts = selected[0].ts_unix_ns;
    let token_count_estimated = selected.iter().any(|event| event.token_count_estimated);

    let mut c2s = 0_u64;
    let mut s2c = 0_u64;
    let mut schema_tokens_observed = 0_u64;
    let mut entries = Vec::with_capacity(selected.len());

    for event in selected {
        match event.direction {
            Direction::ClientToServer => c2s = c2s.saturating_add(event.serialized_tokens),
            Direction::ServerToClient => s2c = s2c.saturating_add(event.serialized_tokens),
        }
        schema_tokens_observed =
            schema_tokens_observed.saturating_add(event.schema_tokens.unwrap_or(0));

        entries.push(TokenLedgerEntry {
            ts_unix_ns: event.ts_unix_ns,
            elapsed_us: event.ts_unix_ns.saturating_sub(first_ts) / 1_000,
            direction: event.direction,
            kind: event.kind.clone(),
            methods: event.methods.clone(),
            tools: event.tools.clone(),
            event_tokens: event.serialized_tokens,
            cumulative_tokens_client_to_server: c2s,
            cumulative_tokens_server_to_client: s2c,
            cumulative_tokens_total: c2s.saturating_add(s2c),
            schema_tokens: event.schema_tokens,
        });
    }

    Ok(TokenLedger {
        run_id,
        tokenizer,
        token_count_estimated,
        serialized_tokens_client_to_server: c2s,
        serialized_tokens_server_to_client: s2c,
        serialized_tokens_total: c2s.saturating_add(s2c),
        schema_tokens_observed,
        entries,
    })
}

pub fn print_text(ledger: &TokenLedger) {
    println!("Run:       {}", ledger.run_id);
    println!(
        "Tokenizer: {}{}",
        ledger.tokenizer,
        if ledger.token_count_estimated {
            " (estimated)"
        } else {
            ""
        }
    );
    println!(
        "Tokens:    {} C→S / {} S→C / {} total",
        ledger.serialized_tokens_client_to_server,
        ledger.serialized_tokens_server_to_client,
        ledger.serialized_tokens_total
    );
    if ledger.schema_tokens_observed > 0 {
        println!(
            "Schema:    {} observed tokens",
            ledger.schema_tokens_observed
        );
    }
    println!();
    println!(
        "{:>10} {:<4} {:<22} {:<28} {:>10} {:>12}",
        "+ms", "Dir", "Kind", "Method / tool", "Event tok", "Cumulative"
    );
    println!("{}", "-".repeat(94));

    for entry in &ledger.entries {
        let label = entry_label(entry);
        println!(
            "{:>10.3} {:<4} {:<22} {:<28} {:>10} {:>12}",
            entry.elapsed_us as f64 / 1000.0,
            direction_text(entry.direction),
            truncate(&entry.kind, 22),
            truncate(&label, 28),
            entry.event_tokens,
            entry.cumulative_tokens_total
        );
    }
}

fn entry_label(entry: &TokenLedgerEntry) -> String {
    if !entry.tools.is_empty() {
        return format!("tool:{}", entry.tools.join(","));
    }
    if !entry.methods.is_empty() {
        return entry.methods.join(",");
    }
    "-".to_string()
}

fn direction_text(direction: Direction) -> &'static str {
    match direction {
        Direction::ClientToServer => "C→S",
        Direction::ServerToClient => "S→C",
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    let mut out: String = value.chars().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::TransportKind;

    fn event(
        ts_unix_ns: u128,
        direction: Direction,
        tokens: u64,
        method: Option<&str>,
        tool: Option<&str>,
        schema_tokens: Option<u64>,
    ) -> MeasurementEvent {
        MeasurementEvent {
            schema_version: 5,
            run_id: "run".to_string(),
            ts_unix_ns,
            transport: TransportKind::Stdio,
            http_mcp_protocol_version: None,
            http_mcp_method: None,
            http_mcp_name: None,
            direction,
            kind: "message".to_string(),
            wire_bytes: Some(2),
            payload_bytes: Some(1),
            serialized_tokens: tokens,
            tokenizer: "o200k_base".to_string(),
            token_count_estimated: false,
            payload_sha256: "00".repeat(32),
            raw_payload: None,
            methods: method.into_iter().map(ToOwned::to_owned).collect(),
            tools: tool.into_iter().map(ToOwned::to_owned).collect(),
            request_count: 0,
            response_count: 0,
            notification_count: 0,
            tool_call_count: u64::from(tool.is_some()),
            tools_exposed: None,
            schema_tokens,
            latencies_us: Vec::new(),
            ok: true,
            parse_error: None,
        }
    }

    #[test]
    fn ledger_sorts_events_and_tracks_directional_cumulative_tokens() {
        let second = event(
            2_000_000,
            Direction::ServerToClient,
            7,
            Some("tools/call"),
            Some("echo"),
            None,
        );
        let first = event(
            1_000_000,
            Direction::ClientToServer,
            5,
            Some("tools/call"),
            Some("echo"),
            None,
        );
        let ledger = build_from_selected("run".to_string(), vec![&second, &first]).unwrap();

        assert_eq!(ledger.entries[0].event_tokens, 5);
        assert_eq!(ledger.entries[0].cumulative_tokens_total, 5);
        assert_eq!(ledger.entries[1].cumulative_tokens_client_to_server, 5);
        assert_eq!(ledger.entries[1].cumulative_tokens_server_to_client, 7);
        assert_eq!(ledger.entries[1].cumulative_tokens_total, 12);
        assert_eq!(ledger.serialized_tokens_total, 12);
    }

    #[test]
    fn ledger_sums_observed_schema_tokens_separately() {
        let first = event(
            1,
            Direction::ServerToClient,
            10,
            Some("tools/list"),
            None,
            Some(42),
        );
        let ledger = build_from_selected("run".to_string(), vec![&first]).unwrap();

        assert_eq!(ledger.schema_tokens_observed, 42);
        assert_eq!(ledger.serialized_tokens_total, 10);
    }

    #[test]
    fn rejects_mixed_tokenizers() {
        let first = event(1, Direction::ClientToServer, 1, None, None, None);
        let mut second = event(2, Direction::ServerToClient, 1, None, None, None);
        second.tokenizer = "cl100k_base".to_string();

        assert!(build_from_selected("run".to_string(), vec![&first, &second]).is_err());
    }

    #[test]
    fn truncation_is_unicode_safe() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("가나다라마바사", 4), "가나다…");
    }
}
