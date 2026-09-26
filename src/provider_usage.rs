use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub const PROVIDER_USAGE_ADAPTER_INTERFACE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderUsageOrigin {
    ProviderReported,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderUsageFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
}

impl ProviderUsageFields {
    fn has_token_usage(&self) -> bool {
        self.input_tokens.is_some() || self.output_tokens.is_some() || self.total_tokens.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderReportedUsage {
    pub interface_version: u32,
    pub origin: ProviderUsageOrigin,
    pub adapter: String,
    pub adapter_version: u32,
    pub provider: String,
    #[serde(flatten)]
    pub usage: ProviderUsageFields,
}

pub trait ProviderUsageAdapter {
    fn adapter_id(&self) -> &'static str;
    fn adapter_version(&self) -> u32;
    fn provider_id(&self) -> &'static str;

    fn extract_usage(
        &self,
        provider_payload: &Value,
    ) -> Result<Option<ProviderUsageFields>, ProviderUsageAdapterError>;
}

pub fn adapt_provider_usage(
    adapter: &dyn ProviderUsageAdapter,
    provider_payload: &Value,
) -> Result<Option<ProviderReportedUsage>, ProviderUsageAdapterError> {
    validate_identity(adapter.adapter_id(), ProviderUsageAdapterError::InvalidAdapterIdentity)?;
    validate_identity(
        adapter.provider_id(),
        ProviderUsageAdapterError::InvalidProviderIdentity,
    )?;

    if adapter.adapter_version() == 0 {
        return Err(ProviderUsageAdapterError::InvalidAdapterVersion);
    }

    let Some(usage) = adapter.extract_usage(provider_payload)? else {
        return Ok(None);
    };
    if !usage.has_token_usage() {
        return Err(ProviderUsageAdapterError::EmptyUsage);
    }

    Ok(Some(ProviderReportedUsage {
        interface_version: PROVIDER_USAGE_ADAPTER_INTERFACE_VERSION,
        origin: ProviderUsageOrigin::ProviderReported,
        adapter: adapter.adapter_id().to_string(),
        adapter_version: adapter.adapter_version(),
        provider: adapter.provider_id().to_string(),
        usage,
    }))
}

fn validate_identity(
    value: &str,
    error: ProviderUsageAdapterError,
) -> Result<(), ProviderUsageAdapterError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(error);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderUsageAdapterError {
    UnsupportedPayload,
    MissingField(&'static str),
    InvalidField(&'static str),
    InvalidAdapterIdentity,
    InvalidAdapterVersion,
    InvalidProviderIdentity,
    EmptyUsage,
}

impl fmt::Display for ProviderUsageAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPayload => formatter.write_str("unsupported provider usage payload"),
            Self::MissingField(field) => write!(formatter, "missing provider usage field: {field}"),
            Self::InvalidField(field) => write!(formatter, "invalid provider usage field: {field}"),
            Self::InvalidAdapterIdentity => {
                formatter.write_str("provider usage adapter id must be non-empty")
            }
            Self::InvalidAdapterVersion => {
                formatter.write_str("provider usage adapter version must be greater than zero")
            }
            Self::InvalidProviderIdentity => {
                formatter.write_str("provider usage provider id must be non-empty")
            }
            Self::EmptyUsage => {
                formatter.write_str("provider usage payload did not report token counts")
            }
        }
    }
}

impl std::error::Error for ProviderUsageAdapterError {}
