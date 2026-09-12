use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementEvent {
    pub schema_version: u32,
    pub run_id: String,
    pub ts_unix_ns: u128,
    pub direction: Direction,
    pub kind: String,
    pub wire_bytes: u64,
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
