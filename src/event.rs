use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

const REDACTED_VALUE: &str = "[REDACTED]";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RedactedMetadataField {
    Methods,
    Tools,
    HttpProtocolVersion,
    HttpMethod,
    HttpName,
    ParseError,
}

#[derive(Debug, Clone, Default)]
pub struct RedactionRules {
    metadata_fields: BTreeSet<RedactedMetadataField>,
    payload_fields: BTreeSet<String>,
}

impl RedactionRules {
    pub fn parse(
        metadata_rules: &[String],
        payload_fields: &[String],
    ) -> Result<Self, &'static str> {
        let mut rules = Self::default();

        for raw_rule in metadata_rules {
            let normalized = raw_rule.trim().to_ascii_lowercase().replace('_', "-");
            match normalized.as_str() {
                "methods" => {
                    rules.metadata_fields.insert(RedactedMetadataField::Methods);
                }
                "tools" => {
                    rules.metadata_fields.insert(RedactedMetadataField::Tools);
                }
                "http-protocol-version" => {
                    rules
                        .metadata_fields
                        .insert(RedactedMetadataField::HttpProtocolVersion);
                }
                "http-method" => {
                    rules
                        .metadata_fields
                        .insert(RedactedMetadataField::HttpMethod);
                }
                "http-name" => {
                    rules
                        .metadata_fields
                        .insert(RedactedMetadataField::HttpName);
                }
                "parse-error" => {
                    rules
                        .metadata_fields
                        .insert(RedactedMetadataField::ParseError);
                }
                "all" => {
                    rules.metadata_fields.extend([
                        RedactedMetadataField::Methods,
                        RedactedMetadataField::Tools,
                        RedactedMetadataField::HttpProtocolVersion,
                        RedactedMetadataField::HttpMethod,
                        RedactedMetadataField::HttpName,
                        RedactedMetadataField::ParseError,
                    ]);
                }
                _ => {
                    return Err(
                        "unsupported --redact-metadata rule; expected methods, tools, http-protocol-version, http-method, http-name, parse-error, or all",
                    );
                }
            }
        }

        for raw_field in payload_fields {
            let field = raw_field.trim();
            if field.is_empty() || field.len() > 128 || field.chars().any(char::is_control) {
                return Err(
                    "invalid --redact-payload-field rule; use a non-empty JSON key up to 128 characters",
                );
            }
            rules.payload_fields.insert(field.to_ascii_lowercase());
        }

        Ok(rules)
    }

    pub fn apply(&self, event: &mut MeasurementEvent) {
        if self
            .metadata_fields
            .contains(&RedactedMetadataField::Methods)
        {
            event.methods.clear();
        }
        if self.metadata_fields.contains(&RedactedMetadataField::Tools) {
            event.tools.clear();
        }
        if self
            .metadata_fields
            .contains(&RedactedMetadataField::HttpProtocolVersion)
        {
            event.http_mcp_protocol_version = None;
        }
        if self
            .metadata_fields
            .contains(&RedactedMetadataField::HttpMethod)
        {
            event.http_mcp_method = None;
        }
        if self
            .metadata_fields
            .contains(&RedactedMetadataField::HttpName)
        {
            event.http_mcp_name = None;
        }
        if self
            .metadata_fields
            .contains(&RedactedMetadataField::ParseError)
        {
            event.parse_error = None;
        }

        if !self.payload_fields.is_empty() {
            event.raw_payload = event
                .raw_payload
                .take()
                .and_then(|payload| redact_raw_payload(&payload, &self.payload_fields));
        }
    }
}

fn redact_raw_payload(payload: &str, fields: &BTreeSet<String>) -> Option<String> {
    let mut value: Value = serde_json::from_str(payload).ok()?;
    redact_json_value(&mut value, fields);
    serde_json::to_string(&value).ok()
}

fn redact_json_value(value: &mut Value, fields: &BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if fields.contains(&key.to_ascii_lowercase()) {
                    *child = Value::String(REDACTED_VALUE.to_string());
                } else {
                    redact_json_value(child, fields);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json_value(item, fields);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementEvent {
    pub schema_version: u32,
    pub run_id: String,
    pub ts_unix_ns: u128,
    #[serde(default)]
    pub transport: TransportKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_mcp_protocol_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_mcp_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_mcp_name: Option<String>,
    pub direction: Direction,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_bytes: Option<u64>,
    pub serialized_tokens: u64,
    pub tokenizer: String,
    pub token_count_estimated: bool,
    pub payload_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_payload: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    pub request_count: u64,
    pub response_count: u64,
    pub notification_count: u64,
    pub tool_call_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools_exposed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub latencies_us: Vec<u64>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    ClientToServer,
    ServerToClient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    #[default]
    Stdio,
    StreamableHttp,
}
