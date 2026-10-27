//! A plugin's name, which also names the namespace its credentials live in.

use std::fmt;

use super::ManifestError;

/// The longest plugin name.
const MAX: usize = 32;

/// `[a-z][a-z0-9]{0,31}`: printable in a path, a log line and a secret's name.
///
/// No separator, so `PLUGIN_<NAME>_` namespaces are disjoint: a prefix of one name's namespace
/// followed by `_` is never another name's.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct PluginName(String);

impl PluginName {
    /// Parse one.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        let mut bytes = text.bytes();
        let first_is_letter = bytes.next().is_some_and(|b| b.is_ascii_lowercase());
        if first_is_letter
            && text.len() <= MAX
            && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        {
            Ok(Self(text.to_owned()))
        } else {
            Err(ManifestError::BadName(text.to_owned()))
        }
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `PLUGIN_<NAME>_`, the prefix every credential this plugin declares must carry, so a
    /// manifest cannot name the app's own secrets or another plugin's.
    #[must_use]
    pub fn credential_prefix(&self) -> String {
        format!("PLUGIN_{}_", self.0.to_ascii_uppercase())
    }
}

impl TryFrom<String> for PluginName {
    type Error = ManifestError;
    fn try_from(text: String) -> Result<Self, ManifestError> {
        Self::parse(&text)
    }
}

impl From<PluginName> for String {
    fn from(name: PluginName) -> Self {
        name.0
    }
}

// A grant card stores the plugin by its name.
prodrome_core::text_literal!(PluginName => |name| name.as_str().to_owned());

impl fmt::Display for PluginName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_lowercase_and_starts_with_a_letter() {
        assert!(PluginName::parse("calendar").is_ok());
        assert!(PluginName::parse("ics2").is_ok());
        for bad in [
            "",
            "Calendar",
            "2ics",
            "ics-feed",
            "ics_feed",
            "ics.feed",
            &"a".repeat(33),
        ] {
            assert!(PluginName::parse(bad).is_err(), "{bad:?}");
        }
    }
}
