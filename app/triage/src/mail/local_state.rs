//! `local/mail.json`, never pushed and carried by the vault, keyed by the canonical content hash,
//! which may be named here because nothing here leaves the box.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub(crate) const SCHEMA_VERSION: u64 = 1;

/// Field order is the stored key order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kept {
    /// A notmuch locator: a shortcut back to the mail, never its identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_id: Option<String>,
    /// `""` is absent and omitted.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum LocalError {
    #[error("the mail's box-local file is not JSON of its shape: {0}")]
    Unparseable(String),
    #[error("the mail's box-local file is version {0:?}; this program reads {SCHEMA_VERSION}")]
    Version(Option<u64>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalState {
    /// Senders whose mail is ignored wholesale, lower-cased.
    pub(crate) noise_senders: BTreeSet<String>,
    /// By canonical hash.
    pub messages: BTreeMap<String, Kept>,
}

/// In the order it is written.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u64,
    noise_senders: BTreeSet<String>,
    messages: BTreeMap<String, Kept>,
}

impl LocalState {
    pub fn parse(text: &str) -> Result<Self, LocalError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| LocalError::Unparseable(e.to_string()))?;
        let version = value.get("version").and_then(serde_json::Value::as_u64);
        if version != Some(SCHEMA_VERSION) {
            return Err(LocalError::Version(version));
        }
        let document: Document =
            serde_json::from_value(value).map_err(|e| LocalError::Unparseable(e.to_string()))?;
        Ok(Self {
            noise_senders: document.noise_senders,
            messages: document.messages,
        })
    }

    /// Trailing newline included.
    #[must_use]
    #[expect(
        clippy::missing_panics_doc,
        reason = "unreachable: every field is a string, a u64 or a map with string keys"
    )]
    pub fn print(&self) -> String {
        let document = Document {
            version: SCHEMA_VERSION,
            noise_senders: self.noise_senders.clone(),
            messages: self.messages.clone(),
        };
        let mut text =
            serde_json::to_string_pretty(&document).expect("a local document always serialises");
        text.push('\n');
        text
    }

    #[must_use]
    pub fn is_noise(&self, sender: &str) -> bool {
        self.noise_senders.contains(&sender.to_lowercase())
    }

    #[must_use]
    pub fn add_noise(mut self, sender: &str) -> Self {
        self.noise_senders.insert(sender.to_lowercase());
        self
    }

    #[must_use]
    pub fn keep(mut self, hash: &str, kept: Kept) -> Self {
        self.messages.insert(hash.to_owned(), kept);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_printed_document_round_trips() {
        let state = LocalState::default()
            .add_noise("Noreply@Example.Test")
            .keep(
                "bbbb",
                Kept {
                    native_id: Some("msg-2".to_owned()),
                    note: "renewal".to_owned(),
                },
            )
            .keep("aaaa", Kept::default());
        let printed = state.print();
        assert_eq!(
            printed,
            concat!(
                "{\n",
                "  \"version\": 1,\n",
                "  \"noise_senders\": [\n",
                "    \"noreply@example.test\"\n",
                "  ],\n",
                "  \"messages\": {\n",
                "    \"aaaa\": {},\n",
                "    \"bbbb\": {\n",
                "      \"native_id\": \"msg-2\",\n",
                "      \"note\": \"renewal\"\n",
                "    }\n",
                "  }\n",
                "}\n",
            )
        );
        assert_eq!(LocalState::parse(&printed), Ok(state));
        assert_eq!(LocalState::parse("{}"), Err(LocalError::Version(None)));
    }
}
