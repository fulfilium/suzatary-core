//! What a tool turn, or a model's call, names: read once, where the name arrives.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::verb_name::VerbName;

/// A name a transcript holds beside a call or an answer. Stored and debugged as the bare name it
/// prints, so every transcript and every pin of one reads as it did when the name was a string.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ToolName {
    /// One of the secretary's verbs.
    Verb(VerbName),
    /// The memory's heading, filed as a verb-shaped turn so a model reads it as data like any
    /// answer, never as the owner's words.
    Memory,
    /// Something the person pointed at, loaded before the round.
    Reference,
    /// A name the surface does not list: a model's guess, or a verb since retired.
    Unlisted(Unlisted),
}

/// A name that is none of the others; only [`ToolName::from`] makes one, so it never spells a
/// listed verb or a heading.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Unlisted(String);

impl ToolName {
    /// The name as it is printed and stored.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Verb(verb) => verb.name(),
            Self::Memory => "your_memory",
            Self::Reference => "reference",
            Self::Unlisted(Unlisted(name)) => name,
        }
    }

    /// The verb it names, if the surface lists it.
    #[must_use]
    pub const fn verb(&self) -> Option<VerbName> {
        match self {
            Self::Verb(verb) => Some(*verb),
            Self::Memory | Self::Reference | Self::Unlisted(_) => None,
        }
    }
}

impl From<&str> for ToolName {
    fn from(name: &str) -> Self {
        match name {
            "your_memory" => Self::Memory,
            "reference" => Self::Reference,
            _ => VerbName::named(name)
                .map_or_else(|| Self::Unlisted(Unlisted(name.to_owned())), Self::Verb),
        }
    }
}

impl From<String> for ToolName {
    fn from(name: String) -> Self {
        Self::from(name.as_str())
    }
}

impl From<ToolName> for String {
    fn from(name: ToolName) -> Self {
        match name {
            ToolName::Unlisted(Unlisted(name)) => name,
            listed => listed.as_str().to_owned(),
        }
    }
}

impl fmt::Debug for ToolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Debug for Unlisted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for ToolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn every_listed() -> Vec<ToolName> {
        VerbName::ALL
            .iter()
            .copied()
            .map(ToolName::Verb)
            .chain([ToolName::Memory, ToolName::Reference])
            .collect()
    }

    #[test]
    fn a_listed_name_reads_back_as_itself_and_no_two_spell_alike() {
        let listed = every_listed();
        for name in &listed {
            assert_eq!(&ToolName::from(name.as_str()), name);
        }
        let spellings: std::collections::BTreeSet<&str> =
            listed.iter().map(ToolName::as_str).collect();
        assert_eq!(spellings.len(), listed.len());
    }

    proptest! {
        /// Reading a name and printing it gives back the name, so a stored transcript reprints
        /// byte for byte.
        #[test]
        fn a_name_prints_as_it_was_read(name in "[a-z_]{0,16}") {
            let read = ToolName::from(name.as_str());
            prop_assert_eq!(read.as_str(), name.as_str());
            prop_assert_eq!(String::from(read.clone()), name.clone());
            let json = serde_json::to_string(&read).expect("a string");
            prop_assert_eq!(&json, &serde_json::to_string(&name).expect("a string"));
            prop_assert_eq!(serde_json::from_str::<ToolName>(&json).expect("it reads"), read);
        }
    }
}
