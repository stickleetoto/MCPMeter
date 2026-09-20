use crate::event::{Direction, MeasurementEvent};
use crate::observer::{observe_http_payload_at, unix_now_ns, PendingMap};
use crate::tokenizer::TokenizerProfile;
use anyhow::{anyhow, bail, Context, Result};
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::PathBuf;
use std::time::Instant;
use tiny_http::{Header, Request, Response, Server, StatusCode};

const MAX_DIRECT_BODY_BYTES: u64 = 64 * 1024 * 1024;

pub struct HttpProxyConfig {
    pub listen: String,
    pub upstream: String,
    pub trace_path: PathBuf,
    pub tokenizer: TokenizerProfile,
    pub capture_payloads: bool,
}

pub fn run(config: HttpProxyConfig) -> Result<()> {
    validate_upstream(&config.upstream)?;

    if config.capture_payloads {
        eprintln!(
            "MCPMeter WARNING: --capture-payloads stores raw MCP HTTP bodies and may persist secrets or private data"
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
    let mut writer = BufWriter::new(trace_file);
    let mut pending = PendingMap::new();

    let server = Server::http(&config.listen)
        .map_err(|error| anyhow!("failed to listen on {}: {error}", config.listen))?;
    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .user_agent("")
        .build();

    let run_id = format!("{}-http-{}", unix_now_ns(), std::process::id());
    eprintln!(
        "MCPMeter HTTP run {run_id}: http://{} -> {}",
        config.listen, config.upstream
    );

    for request in server.incoming_requests() {
        if let Err(error) = handle_request(
            request,
            &config,
            &agent,
            &run_id,
            &mut pending,
            &mut writer,
        ) {
            eprintln!("MCPMeter warning: HTTP request handling failed: {error:#}");
        }
    }

    writer.flush()?;
    Ok(())
}

fn handle_request(
    mut request: Request,
    config: &HttpProxyConfig,
    agent: &ureq::Agent,
    run_id: &str,
    pending: &mut PendingMap,
    writer: &mut BufWriter<File>,
) -> Result<()> {
    let method = request.method().as_str().to_string();
    let request_target = request.url().to_string();
    let upstream_url = build_upstream_url(&config.upstream, &request_target)?;

    let mut request_body = Vec::new();
    request
        .as_reader()
        .take(MAX_DIRECT_BODY_BYTES + 1)
        .read_to_end(&mut request_body)
        .context("failed to read downstream request body")?;
    if request_body.len() as u64 > MAX_DIRECT_BODY_BYTES {
        request
            .respond(
                Response::from_string("MCPMeter: request body exceeds 64 MiB direct-JSON limit")
                    .with_status_code(StatusCode(413)),
            )
            .context("failed to return request-size error")?;
        return Ok(());
    }

    let request_observed_at = Instant::now();
    if !request_body.is_empty() {
        let event = observe_http_payload_at(
            &request_body,
            Direction::ClientToServer,
            run_id,
            &config.tokenizer,
            config.capture_payloads,
            pending,
            request_observed_at,
            unix_now_ns(),
        );
        write_event(writer, &event);
    }

    let mut upstream_request = agent.request(&method, &upstream_url);
    for header in request.headers() {
        let name = header.field.to_string();
        if should_forward_request_header(&name) {
            upstream_request = upstream_request.set(&name, &header.value.to_string());
        }
    }

    let upstream_result = if request_body.is_empty() {
        upstream_request.call()
    } else {
        upstream_request.send_bytes(&request_body)
    };

    let upstream_response = match upstream_result {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            request
                .respond(
                    Response::from_string(format!("MCPMeter upstream error: {error}"))
                        .with_status_code(StatusCode(502)),
                )
                .context("failed to return upstream error")?;
            return Ok(());
        }
    };

    let status = upstream_response.status();
    let content_type = upstream_response
        .header("content-type")
        .unwrap_or("")
        .to_ascii_lowercase();

    if content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim() == "text/event-stream")
    {
        request
            .respond(
                Response::from_string(
                    "MCPMeter: SSE forwarding is not enabled in this direct-JSON slice",
                )
                .with_status_code(StatusCode(501)),
            )
            .context("failed to return SSE-not-supported response")?;
        return Ok(());
    }

    let response_headers = collect_response_headers(&upstream_response);
    let mut response_body = Vec::new();
    upstream_response
        .into_reader()
        .take(MAX_DIRECT_BODY_BYTES + 1)
        .read_to_end(&mut response_body)
        .context("failed to read upstream response body")?;

    if response_body.len() as u64 > MAX_DIRECT_BODY_BYTES {
        request
            .respond(
                Response::from_string("MCPMeter: response body exceeds 64 MiB direct-JSON limit")
                    .with_status_code(StatusCode(502)),
            )
            .context("failed to return response-size error")?;
        return Ok(());
    }

    if !response_body.is_empty() {
        let event = observe_http_payload_at(
            &response_body,
            Direction::ServerToClient,
            run_id,
            &config.tokenizer,
            config.capture_payloads,
            pending,
            Instant::now(),
            unix_now_ns(),
        );
        write_event(writer, &event);
    }

    let mut downstream_response =
        Response::from_data(response_body).with_status_code(StatusCode(status));
    for header in response_headers {
        downstream_response.add_header(header);
    }

    request
        .respond(downstream_response)
        .context("failed to send downstream response")?;
    Ok(())
}

fn collect_response_headers(response: &ureq::Response) -> Vec<Header> {
    let mut headers = Vec::new();
    for name in response.headers_names() {
        if !should_forward_response_header(&name) {
            continue;
        }
        for value in response.all(&name) {
            if let Ok(header) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
                headers.push(header);
            }
        }
    }
    headers
}

fn should_forward_request_header(name: &str) -> bool {
    !name.eq_ignore_ascii_case("host")
        && !name.eq_ignore_ascii_case("content-length")
        && !is_hop_by_hop(name)
}

fn should_forward_response_header(name: &str) -> bool {
    !name.eq_ignore_ascii_case("content-length") && !is_hop_by_hop(name)
}

fn is_hop_by_hop(name: &str) -> bool {
    [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn build_upstream_url(base: &str, request_target: &str) -> Result<String> {
    if !request_target.starts_with('/') {
        bail!("unsupported HTTP request target: {request_target}");
    }
    Ok(format!("{}{}", base.trim_end_matches('/'), request_target))
}

fn validate_upstream(upstream: &str) -> Result<()> {
    if upstream.starts_with("http://") || upstream.starts_with("https://") {
        return Ok(());
    }
    bail!("upstream must start with http:// or https://");
}

fn write_event(writer: &mut BufWriter<File>, event: &MeasurementEvent) {
    if let Err(error) = (|| -> Result<()> {
        serde_json::to_writer(&mut *writer, event)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    })() {
        eprintln!("MCPMeter warning: failed to write HTTP trace event: {error:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_url_preserves_path_and_query() {
        assert_eq!(
            build_upstream_url("http://127.0.0.1:9000", "/mcp?x=1").unwrap(),
            "http://127.0.0.1:9000/mcp?x=1"
        );
        assert_eq!(
            build_upstream_url("http://127.0.0.1:9000/base/", "/mcp").unwrap(),
            "http://127.0.0.1:9000/base/mcp"
        );
    }

    #[test]
    fn authorization_and_mcp_headers_are_forwarded() {
        assert!(should_forward_request_header("Authorization"));
        assert!(should_forward_request_header("MCP-Protocol-Version"));
        assert!(should_forward_request_header("Mcp-Method"));
        assert!(should_forward_request_header("Mcp-Param-query"));
        assert!(!should_forward_request_header("Host"));
        assert!(!should_forward_request_header("Connection"));
    }
}
