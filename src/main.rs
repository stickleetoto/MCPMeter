mod compare;
mod event;
mod export;
mod fixture;
mod observer;
mod proxy;
mod report;
mod runs;
mod token_ledger;
mod tokenizer;
mod tool_cost;
mod trace;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tokenizer::TokenizerProfile;

#[derive(Debug, Parser)]
#[command(
    name = "mcp-meter",
    version,
    about = "Measure token, traffic, and latency overhead of MCP servers"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run an MCP server behind a transparent stdio measurement proxy.
    Proxy {
        /// Append JSONL measurement events to this file.
        #[arg(long, default_value = "mcpmeter.jsonl")]
        trace: PathBuf,

        /// Tokenizer profile: o200k-base, cl100k-base, or bytes4-estimate.
        #[arg(long, default_value = "o200k-base")]
        tokenizer: String,

        /// Persist raw MCP payloads. Dangerous: payloads may contain secrets.
        #[arg(long)]
        capture_payloads: bool,

        /// MCP server command and arguments. Prefer placing `--` before it.
        #[arg(required = true, num_args = 1.., trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },

    /// Summarize a JSONL trace. Defaults to the most recent run in the file.
    Report {
        /// JSONL trace produced by `mcp-meter proxy`.
        trace: PathBuf,

        /// Report a specific run id instead of the most recent run.
        #[arg(long)]
        run_id: Option<String>,

        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Export a run report as a standalone HTML page or CSV file.
    Export {
        /// JSONL trace produced by `mcp-meter proxy`.
        trace: PathBuf,

        /// Export format: html or csv.
        #[arg(long)]
        format: String,

        /// Destination file path.
        #[arg(short, long)]
        output: PathBuf,

        /// Export a specific run id instead of the most recent run.
        #[arg(long)]
        run_id: Option<String>,
    },

    /// Show event-by-event and cumulative serialized token usage.
    Tokens {
        /// JSONL trace produced by `mcp-meter proxy`.
        trace: PathBuf,

        /// Track a specific run id instead of the most recent run.
        #[arg(long)]
        run_id: Option<String>,

        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Show per-tool token, wire, error, and latency costs.
    Tools {
        /// JSONL trace produced by `mcp-meter proxy`.
        trace: PathBuf,

        /// Report a specific run id instead of the most recent run.
        #[arg(long)]
        run_id: Option<String>,

        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// List runs stored in a JSONL trace, newest first.
    Runs {
        /// JSONL trace produced by `mcp-meter proxy`.
        trace: PathBuf,

        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Compare a candidate run against a baseline run.
    Compare {
        /// Baseline JSONL trace.
        baseline: PathBuf,

        /// Candidate JSONL trace.
        candidate: PathBuf,

        /// Select a specific baseline run instead of the newest one.
        #[arg(long)]
        baseline_run_id: Option<String>,

        /// Select a specific candidate run instead of the newest one.
        #[arg(long)]
        candidate_run_id: Option<String>,

        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Run the deterministic built-in MCP fixture server.
    #[command(hide = true)]
    Fixture,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Proxy {
            trace,
            tokenizer,
            capture_payloads,
            command,
        } => {
            let tokenizer = TokenizerProfile::parse(&tokenizer)?;
            let code = proxy::run(proxy::ProxyConfig {
                command,
                trace_path: trace,
                tokenizer,
                capture_payloads,
            })?;
            if code != 0 {
                std::process::exit(code);
            }
        }
        Commands::Report {
            trace,
            run_id,
            json,
        } => {
            let report = report::build_report(&trace, run_id.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                report::print_text(&report);
            }
        }
        Commands::Export {
            trace,
            format,
            output,
            run_id,
        } => {
            let report = report::build_report(&trace, run_id.as_deref())?;
            let format = export::ExportFormat::parse(&format)?;
            export::write_report(&report, format, &output)?;
            eprintln!("MCPMeter report -> {}", output.display());
        }
        Commands::Tokens {
            trace,
            run_id,
            json,
        } => {
            let ledger = token_ledger::build_token_ledger(&trace, run_id.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&ledger)?);
            } else {
                token_ledger::print_text(&ledger);
            }
        }
        Commands::Tools {
            trace,
            run_id,
            json,
        } => {
            let summary = tool_cost::build_tool_cost(&trace, run_id.as_deref())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else {
                tool_cost::print_text(&summary);
            }
        }
        Commands::Runs { trace, json } => {
            let runs = runs::list_runs(&trace)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&runs)?);
            } else {
                runs::print_text(&runs);
            }
        }
        Commands::Compare {
            baseline,
            candidate,
            baseline_run_id,
            candidate_run_id,
            json,
        } => {
            let baseline_report = report::build_report(&baseline, baseline_run_id.as_deref())?;
            let candidate_report = report::build_report(&candidate, candidate_run_id.as_deref())?;
            let comparison = compare::compare_reports(&baseline_report, &candidate_report)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&comparison)?);
            } else {
                compare::print_text(&comparison);
            }
        }
        Commands::Fixture => fixture::run()?,
    }

    Ok(())
}
