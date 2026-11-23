//! What a `Began` line names: the run that claims a round, and each turn it covers with the box's
//! judgment of it.

use prodrome_data::event::Hash;
use prodrome_data::literal::codec::{Constructor, Fields, Literal, record};
use prodrome_data::literal::{ProdromeError, Value};

use crate::secretary::taint::Taint;

/// One process's round, named by 32 hex digits drawn from the system's entropy. A run is gone
/// once nothing holds its lock.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId(String);

impl RunId {
    pub fn drawn(entropy: &dyn suzatary_base::capability::Entropy) -> Result<Self, String> {
        let mut bytes = [0u8; 16];
        entropy.fill(&mut bytes)?;
        Ok(Self::of(bytes))
    }

    #[must_use]
    pub fn of(bytes: [u8; 16]) -> Self {
        Self(hex::encode(bytes))
    }

    /// The run `text` names: 32 lowercase hex digits, nothing else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        (text.len() == 32
            && text
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')))
        .then(|| Self(text.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl serde::Serialize for RunId {
    fn serialize<S: serde::Serializer>(&self, to: S) -> Result<S::Ok, S::Error> {
        to.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for RunId {
    fn deserialize<D: serde::Deserializer<'de>>(from: D) -> Result<Self, D::Error> {
        let text = String::deserialize(from)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("a run is 32 hex digits"))
    }
}

impl Literal for RunId {
    fn to_value(&self) -> Value {
        self.0.to_value()
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let text = String::from_value(value)?;
        Self::parse(&text)
            .ok_or_else(|| ProdromeError::invalid(format!("{text:?} is no run: 32 hex digits")))
    }
}

/// A turn a claim covers, by its line's name, and the taint the box judged it to carry then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Covered {
    pub line: Hash,
    pub taint: Taint,
}

impl serde::Serialize for Covered {
    fn serialize<S: serde::Serializer>(&self, to: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct as _;
        let mut covered = to.serialize_struct("Covered", 2)?;
        covered.serialize_field("line", self.line.as_str())?;
        covered.serialize_field("taint", self.taint.as_str())?;
        covered.end()
    }
}

/// `Covered(line, taint)`.
pub const COVERED: Constructor = ("Covered", &["line", "taint"]);

impl Literal for Covered {
    fn to_value(&self) -> Value {
        record(COVERED, vec![self.line.to_value(), self.taint.to_value()])
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::among(value, "Covered", &[COVERED])?;
        Ok(Self {
            line: fields.get("line")?,
            taint: fields.get("taint")?,
        })
    }
}
