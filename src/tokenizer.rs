use anyhow::{bail, Result};
use tiktoken_rs::{cl100k_base_singleton, o200k_base_singleton};

#[derive(Clone, Debug)]
pub enum TokenizerProfile {
    O200kBase,
    Cl100kBase,
    Bytes4Estimate,
}

impl TokenizerProfile {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "o200k" | "o200k-base" | "o200k_base" => Ok(Self::O200kBase),
            "cl100k" | "cl100k-base" | "cl100k_base" => Ok(Self::Cl100kBase),
            "bytes4" | "bytes4-estimate" | "bytes4_estimate" => Ok(Self::Bytes4Estimate),
            other => bail!("unknown tokenizer profile: {other}"),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::O200kBase => "o200k_base",
            Self::Cl100kBase => "cl100k_base",
            Self::Bytes4Estimate => "bytes4_estimate",
        }
    }

    pub fn is_estimate(&self) -> bool {
        matches!(self, Self::Bytes4Estimate)
    }

    pub fn count(&self, text: &str) -> usize {
        match self {
            Self::O200kBase => o200k_base_singleton().encode_ordinary(text).len(),
            Self::Cl100kBase => cl100k_base_singleton().encode_ordinary(text).len(),
            Self::Bytes4Estimate => text.len().div_ceil(4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_have_stable_names() {
        assert_eq!(
            TokenizerProfile::parse("o200k-base").unwrap().name(),
            "o200k_base"
        );
        assert_eq!(
            TokenizerProfile::parse("cl100k").unwrap().name(),
            "cl100k_base"
        );
        assert!(TokenizerProfile::parse("bytes4").unwrap().is_estimate());
    }

    #[test]
    fn byte_estimator_rounds_up() {
        let p = TokenizerProfile::Bytes4Estimate;
        assert_eq!(p.count(""), 0);
        assert_eq!(p.count("a"), 1);
        assert_eq!(p.count("abcd"), 1);
        assert_eq!(p.count("abcde"), 2);
    }
}
