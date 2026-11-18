//! The name a deployment's model is asked for by, parsed once where the instance file is read.

use serde::{Deserialize, Serialize};

/// A model name, `[A-Za-z0-9._:/-]{1,128}`: what model ids use and nothing a
/// command line or a TOML string could be confused by.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ModelName(String);

impl ModelName {
    /// Longest model name accepted.
    pub const MAX: usize = 128;

    /// Parse one.
    ///
    /// # Errors
    /// [`BadModel`] for an empty, overlong or out-of-class name.
    pub fn parse(text: &str) -> Result<Self, BadModel> {
        let fits = (1..=Self::MAX).contains(&text.len())
            && text.bytes().all(|b| {
                b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'/' | b'-')
            });
        if fits {
            Ok(Self(text.to_owned()))
        } else {
            Err(BadModel(text.to_owned()))
        }
    }

    /// The name, as the model's implementation takes it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

prodrome_core::text_literal!(
    ModelName => |name| name.as_str().to_owned(),
);

impl TryFrom<String> for ModelName {
    type Error = BadModel;
    fn try_from(text: String) -> Result<Self, BadModel> {
        Self::parse(&text)
    }
}

impl From<ModelName> for String {
    fn from(name: ModelName) -> Self {
        name.0
    }
}

/// Why a model name did not parse, naming what was typed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a model name (1 to 128 of A-Z a-z 0-9 . _ : / -)")]
pub struct BadModel(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    /// A model name cannot carry anything but its class.
    #[test]
    fn a_model_name_is_its_class_and_length() {
        for bad in [
            "",
            "has space",
            "a\"b",
            "a\nb",
            "x?y=z",
            "é",
            &"m".repeat(129),
        ] {
            assert_eq!(
                ModelName::parse(bad),
                Err(BadModel(bad.to_owned())),
                "{bad:?}"
            );
        }
        assert!(ModelName::parse(&"m".repeat(128)).is_ok());
    }
}
