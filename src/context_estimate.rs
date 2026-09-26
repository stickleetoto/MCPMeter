use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub const CONTEXT_ESTIMATE_ADAPTER_INTERFACE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextEstimateOrigin {
    Estimated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextEstimateFields {
    pub estimated_context_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelContextEstimate {
    pub interface_version: u32,
    pub origin: ContextEstimateOrigin,
    pub adapter: String,
    pub adapter_version: u32,
    pub basis: String,
    #[serde(flatten)]
    pub estimate: ContextEstimateFields,
}

pub trait ContextEstimateAdapter {
    fn adapter_id(&self) -> &'static str;
    fn adapter_version(&self) -> u32;
    fn estimate_basis(&self) -> &'static str;

    fn estimate_context(
        &self,
        context_input: &Value,
    ) -> Result<Option<ContextEstimateFields>, ContextEstimateAdapterError>;
}

pub fn adapt_context_estimate(
    adapter: &dyn ContextEstimateAdapter,
    context_input: &Value,
) -> Result<Option<ModelContextEstimate>, ContextEstimateAdapterError> {
    validate_identity(
        adapter.adapter_id(),
        ContextEstimateAdapterError::InvalidAdapterIdentity,
    )?;
    validate_identity(
        adapter.estimate_basis(),
        ContextEstimateAdapterError::InvalidEstimateBasis,
    )?;

    if adapter.adapter_version() == 0 {
        return Err(ContextEstimateAdapterError::InvalidAdapterVersion);
    }

    let Some(estimate) = adapter.estimate_context(context_input)? else {
        return Ok(None);
    };

    Ok(Some(ModelContextEstimate {
        interface_version: CONTEXT_ESTIMATE_ADAPTER_INTERFACE_VERSION,
        origin: ContextEstimateOrigin::Estimated,
        adapter: adapter.adapter_id().to_string(),
        adapter_version: adapter.adapter_version(),
        basis: adapter.estimate_basis().to_string(),
        estimate,
    }))
}

fn validate_identity(
    value: &str,
    error: ContextEstimateAdapterError,
) -> Result<(), ContextEstimateAdapterError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(error);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextEstimateAdapterError {
    UnsupportedInput,
    MissingField(&'static str),
    InvalidField(&'static str),
    InvalidAdapterIdentity,
    InvalidAdapterVersion,
    InvalidEstimateBasis,
}

impl fmt::Display for ContextEstimateAdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedInput => formatter.write_str("unsupported context estimate input"),
            Self::MissingField(field) => write!(formatter, "missing context estimate field: {field}"),
            Self::InvalidField(field) => write!(formatter, "invalid context estimate field: {field}"),
            Self::InvalidAdapterIdentity => {
                formatter.write_str("context estimate adapter id must be non-empty")
            }
            Self::InvalidAdapterVersion => {
                formatter.write_str("context estimate adapter version must be greater than zero")
            }
            Self::InvalidEstimateBasis => {
                formatter.write_str("context estimate basis must be non-empty")
            }
        }
    }
}

impl std::error::Error for ContextEstimateAdapterError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct FixedEstimateAdapter;

    impl ContextEstimateAdapter for FixedEstimateAdapter {
        fn adapter_id(&self) -> &'static str {
            "fixture_context"
        }

        fn adapter_version(&self) -> u32 {
            1
        }

        fn estimate_basis(&self) -> &'static str {
            "host_assembled_context"
        }

        fn estimate_context(
            &self,
            _context_input: &Value,
        ) -> Result<Option<ContextEstimateFields>, ContextEstimateAdapterError> {
            Ok(Some(ContextEstimateFields {
                estimated_context_tokens: 321,
                model: Some("fixture-model".to_string()),
            }))
        }
    }

    struct NoEstimateAdapter;

    impl ContextEstimateAdapter for NoEstimateAdapter {
        fn adapter_id(&self) -> &'static str {
            "no_estimate"
        }

        fn adapter_version(&self) -> u32 {
            1
        }

        fn estimate_basis(&self) -> &'static str {
            "explicit_context_only"
        }

        fn estimate_context(
            &self,
            _context_input: &Value,
        ) -> Result<Option<ContextEstimateFields>, ContextEstimateAdapterError> {
            Ok(None)
        }
    }

    #[test]
    fn estimate_is_explicitly_labeled_and_versioned() {
        let estimate =
            adapt_context_estimate(&FixedEstimateAdapter, &json!({"messages": []}))
                .unwrap()
                .unwrap();

        assert_eq!(
            estimate.interface_version,
            CONTEXT_ESTIMATE_ADAPTER_INTERFACE_VERSION
        );
        assert_eq!(estimate.origin, ContextEstimateOrigin::Estimated);
        assert_eq!(estimate.adapter, "fixture_context");
        assert_eq!(estimate.adapter_version, 1);
        assert_eq!(estimate.basis, "host_assembled_context");
        assert_eq!(estimate.estimate.estimated_context_tokens, 321);

        let value = serde_json::to_value(estimate).unwrap();
        assert_eq!(value["origin"], "estimated");
        assert_eq!(value["estimated_context_tokens"], 321);
        assert!(value.get("provider").is_none());
        assert!(value.get("input_tokens").is_none());
        assert!(value.get("serialized_tokens").is_none());
    }

    #[test]
    fn wrapper_does_not_manufacture_estimate_from_other_token_sources() {
        let context_input = json!({
            "serialized_tokens": 111,
            "input_tokens": 222,
            "output_tokens": 333,
            "total_tokens": 555
        });

        let estimate = adapt_context_estimate(&NoEstimateAdapter, &context_input).unwrap();
        assert!(estimate.is_none());
    }

    #[test]
    fn invalid_contract_errors_do_not_echo_input_values() {
        struct InvalidBasisAdapter;

        impl ContextEstimateAdapter for InvalidBasisAdapter {
            fn adapter_id(&self) -> &'static str {
                "invalid_basis"
            }

            fn adapter_version(&self) -> u32 {
                1
            }

            fn estimate_basis(&self) -> &'static str {
                ""
            }

            fn estimate_context(
                &self,
                _context_input: &Value,
            ) -> Result<Option<ContextEstimateFields>, ContextEstimateAdapterError> {
                Ok(Some(ContextEstimateFields {
                    estimated_context_tokens: 1,
                    model: None,
                }))
            }
        }

        let secret_marker = "context-secret-marker";
        let error = adapt_context_estimate(
            &InvalidBasisAdapter,
            &json!({"content": secret_marker}),
        )
        .unwrap_err();

        assert!(error.to_string().contains("basis"));
        assert!(!error.to_string().contains(secret_marker));
    }
}
