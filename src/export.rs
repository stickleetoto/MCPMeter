use crate::report::RunReport;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Html,
    Csv,
}

impl ExportFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "html" => Ok(Self::Html),
            "csv" => Ok(Self::Csv),
            other => bail!("unsupported export format: {other}; expected html or csv"),
        }
    }
}

pub fn write_report(report: &RunReport, format: ExportFormat, output: &Path) -> Result<()> {
    let rendered = match format {
        ExportFormat::Html => render_html(report),
        ExportFormat::Csv => render_csv(report),
    };

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create export directory {}", parent.display())
            })?;
        }
    }

    fs::write(output, rendered)
        .with_context(|| format!("failed to write report export {}", output.display()))?;
    Ok(())
}

pub fn render_csv(report: &RunReport) -> String {
    let mut out = String::from("metric,value\n");
    csv_row(&mut out, "run_id", &report.run_id);
    csv_row(&mut out, "tokenizer", &report.tokenizer);
    csv_row(
        &mut out,
        "token_count_estimated",
        &report.token_count_estimated.to_string(),
    );
    csv_row(&mut out, "messages", &report.messages.to_string());
    csv_row(&mut out, "requests", &report.requests.to_string());
    csv_row(&mut out, "responses", &report.responses.to_string());
    csv_row(
        &mut out,
        "notifications",
        &report.notifications.to_string(),
    );
    csv_row(&mut out, "tool_calls", &report.tool_calls.to_string());
    csv_row(
        &mut out,
        "unique_tools_called",
        &report.unique_tools.len().to_string(),
    );
    csv_row(&mut out, "tools", &report.unique_tools.join(","));
    optional_u64_row(&mut out, "tools_exposed", report.tools_exposed);
    optional_u64_row(&mut out, "schema_tokens", report.schema_tokens);
    csv_row(
        &mut out,
        "wire_bytes_client_to_server",
        &report.wire_bytes_client_to_server.to_string(),
    );
    csv_row(
        &mut out,
        "wire_bytes_server_to_client",
        &report.wire_bytes_server_to_client.to_string(),
    );
    csv_row(
        &mut out,
        "wire_bytes_total",
        &report.wire_bytes_total.to_string(),
    );
    csv_row(
        &mut out,
        "serialized_tokens_client_to_server",
        &report.serialized_tokens_client_to_server.to_string(),
    );
    csv_row(
        &mut out,
        "serialized_tokens_server_to_client",
        &report.serialized_tokens_server_to_client.to_string(),
    );
    csv_row(
        &mut out,
        "serialized_tokens_total",
        &report.serialized_tokens_total.to_string(),
    );
    csv_row(&mut out, "error_events", &report.error_events.to_string());
    csv_row(
        &mut out,
        "latency_samples",
        &report.latency_samples.to_string(),
    );
    optional_f64_row(&mut out, "latency_p50_ms", report.latency_p50_ms);
    optional_f64_row(&mut out, "latency_p95_ms", report.latency_p95_ms);
    optional_f64_row(&mut out, "latency_p99_ms", report.latency_p99_ms);
    optional_f64_row(&mut out, "latency_max_ms", report.latency_max_ms);
    out
}

pub fn render_html(report: &RunReport) -> String {
    let tools = if report.unique_tools.is_empty() {
        "—".to_string()
    } else {
        escape_html(&report.unique_tools.join(", "))
    };
    let estimate = if report.token_count_estimated {
        " <span class=\"badge\">estimated</span>"
    } else {
        ""
    };

    let mut rows = String::new();
    html_row(&mut rows, "Messages", &report.messages.to_string());
    html_row(
        &mut rows,
        "Requests / responses",
        &format!("{} / {}", report.requests, report.responses),
    );
    html_row(&mut rows, "Notifications", &report.notifications.to_string());
    html_row(&mut rows, "Tool calls", &report.tool_calls.to_string());
    html_row(
        &mut rows,
        "Unique tools called",
        &report.unique_tools.len().to_string(),
    );
    html_row(&mut rows, "Tools", &tools);
    html_row(
        &mut rows,
        "Tools exposed",
        &optional_u64_text(report.tools_exposed),
    );
    html_row(
        &mut rows,
        "Schema tokens",
        &optional_u64_text(report.schema_tokens),
    );
    html_row(
        &mut rows,
        "Wire bytes C→S / S→C",
        &format!(
            "{} / {}",
            report.wire_bytes_client_to_server, report.wire_bytes_server_to_client
        ),
    );
    html_row(
        &mut rows,
        "Wire bytes total",
        &report.wire_bytes_total.to_string(),
    );
    html_row(
        &mut rows,
        "Serialized tokens C→S / S→C",
        &format!(
            "{} / {}",
            report.serialized_tokens_client_to_server,
            report.serialized_tokens_server_to_client
        ),
    );
    html_row(
        &mut rows,
        "Serialized tokens total",
        &report.serialized_tokens_total.to_string(),
    );
    html_row(&mut rows, "Error events", &report.error_events.to_string());
    html_row(
        &mut rows,
        "Latency p50",
        &optional_f64_text(report.latency_p50_ms),
    );
    html_row(
        &mut rows,
        "Latency p95",
        &optional_f64_text(report.latency_p95_ms),
    );
    html_row(
        &mut rows,
        "Latency p99",
        &optional_f64_text(report.latency_p99_ms),
    );
    html_row(
        &mut rows,
        "Latency max",
        &optional_f64_text(report.latency_max_ms),
    );

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n<title>MCPMeter report {run}</title>\n<style>\nbody{{font-family:system-ui,-apple-system,BlinkMacSystemFont,\"Segoe UI\",sans-serif;max-width:960px;margin:40px auto;padding:0 20px;color:#18181b;background:#fafafa}}\n.card{{background:white;border:1px solid #e4e4e7;border-radius:14px;padding:24px;box-shadow:0 1px 3px rgba(0,0,0,.04)}}\nh1{{margin-top:0}}\n.meta{{color:#52525b;margin-bottom:24px}}\ntable{{width:100%;border-collapse:collapse}}\nth,td{{text-align:left;padding:10px 8px;border-bottom:1px solid #e4e4e7}}\nth{{width:46%;color:#52525b;font-weight:600}}\ncode{{font-family:ui-monospace,SFMono-Regular,Menlo,monospace}}\n.badge{{display:inline-block;padding:2px 7px;border-radius:999px;background:#f4f4f5;font-size:.8em;color:#52525b}}\nfooter{{margin-top:18px;color:#71717a;font-size:.9em}}\n</style>\n</head>\n<body>\n<div class=\"card\">\n<h1>MCPMeter report</h1>\n<div class=\"meta\">Run <code>{run}</code><br>Tokenizer <code>{tokenizer}</code>{estimate}</div>\n<table><tbody>\n{rows}</tbody></table>\n<footer>Generated locally by MCPMeter. Token metrics describe observed serialized MCP payloads, not automatically provider-billed or model-visible tokens.</footer>\n</div>\n</body>\n</html>\n",
        run = escape_html(&report.run_id),
        tokenizer = escape_html(&report.tokenizer),
    )
}

fn csv_row(out: &mut String, key: &str, value: &str) {
    out.push_str(&csv_escape(key));
    out.push(',');
    out.push_str(&csv_escape(value));
    out.push('\n');
}

fn optional_u64_row(out: &mut String, key: &str, value: Option<u64>) {
    csv_row(out, key, &value.map(|v| v.to_string()).unwrap_or_default());
}

fn optional_f64_row(out: &mut String, key: &str, value: Option<f64>) {
    csv_row(
        out,
        key,
        &value.map(|v| format!("{v:.3}")).unwrap_or_default(),
    );
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn html_row(out: &mut String, key: &str, value: &str) {
    out.push_str("<tr><th>");
    out.push_str(&escape_html(key));
    out.push_str("</th><td>");
    out.push_str(value);
    out.push_str("</td></tr>\n");
}

fn optional_u64_text(value: Option<u64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string())
}

fn optional_f64_text(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.3} ms"))
        .unwrap_or_else(|| "—".to_string())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> RunReport {
        RunReport {
            run_id: "run,<1>".to_string(),
            tokenizer: "o200k_base".to_string(),
            token_count_estimated: false,
            messages: 3,
            requests: 1,
            responses: 1,
            notifications: 1,
            tool_calls: 1,
            unique_tools: vec!["echo".to_string(), "quote,tool".to_string()],
            tools_exposed: Some(2),
            schema_tokens: Some(42),
            wire_bytes_client_to_server: 100,
            wire_bytes_server_to_client: 120,
            wire_bytes_total: 220,
            serialized_tokens_client_to_server: 20,
            serialized_tokens_server_to_client: 30,
            serialized_tokens_total: 50,
            error_events: 0,
            latency_samples: 1,
            latency_p50_ms: Some(1.25),
            latency_p95_ms: Some(1.25),
            latency_p99_ms: Some(1.25),
            latency_max_ms: Some(1.25),
        }
    }

    #[test]
    fn csv_quotes_values_with_commas() {
        let csv = render_csv(&sample_report());
        assert!(csv.contains("run_id,\"run,<1>\""));
        assert!(csv.contains("tools,\"echo,quote,tool\""));
    }

    #[test]
    fn html_escapes_untrusted_text() {
        let html = render_html(&sample_report());
        assert!(html.contains("run,&lt;1&gt;"));
        assert!(!html.contains("run,<1>"));
    }

    #[test]
    fn rejects_unknown_format() {
        assert!(ExportFormat::parse("xml").is_err());
    }
}
