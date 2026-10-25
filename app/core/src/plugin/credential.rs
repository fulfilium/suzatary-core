//! A credential a plugin may name: a secret whose value is a URL the host requests for it.

use std::fmt;

use super::{ManifestError, PluginName};

/// The longest credential name.
const MAX: usize = 96;

/// A secret's name in the plugin's own namespace, `PLUGIN_<NAME>_[A-Z0-9_]+`.
///
/// The namespace is the fence a manifest cannot reach around: the app's own keys, the release
/// secrets and another plugin's credentials are outside it, so naming one does not parse.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CredentialName {
    name: String,
    /// Where the part after the plugin's namespace begins.
    label: usize,
}

impl CredentialName {
    /// Parse one in `plugin`'s namespace.
    pub fn parse(plugin: &PluginName, text: &str) -> Result<Self, ManifestError> {
        let prefix = plugin.credential_prefix();
        let rest = text.strip_prefix(&prefix).unwrap_or_default();
        let shaped = !rest.is_empty()
            && text.len() <= MAX
            && rest
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
        if shaped {
            Ok(Self {
                name: text.to_owned(),
                label: prefix.len(),
            })
        } else {
            Err(ManifestError::CredentialOutsideNamespace {
                name: text.to_owned(),
                prefix,
            })
        }
    }

    /// The name, as secretspec spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// The part after the plugin's namespace, `WORK` of `PLUGIN_ICS_WORK`.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.name[self.label..]
    }
}

impl fmt::Display for CredentialName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No name outside a plugin's own namespace parses, and no name is in two namespaces.
    #[test]
    fn a_credential_lives_in_its_plugin_namespace_alone() {
        let calendar = PluginName::parse("calendar").expect("a name");
        assert!(CredentialName::parse(&calendar, "PLUGIN_CALENDAR_ICS_URL").is_ok());
        for bad in [
            "MODEL_TOKEN",
            "SUZATARY_ROUTINE_TOKEN",
            "PLUGIN_BANK_TOKEN",
            "PLUGIN_CALENDAR_",
            "PLUGIN_CALENDAR_ics",
            "plugin_calendar_ICS",
            "PLUGIN_CALENDAR_ICS-URL",
        ] {
            assert!(CredentialName::parse(&calendar, bad).is_err(), "{bad:?}");
        }
        let ics = PluginName::parse("ics").expect("a name");
        let icsfeed = PluginName::parse("icsfeed").expect("a name");
        assert!(CredentialName::parse(&icsfeed, "PLUGIN_ICSFEED_TOKEN").is_ok());
        assert!(CredentialName::parse(&ics, "PLUGIN_ICSFEED_TOKEN").is_err());
    }
}
