//! A plugin's manifest: its component, its schema and its grant.

use std::collections::BTreeSet;

use serde::Deserialize;

use super::{BundlePath, CredentialName, Origin, PluginName};
use crate::source_key::SourceKey;

/// One `[[plugins]]` table, parsed: the origins it may reach and the credentials it may name.
///
/// Deserialising is the one way to build one, through `Raw`'s `TryFrom`, so a value of this
/// type cannot hold an origin or a credential the file did not declare.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Raw")]
pub struct Manifest {
    name: PluginName,
    component: BundlePath,
    schema: BundlePath,
    origins: BTreeSet<Origin>,
    credentials: BTreeSet<CredentialName>,
}

/// The table as the file spells it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    component: String,
    schema: String,
    #[serde(default)]
    origins: Vec<String>,
    #[serde(default)]
    credentials: Vec<String>,
}

/// Why a `[[plugins]]` table was refused; it names the offending text, and a manifest holds no
/// secret to leak.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// Not `[a-z][a-z0-9]{0,31}`.
    #[error("{0:?} is not a plugin name (a lowercase letter, then lowercase letters or digits)")]
    BadName(String),
    /// Not a relative `.wasm` path of plain segments.
    #[error("{0:?} is not a component path (relative, plain segments, ending in .wasm)")]
    BadComponent(String),
    /// Not a relative `.schema` path of plain segments.
    #[error("{0:?} is not a schema path (relative, plain segments, ending in .schema)")]
    BadSchema(String),
    /// Not `http(s)://host[:port]`.
    #[error("{0:?} is not an origin (http(s)://host[:port], nothing after it)")]
    BadOrigin(String),
    /// An origin named twice.
    #[error("{0} is allowed twice")]
    DuplicateOrigin(String),
    /// A credential outside `PLUGIN_<NAME>_`.
    #[error("{name:?} is not a credential this plugin may name: it must start with {prefix}")]
    CredentialOutsideNamespace {
        /// The name the manifest gave.
        name: String,
        /// The namespace it had to be in.
        prefix: String,
    },
    /// A credential declared twice.
    #[error("{0} is declared twice")]
    DuplicateCredential(String),
}

impl TryFrom<Raw> for Manifest {
    type Error = ManifestError;

    fn try_from(raw: Raw) -> Result<Self, Self::Error> {
        let name = PluginName::parse(&raw.name)?;
        let mut origins = BTreeSet::new();
        for text in &raw.origins {
            let origin = Origin::parse(text)?;
            if !origins.insert(origin.clone()) {
                return Err(ManifestError::DuplicateOrigin(origin.to_string()));
            }
        }
        let mut manifest = Self {
            component: BundlePath::parse(&raw.component, "wasm")
                .ok_or(ManifestError::BadComponent(raw.component))?,
            schema: BundlePath::parse(&raw.schema, "schema")
                .ok_or(ManifestError::BadSchema(raw.schema))?,
            name,
            origins,
            credentials: BTreeSet::new(),
        };
        for credential in &raw.credentials {
            manifest = manifest.with_link(credential)?;
        }
        Ok(manifest)
    }
}

impl Manifest {
    /// The manifest of a bundle with no credential, through the one door the file's tables go
    /// through: what a template declares when the instance has none of its plugin.
    ///
    /// # Errors
    /// The [`ManifestError`] the file's table would be refused with.
    pub fn declare(
        name: &str,
        component: &str,
        schema: &str,
        origins: &[&str],
    ) -> Result<Self, ManifestError> {
        Self::try_from(Raw {
            name: name.to_owned(),
            component: component.to_owned(),
            schema: schema.to_owned(),
            origins: origins.iter().map(|origin| (*origin).to_owned()).collect(),
            credentials: Vec::new(),
        })
    }

    /// This manifest with one more credential, refused as the file would refuse it.
    ///
    /// # Errors
    /// A name outside its namespace, or one it already declares.
    pub fn with_link(&self, name: &str) -> Result<Self, ManifestError> {
        let credential = CredentialName::parse(&self.name, name)?;
        let mut credentials = self.credentials.clone();
        if !credentials.insert(credential) {
            return Err(ManifestError::DuplicateCredential(name.to_owned()));
        }
        Ok(Self {
            credentials,
            ..self.clone()
        })
    }

    /// This manifest without the credential `name`, the same manifest when it declares none.
    #[must_use]
    pub fn without_credential(&self, name: &str) -> Self {
        Self {
            credentials: self
                .credentials
                .iter()
                .filter(|declared| declared.as_str() != name)
                .cloned()
                .collect(),
            ..self.clone()
        }
    }

    /// This manifest allowed `origin` too; allowing twice is allowing once.
    #[must_use]
    pub fn with_origin(&self, origin: Origin) -> Self {
        let mut origins = self.origins.clone();
        origins.insert(origin);
        Self {
            origins,
            ..self.clone()
        }
    }

    /// The plugin's name, and its credentials' namespace.
    #[must_use]
    pub fn name(&self) -> &PluginName {
        &self.name
    }

    /// Where its component lives, relative to the directory the binary resolves it in.
    #[must_use]
    pub fn component(&self) -> &BundlePath {
        &self.component
    }

    /// Where its declared schema lives, beside its component.
    #[must_use]
    pub fn schema(&self) -> &BundlePath {
        &self.schema
    }

    /// Whether a request to `origin` may be sent at all.
    #[must_use]
    pub fn allows(&self, origin: &Origin) -> bool {
        self.origins.contains(origin)
    }

    /// Every origin it may reach.
    pub fn origins(&self) -> impl Iterator<Item = &Origin> {
        self.origins.iter()
    }

    /// The declared credential a plugin named, or `None` for any name it did not declare,
    /// including every name outside its namespace.
    #[must_use]
    pub fn declared(&self, name: &str) -> Option<&CredentialName> {
        self.credentials
            .iter()
            .find(|declared| declared.as_str() == name)
    }

    /// Every credential it declares.
    pub fn credentials(&self) -> impl Iterator<Item = &CredentialName> {
        self.credentials.iter()
    }

    /// The source its runs are recorded under.
    #[must_use]
    pub fn ran(&self) -> SourceKey {
        SourceKey::Plugin(self.name.clone())
    }

    /// The source the fetches of its `credential`'s link are recorded under.
    #[must_use]
    pub fn link(&self, credential: &CredentialName) -> SourceKey {
        SourceKey::Link {
            plugin: self.name.clone(),
            credential: credential.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct Instance {
        plugins: Vec<Manifest>,
    }

    fn parse(toml: &str) -> Result<Manifest, String> {
        toml::from_str::<Instance>(toml)
            .map(|mut instance| instance.plugins.remove(0))
            .map_err(|failed| failed.message().to_owned())
    }

    const CALENDAR: &str = r#"
        [[plugins]]
        name = "calendar"
        component = "plugins/ics.wasm"
        schema = "plugins/ics.schema"
        origins = ["https://calendar.google.com", "https://api.example.com"]
        credentials = ["PLUGIN_CALENDAR_HOME", "PLUGIN_CALENDAR_WORK"]
    "#;

    /// The grant is the security boundary of the manifest: each way the file could reach past
    /// its own origins and its own namespace is refused when the file is read.
    #[test]
    fn every_overreach_is_refused_when_the_file_is_read() {
        let manifest = parse(CALENDAR).expect("the calendar manifest");
        assert!(manifest.allows(&Origin::parse("https://api.example.com").expect("an origin")));
        assert!(!manifest.allows(&Origin::parse("https://example.com").expect("an origin")));
        assert!(manifest.declared("PLUGIN_CALENDAR_WORK").is_some());
        assert_eq!(manifest.declared("MODEL_TOKEN"), None);
        for (from, to, said) in [
            (
                "PLUGIN_CALENDAR_HOME",
                "MODEL_TOKEN",
                "must start with PLUGIN_CALENDAR_",
            ),
            (
                "PLUGIN_CALENDAR_WORK",
                "PLUGIN_CALENDAR_HOME",
                "declared twice",
            ),
            ("plugins/ics.wasm", "../ics.wasm", "not a component path"),
            ("plugins/ics.schema", "../ics.schema", "not a schema path"),
            (
                "https://api.example.com\"",
                "https://calendar.google.com:443\"",
                "allowed twice",
            ),
            (
                "https://api.example.com\"",
                "https://api.example.com/feed\"",
                "not an origin",
            ),
            (
                "name = \"calendar\"",
                "name = \"calendar\"\nsockets = true",
                "unknown field",
            ),
            (
                "name = \"calendar\"",
                "name = \"calendar\"\nworld = \"source@0.3.0\"",
                "unknown field",
            ),
        ] {
            let toml = CALENDAR.replacen(from, to, 1);
            assert_ne!(toml, CALENDAR, "{from:?} is not in the fixture");
            let refused = parse(&toml).expect_err(&format!("{from:?} -> {to:?}"));
            assert!(refused.contains(said), "{refused}");
        }
    }
}
