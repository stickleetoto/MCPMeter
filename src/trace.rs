use crate::event::MeasurementEvent;
use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn read_events(path: &Path) -> Result<Vec<MeasurementEvent>> {
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

    Ok(events)
}

pub fn select_run<'a>(
    events: &'a [MeasurementEvent],
    requested_run: Option<&str>,
) -> Result<(String, Vec<&'a MeasurementEvent>)> {
    let run_id = match requested_run {
        Some(run) => run.to_string(),
        None => events
            .iter()
            .max_by_key(|event| event.ts_unix_ns)
            .map(|event| event.run_id.clone())
            .context("trace contains no events")?,
    };

    let selected: Vec<&MeasurementEvent> = events
        .iter()
        .filter(|event| event.run_id == run_id)
        .collect();
    if selected.is_empty() {
        anyhow::bail!("run id not found in trace: {run_id}");
    }

    Ok((run_id, selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Direction, TransportKind};

    fn event(run_id: &str, ts: u128) -> MeasurementEvent {
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
            wire_bytes: Some(2),
            payload_bytes: Some(1),
            serialized_tokens: 1,
            tokenizer: "o200k_base".to_string(),
            token_count_estimated: false,
            payload_sha256: "00".repeat(32),
            raw_payload: None,
            methods: Vec::new(),
            tools: Vec::new(),
            request_count: 1,
            response_count: 0,
            notification_count: 0,
            tool_call_count: 0,
            tools_exposed: None,
            schema_tokens: None,
            latencies_us: Vec::new(),
            ok: true,
            parse_error: None,
        }
    }

    #[test]
    fn v1_event_without_transport_defaults_to_stdio() {
        let current = event("legacy", 1);
        let mut value = serde_json::to_value(current).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("transport");
        object.remove("payload_bytes");
        object.insert("schema_version".to_string(), serde_json::json!(1));

        let parsed: MeasurementEvent = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.transport, TransportKind::Stdio);
        assert_eq!(parsed.wire_bytes, Some(2));
        assert_eq!(parsed.payload_bytes, None);
    }

    #[test]
    fn v2_event_without_payload_bytes_remains_readable() {
        let current = event("v2", 1);
        let mut value = serde_json::to_value(current).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("payload_bytes");
        object.insert("schema_version".to_string(), serde_json::json!(2));

        let parsed: MeasurementEvent = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.schema_version, 2);
        assert_eq!(parsed.transport, TransportKind::Stdio);
        assert_eq!(parsed.payload_bytes, None);
    }

    #[test]
    fn serialized_v5_stdio_event_omits_http_routing_metadata() {
        let value = serde_json::to_value(event("current", 1)).unwrap();
        assert_eq!(value["schema_version"], serde_json::json!(5));
        assert_eq!(value["transport"], serde_json::json!("stdio"));
        assert_eq!(value["payload_bytes"], serde_json::json!(1));
        assert!(value.get("http_mcp_protocol_version").is_none());
        assert!(value.get("http_mcp_method").is_none());
        assert!(value.get("http_mcp_name").is_none());
    }

    #[test]
    fn newest_run_is_selected_by_latest_event_timestamp() {
        let events = vec![event("old", 1), event("new", 3), event("old", 2)];
        let (run_id, selected) = select_run(&events, None).unwrap();
        assert_eq!(run_id, "new");
        assert_eq!(selected.len(), 1);
    }

    #[test]
    fn explicit_run_selection_works() {
        let events = vec![event("a", 1), event("b", 2), event("a", 3)];
        let (run_id, selected) = select_run(&events, Some("a")).unwrap();
        assert_eq!(run_id, "a");
        assert_eq!(selected.len(), 2);
    }
}
