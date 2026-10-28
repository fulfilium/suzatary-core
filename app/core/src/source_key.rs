//! What the box reads from, by name: the key its health register holds each source's results
//! under.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::mail::account_email::AccountEmail;
use crate::plugin::{CredentialName, PluginName};

/// A source the box reads from. Its print is its name in the health register and parses back to
/// it: `mail:ada@example.test`, `plugin:ics`, `link:ics/PLUGIN_ICS_WORK`,
/// `google-calendar:ada@example.test`, `drive`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum SourceKey {
    /// A mail account's sync.
    Mail(AccountEmail),
    /// A plugin's runs: whether what each reported was stored.
    Plugin(PluginName),
    /// A plugin's fetches of one credential's link.
    Link {
        /// The plugin.
        plugin: PluginName,
        /// The credential, in the plugin's namespace.
        credential: CredentialName,
    },
    /// A Google account's calendars, read through Google's Calendar API.
    GoogleCalendar(AccountEmail),
    /// The Drive folders the secretary may read, listed together.
    Drive,
}

/// Text that names no source.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a source: {0:?}")]
pub struct NotASource(String);

impl SourceKey {
    /// Parse a print.
    ///
    /// # Errors
    /// [`NotASource`] for text no source prints as.
    pub fn parse(text: &str) -> Result<Self, NotASource> {
        let parsed = match text.split_once(':') {
            None if text == "drive" => Some(Self::Drive),
            Some(("mail", rest)) => AccountEmail::new(rest).ok().map(Self::Mail),
            Some(("google-calendar", rest)) => {
                AccountEmail::new(rest).ok().map(Self::GoogleCalendar)
            }
            Some(("plugin", rest)) => PluginName::parse(rest).ok().map(Self::Plugin),
            Some(("link", rest)) => rest.split_once('/').and_then(|(plugin, credential)| {
                let plugin = PluginName::parse(plugin).ok()?;
                let credential = CredentialName::parse(&plugin, credential).ok()?;
                Some(Self::Link { plugin, credential })
            }),
            _ => None,
        };
        parsed.ok_or_else(|| NotASource(text.to_owned()))
    }
}

impl fmt::Display for SourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mail(email) => write!(f, "mail:{email}"),
            Self::Plugin(plugin) => write!(f, "plugin:{plugin}"),
            Self::Link { plugin, credential } => write!(f, "link:{plugin}/{credential}"),
            Self::GoogleCalendar(email) => write!(f, "google-calendar:{email}"),
            Self::Drive => f.write_str("drive"),
        }
    }
}

impl TryFrom<String> for SourceKey {
    type Error = NotASource;
    fn try_from(text: String) -> Result<Self, NotASource> {
        Self::parse(&text)
    }
}

impl From<SourceKey> for String {
    fn from(key: SourceKey) -> String {
        key.to_string()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn key() -> impl Strategy<Value = SourceKey> {
        let email = "[a-z0-9.]{1,8}@[a-z]{1,6}\\.(test|example)"
            .prop_map(|text| AccountEmail::new(&text).expect("a lower-case address"));
        let plugin =
            "[a-z][a-z0-9]{0,8}".prop_map(|name| PluginName::parse(&name).expect("a name"));
        let link = (plugin.clone(), "[A-Z0-9_]{1,12}").prop_map(|(plugin, label)| {
            let credential =
                CredentialName::parse(&plugin, &format!("{}{label}", plugin.credential_prefix()))
                    .expect("a credential in its namespace");
            SourceKey::Link { plugin, credential }
        });
        prop_oneof![
            email.clone().prop_map(SourceKey::Mail),
            email.prop_map(SourceKey::GoogleCalendar),
            plugin.prop_map(SourceKey::Plugin),
            link,
            Just(SourceKey::Drive),
        ]
    }

    proptest! {
        /// The register's file names each source by its print, so a print parses back to the
        /// source it printed, as JSON's map key too.
        #[test]
        fn a_source_survives_print_and_parse(key in key()) {
            prop_assert_eq!(SourceKey::parse(&key.to_string()), Ok(key.clone()));
            let keyed = std::collections::BTreeMap::from([(key.clone(), 1)]);
            let json = serde_json::to_string(&keyed).expect("prints");
            prop_assert_eq!(
                serde_json::from_str::<std::collections::BTreeMap<SourceKey, i32>>(&json)
                    .expect("parses"),
                keyed
            );
        }
    }

    /// Text that is no print is refused, a credential outside its plugin's namespace included.
    #[test]
    fn text_that_no_source_prints_is_refused() {
        for text in [
            "",
            "drive:",
            "ada@example.test",
            "mail:Ada@example.test",
            "link:ics",
            "plugin:",
            "link:ics/PLUGIN_OTHER_WORK",
            "calendar:ada@example.test",
        ] {
            assert!(SourceKey::parse(text).is_err(), "{text:?} parsed");
        }
    }
}
