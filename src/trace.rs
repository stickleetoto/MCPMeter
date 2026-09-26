use crate::event::MeasurementEvent;
use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub fn resolve_trace_path(path: &Path, max_bytes: Option<u64>) -> Result<PathBuf> {
    let Some(max_bytes) = max_bytes else {
        return Ok(path.to_path_buf());
    };
    if max_bytes == 0 {
        bail!("--trace-max-bytes must be greater than zero");
    }

    let segments = existing_trace_segments(path)?;
    let Some((latest_index, latest_path)) = segments.last() else {
        return Ok(path.to_path_buf());
    };

    let size = fs::metadata(latest_path)
        .with_context(|| format!("failed to inspect trace segment {}", latest_path.display()))?
        .len();

    if size < max_bytes {
        return Ok(latest_path.clone());
    }

    trace_segment_path(path, latest_index.saturating_add(1))
}

fn existing_trace_segments(path: &Path) -> Result<Vec<(u64, PathBuf)>> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut segments = Vec::new();

    if path.is_file() {
        segments.push((0, path.to_path_buf()));
    }

    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(segments),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect trace directory {}", parent.display()));
        }
    };

    for entry in entries {
        let entry = entry
            .with_context(|| format!("failed to inspect trace directory {}", parent.display()))?;
        let candidate = entry.path();
        if candidate == path || !candidate.is_file() {
            continue;
        }
        if let Some(index) = trace_segment_index(path, &candidate) {
            segments.push((index, candidate));
        }
    }

    segments.sort_by_key(|(index, _)| *index);
    Ok(segments)
}

fn trace_segment_index(base: &Path, candidate: &Path) -> Option<u64> {
    let candidate_name = candidate.file_name()?;
    let extension = base.extension();

    let prefix = match extension {
        Some(_) => {
            let mut prefix = base.file_stem()?.to_os_string();
            prefix.push(".");
            prefix
        }
        None => {
            let mut prefix = base.file_name()?.to_os_string();
            prefix.push(".");
            prefix
        }
    };

    let suffix = extension.map(|extension| {
        let mut suffix = OsString::from(".");
        suffix.push(extension);
        suffix
    });

    let candidate = candidate_name.to_string_lossy();
    let prefix = prefix.to_string_lossy();
    let middle = candidate.strip_prefix(prefix.as_ref())?;
    let middle = match suffix {
        Some(suffix) => middle.strip_suffix(suffix.to_string_lossy().as_ref())?,
        None => middle,
    };

    let index = middle.parse::<u64>().ok()?;
    (index > 0).then_some(index)
}

fn trace_segment_path(base: &Path, index: u64) -> Result<PathBuf> {
    if index == 0 {
        return Ok(base.to_path_buf());
    }

    let file_name = match base.extension() {
        Some(extension) => {
            let mut file_name = base
                .file_stem()
                .context("trace path must include a file name")?
                .to_os_string();
            file_name.push(format!(".{index}."));
            file_name.push(extension);
            file_name
        }
        None => {
            let mut file_name = base
                .file_name()
                .context("trace path must include a file name")?
                .to_os_string();
            file_name.push(format!(".{index}"));
            file_name
        }
    };

    Ok(base.with_file_name(file_name))
}

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
    fn rotates_when_latest_segment_reaches_exact_size_boundary() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mcpmeter-trace-boundary-{}-{unique}.jsonl",
            std::process::id()
        ));
        fs::write(&path, b"12345").unwrap();

        let selected = resolve_trace_path(&path, Some(5)).unwrap();
        assert_eq!(
            selected.file_name().unwrap().to_string_lossy(),
            format!(
                "{}.1.jsonl",
                path.file_stem().unwrap().to_string_lossy()
            )
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn selects_latest_rotated_segment_when_it_is_below_limit() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mcpmeter-trace-latest-{}-{unique}.jsonl",
            std::process::id()
        ));
        let rotated = trace_segment_path(&path, 1).unwrap();
        fs::write(&path, b"12345").unwrap();
        fs::write(&rotated, b"12").unwrap();

        assert_eq!(resolve_trace_path(&path, Some(5)).unwrap(), rotated);

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(rotated);
    }

    #[test]
    fn reopen_appends_to_existing_rotated_segment_without_truncation() {
        use std::io::Write;

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mcpmeter-trace-reopen-{}-{unique}.jsonl",
            std::process::id()
        ));
        let rotated = trace_segment_path(&path, 1).unwrap();
        fs::write(&path, b"12345").unwrap();
        fs::write(&rotated, b"{\"a\":1}\n").unwrap();

        let selected = resolve_trace_path(&path, Some(64)).unwrap();
        assert_eq!(selected, rotated);

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&selected)
            .unwrap();
        file.write_all(b"{\"b\":2}\n").unwrap();
        drop(file);

        assert_eq!(
            fs::read_to_string(&selected).unwrap(),
            "{\"a\":1}\n{\"b\":2}\n"
        );

        let _ = fs::remove_file(path);
        let _ = fs::remove_file(rotated);
    }

    #[test]
    fn zero_rotation_limit_is_rejected() {
        let error = resolve_trace_path(Path::new("trace.jsonl"), Some(0)).unwrap_err();
        assert!(error.to_string().contains("greater than zero"));
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
