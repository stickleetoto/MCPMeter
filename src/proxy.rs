use crate::event::{Direction, MeasurementEvent};
use crate::observer::{observe_payload_at, unix_now_ns, PendingMap};
use crate::tokenizer::TokenizerProfile;
use anyhow::{bail, Context, Result};
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

pub struct ProxyConfig {
    pub command: Vec<String>,
    pub trace_path: PathBuf,
    pub tokenizer: TokenizerProfile,
    pub capture_payloads: bool,
}

struct ObservedFrame {
    payload: Vec<u8>,
    direction: Direction,
    observed_at: Instant,
    ts_unix_ns: u128,
}

enum ObserverMessage {
    Frame(ObservedFrame),
    Shutdown(Sender<()>),
}

pub fn run(config: ProxyConfig) -> Result<i32> {
    if config.command.is_empty() {
        bail!("missing MCP server command after --");
    }

    if config.capture_payloads {
        eprintln!(
            "MCPMeter WARNING: --capture-payloads stores raw MCP traffic and may persist secrets or private data"
        );
    }

    if let Some(parent) = config.trace_path.parent() {
        if !parent.as_os_str().is_empty() {
            create_dir_all(parent).with_context(|| {
                format!("failed to create trace directory {}", parent.display())
            })?;
        }
    }

    let trace_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.trace_path)
        .with_context(|| format!("failed to open trace file {}", config.trace_path.display()))?;

    let run_id = format!("{}-{}", unix_now_ns(), std::process::id());
    let (observer_tx, observer_rx) = mpsc::channel::<ObserverMessage>();
    let observer_run_id = run_id.clone();
    let observer_tokenizer = config.tokenizer.clone();
    let capture_payloads = config.capture_payloads;

    let observer_thread = thread::spawn(move || {
        observer_loop(
            observer_rx,
            trace_file,
            observer_run_id,
            observer_tokenizer,
            capture_payloads,
        )
    });

    let mut child = Command::new(&config.command[0])
        .args(&config.command[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn MCP server: {}", config.command.join(" ")))?;

    let mut child_stdin = child
        .stdin
        .take()
        .context("failed to capture child stdin")?;
    let child_stdout = child
        .stdout
        .take()
        .context("failed to capture child stdout")?;

    let client_observer_tx = observer_tx.clone();
    let client_thread = thread::spawn(move || -> Result<()> {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let mut buffer = Vec::with_capacity(4096);

        loop {
            buffer.clear();
            let bytes = reader.read_until(b'\n', &mut buffer)?;
            if bytes == 0 {
                break;
            }

            let frame = ObservedFrame {
                payload: buffer.clone(),
                direction: Direction::ClientToServer,
                observed_at: Instant::now(),
                ts_unix_ns: unix_now_ns(),
            };
            let _ = client_observer_tx.send(ObserverMessage::Frame(frame));

            child_stdin.write_all(&buffer)?;
            child_stdin.flush()?;
        }

        Ok(())
    });

    let stdout = io::stdout();
    let mut stdout_lock = stdout.lock();
    let mut server_reader = BufReader::new(child_stdout);
    let mut buffer = Vec::with_capacity(4096);

    loop {
        buffer.clear();
        let bytes = server_reader.read_until(b'\n', &mut buffer)?;
        if bytes == 0 {
            break;
        }

        let frame = ObservedFrame {
            payload: buffer.clone(),
            direction: Direction::ServerToClient,
            observed_at: Instant::now(),
            ts_unix_ns: unix_now_ns(),
        };
        let _ = observer_tx.send(ObserverMessage::Frame(frame));

        stdout_lock.write_all(&buffer)?;
        stdout_lock.flush()?;
    }

    let status = child.wait().context("failed waiting for MCP server")?;

    if client_thread.is_finished() {
        if let Err(join_error) = client_thread.join() {
            eprintln!("MCPMeter warning: client forwarding thread panicked: {join_error:?}");
        }
    }

    let (ack_tx, ack_rx) = mpsc::channel();
    if observer_tx.send(ObserverMessage::Shutdown(ack_tx)).is_ok() {
        let _ = ack_rx.recv();
    }
    drop(observer_tx);

    match observer_thread.join() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => eprintln!("MCPMeter warning: observer failed: {error:#}"),
        Err(error) => eprintln!("MCPMeter warning: observer thread panicked: {error:?}"),
    }

    eprintln!("MCPMeter run {run_id} -> {}", config.trace_path.display());

    Ok(status.code().unwrap_or(1))
}

fn observer_loop(
    receiver: Receiver<ObserverMessage>,
    trace_file: File,
    run_id: String,
    tokenizer: TokenizerProfile,
    capture_payloads: bool,
) -> Result<()> {
    let mut writer = BufWriter::new(trace_file);
    let mut pending = PendingMap::new();

    while let Ok(message) = receiver.recv() {
        match message {
            ObserverMessage::Frame(frame) => {
                let event = observe_payload_at(
                    &frame.payload,
                    frame.direction,
                    &run_id,
                    &tokenizer,
                    capture_payloads,
                    &mut pending,
                    frame.observed_at,
                    frame.ts_unix_ns,
                );
                if let Err(error) = write_event(&mut writer, &event) {
                    eprintln!("MCPMeter warning: failed to write trace event: {error:#}");
                }
            }
            ObserverMessage::Shutdown(ack) => {
                writer.flush()?;
                let _ = ack.send(());
                return Ok(());
            }
        }
    }

    writer.flush()?;
    Ok(())
}

fn write_event(writer: &mut BufWriter<File>, event: &MeasurementEvent) -> Result<()> {
    serde_json::to_writer(&mut *writer, event)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}
