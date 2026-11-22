//! The closed list of secret names this app may write, derived from the instance file and never
//! from `secretspec.toml`, so release and mail-transport secrets are refused by absence.

mod declaration;
mod help;
pub mod remote;

use std::collections::{BTreeMap, BTreeSet};

use crate::connect;

pub use declaration::Declaration;
pub use help::{Help, Page, Words};
use suzatary_services::plugin::Manifest;
use suzatary_services::repos::Declared;

#[cfg(any(test, feature = "nothing"))]
pub const MODEL_TOKEN: &str = "MODEL_TOKEN";

/// The data repository's push token, scoped to that repository, never the code's.
pub const DATA_PUSH_TOKEN: &str = "SUZATARY_DATA_PUSH_TOKEN";

/// Every declared secret name with what declares it; a join-semilattice under pointwise union.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settables(BTreeMap<String, BTreeSet<Declaration>>);

impl Settables {
    #[must_use]
    pub fn declared(name: &str, by: Declaration) -> Self {
        Self(BTreeMap::from([(name.to_owned(), BTreeSet::from([by]))]))
    }

    #[must_use]
    pub fn join(mut self, other: Self) -> Self {
        for (name, by) in other.0 {
            self.0.entry(name).or_default().extend(by);
        }
        self
    }

    /// Every name a deployment may set; a server re-derives with this one function, so no name
    /// drops out after a tap edits the instance file.
    #[must_use]
    pub fn of_deployment(
        model: &str,
        channels: impl IntoIterator<Item = connect::Channel>,
        coding: Option<&connect::Coding>,
        (plugins, accounts): (&[Manifest], &[suzatary_mail::mail::account::MailAccount]),
        repos: &Declared,
    ) -> Self {
        Self::derive(model, channels, coding, plugins)
            .join(Self::github(repos))
            .join(Self::google(accounts))
    }

    /// A manifest declares names only in its own `PLUGIN_<NAME>_` namespace.
    #[must_use]
    pub fn derive(
        model: &str,
        channels: impl IntoIterator<Item = connect::Channel>,
        coding: Option<&connect::Coding>,
        plugins: &[Manifest],
    ) -> Self {
        let model = (model.to_owned(), Declaration::Model);
        let routine = coding.map(|_| (connect::ROUTINE_TOKEN.to_owned(), Declaration::Coding));
        let notified = channels.into_iter().flat_map(|channel| {
            crate::connect::channel_secrets(channel)
                .into_iter()
                .map(move |secret| (secret.to_owned(), Declaration::Channel(channel)))
        });
        let credentials = plugins.iter().flat_map(|manifest| {
            manifest.credentials().map(|name| {
                (
                    name.as_str().to_owned(),
                    Declaration::Plugin(manifest.name().clone()),
                )
            })
        });
        std::iter::once(model)
            .chain(routine)
            .chain(notified)
            .chain(credentials)
            .map(|(name, by)| Self::declared(&name, by))
            .fold(Self::default(), Self::join)
    }

    /// Any account may hold a grant for its Google calendar, however its mail is read.
    #[must_use]
    pub fn google(accounts: &[suzatary_mail::mail::account::MailAccount]) -> Self {
        let addresses: Vec<&suzatary_mail::mail::address::EmailAddress> = accounts
            .iter()
            .map(|account| account.email.address())
            .collect();
        let client = addresses.first().map(|_| {
            Self::declared(
                suzatary_google::google::CLIENT_ID,
                Declaration::GoogleClient,
            )
            .join(Self::declared(
                suzatary_google::google::CLIENT_SECRET,
                Declaration::GoogleClient,
            ))
        });
        addresses
            .iter()
            .map(|account| {
                Self::declared(
                    &suzatary_google::google::grant_name(account),
                    Declaration::GoogleGrant(account.as_str().to_owned()),
                )
            })
            .chain(client)
            .fold(Self::default(), Self::join)
    }

    /// The GitHub tokens, one per purpose so no token does another's job.
    #[must_use]
    pub fn github(repos: &Declared) -> Self {
        let reached = repos
            .iter()
            .next()
            .map(|_| Self::declared(suzatary_services::repos::TOKEN, Declaration::Repos));
        reached.into_iter().fold(
            Self::declared(DATA_PUSH_TOKEN, Declaration::DataPush),
            Self::join,
        )
    }

    /// A homomorphism of the join.
    #[must_use]
    pub fn restricted(&self, keep: impl Fn(&Declaration) -> bool) -> Self {
        Self(
            self.0
                .iter()
                .filter_map(|(name, by)| {
                    let kept: BTreeSet<Declaration> =
                        by.iter().filter(|d| keep(d)).cloned().collect();
                    (!kept.is_empty()).then(|| (name.clone(), kept))
                })
                .collect(),
        )
    }

    /// Sorted.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.0.keys().map(String::as_str).collect()
    }

    /// Sorted.
    pub fn iter(&self) -> impl Iterator<Item = Settable> + '_ {
        self.0.keys().cloned().map(Settable)
    }

    pub fn declarations(&self) -> impl Iterator<Item = (Settable, &BTreeSet<Declaration>)> + '_ {
        self.0.iter().map(|(name, by)| (Settable(name.clone()), by))
    }

    #[must_use]
    pub fn model(&self) -> Option<Settable> {
        self.declarations()
            .find(|(_, by)| by.contains(&Declaration::Model))
            .map(|(name, _)| name)
    }

    #[must_use]
    pub fn by_handle(&self, handle: &str) -> Option<Settable> {
        self.iter().find(|name| name.handle() == handle)
    }

    #[must_use]
    pub fn declaration(&self, name: &Settable) -> Option<&BTreeSet<Declaration>> {
        self.0.get(name.name())
    }
}

/// A declared secret name, the only name a write route accepts; [`Settable::mint`] is its only
/// constructor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Settable(String);

impl Settable {
    pub fn mint(declared: &Settables, candidate: &str) -> Result<Self, SettableError> {
        if declared.0.contains_key(candidate) {
            Ok(Self(candidate.to_owned()))
        } else {
            Err(SettableError::Undeclared(candidate.to_owned()))
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }

    /// A digest of the name, so the name reaches no page.
    #[must_use]
    pub fn handle(&self) -> String {
        use sha2::{Digest as _, Sha256};
        let digest = Sha256::digest(format!("suzatary settable v1\0{}", self.0));
        hex::encode(&digest[..8])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettableError {
    #[error("{0:?} is not a name this deployment has declared it needs")]
    Undeclared(String),
}

/// Writes a secret; held in no capability bundle, so no verb reaches it.
pub trait Writes {
    /// # Errors
    /// Never carries the value.
    fn write_secret(
        &self,
        name: &Settable,
        value: &suzatary_base::capability::Secret,
    ) -> Result<(), String>;

    fn clear_secret(&self, name: &Settable) -> Result<(), String>;
}

/// Declares names to the store, which refuses a name it was never told of.
pub trait Declares {
    /// Declares exactly `names` beside the store's own; on error, earlier names stay declared.
    fn declare(&self, names: &Settables) -> Result<(), String>;
}

/// One test call before a model's credential is kept; held in no capability record.
pub trait Verify {
    /// `Ok` if no model reads `name`.
    ///
    /// # Errors
    /// Never carries the value.
    fn verify(
        &self,
        name: &Settable,
        value: &suzatary_base::capability::Secret,
    ) -> Result<(), String>;
}

/// Reads a source once, when an act makes it readable; held in no capability record.
pub trait Checks {
    fn check(&self, source: &suzatary_health::health::SourceKey);
}

/// Edits the instance file; held in no capability bundle.
pub trait Edits: Configuration {
    /// Idempotent.
    fn allow_web(&self, rule: &suzatary_services::web::AllowRule) -> Result<(), EditError>;

    /// Idempotent.
    fn allow_drive(&self, folder: &suzatary_base::drive_path::DrivePath) -> Result<(), EditError>;

    /// Keeps every other byte, so a remove undoes it.
    fn add_connection(&self, connection: &connect::Connection) -> Result<(), EditError>;

    /// Keeps every other byte.
    fn remove_connection(&self, connection: &connect::Connection) -> Result<(), EditError>;

    /// Idempotent.
    fn add_origin(
        &self,
        plugin: &suzatary_health::health::plugin::PluginName,
        origin: &suzatary_health::health::plugin::Origin,
    ) -> Result<(), EditError>;
}

/// Reads the connection tables; a read alone, so a record that files cards may hold it.
pub trait Configuration: Send + Sync {
    /// The tables as the file declares them now, not as read at start.
    fn declared(&self) -> Result<connect::Declared, EditError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("could not edit the instance file: {0}")]
    Io(String),
    #[error("the instance file is malformed: {0}")]
    Malformed(String),
    /// The edit touched nothing.
    #[error(transparent)]
    Connection(#[from] suzatary_health::health::Refused),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> Settables {
        Settables::derive(MODEL_TOKEN, connect::Channel::ALL, None, &[])
    }

    #[test]
    fn every_declared_name_mints() {
        let declared = declared();
        for name in [MODEL_TOKEN, "FCM_SERVICE_ACCOUNT_B64", "NTFY_TOPIC"] {
            assert_eq!(
                Settable::mint(&declared, name).as_ref().map(Settable::name),
                Ok(name),
                "{name} should have minted"
            );
        }
    }

    #[test]
    fn the_five_forbidden_names_are_always_refused() {
        let declared = declared();
        for name in [
            "GITHUB_PAT",
            "ANDROID_KEYSTORE_B64",
            "ANDROID_KEYSTORE_PASSWORD",
            "PROTON_BRIDGE_PASSWORD",
            "SUZATARY_GMAIL_PASSWORD",
        ] {
            assert_eq!(
                Settable::mint(&declared, name),
                Err(SettableError::Undeclared(name.to_owned())),
                "{name} must never be settable"
            );
        }
    }

    #[test]
    fn an_arbitrary_name_is_refused() {
        assert_eq!(
            Settable::mint(&declared(), "WHATEVER_I_TYPE"),
            Err(SettableError::Undeclared("WHATEVER_I_TYPE".to_owned()))
        );
    }

    #[test]
    fn an_empty_deployment_still_settles_the_models_credential() {
        let declared = Settables::derive(MODEL_TOKEN, std::iter::empty(), None, &[]);
        assert_eq!(declared.names(), vec![MODEL_TOKEN]);
        assert!(Settable::mint(&declared, "GITHUB_PAT").is_err());
    }

    /// One token per purpose: the data push always, the repositories' token only when declared.
    #[test]
    fn each_github_token_is_declared_for_its_own_purpose() {
        let none = Settables::github(&suzatary_services::repos::Declared::default());
        assert_eq!(none.names(), vec![DATA_PUSH_TOKEN]);
        let repos = suzatary_services::repos::Declared::new([(
            "ada/notes".parse().expect("a name"),
            suzatary_services::repos::RepoTarget::default(),
        )])
        .expect("a list");
        let both = Settables::github(&repos);
        assert_eq!(
            both.names(),
            vec![DATA_PUSH_TOKEN, suzatary_services::repos::TOKEN]
        );
        for name in [DATA_PUSH_TOKEN, suzatary_services::repos::TOKEN] {
            assert!(Settable::mint(&both, name).is_ok(), "{name}");
        }
        assert_ne!(DATA_PUSH_TOKEN, suzatary_services::repos::TOKEN);
    }

    #[test]
    fn a_declared_routine_makes_its_token_settable() {
        let coding = connect::Coding {
            routine: "trig_01fake".parse().expect("a routine id"),
            repo: "bmabsout/Suzatary".parse().expect("a repository"),
        };
        let with = Settables::derive(MODEL_TOKEN, std::iter::empty(), Some(&coding), &[]);
        assert!(Settable::mint(&with, connect::ROUTINE_TOKEN).is_ok());
        let without = Settables::derive(MODEL_TOKEN, std::iter::empty(), None, &[]);
        assert!(Settable::mint(&without, connect::ROUTINE_TOKEN).is_err());
    }

    /// Only the names a manifest declares, in its own namespace, are added.
    #[test]
    fn a_plugin_s_declared_credentials_are_settable_by_name() {
        let manifest: suzatary_services::plugin::Manifest = toml::from_str(
            r#"
            name = "ics"
            component = "ics.wasm"
            schema = "ics.schema"
            credentials = ["PLUGIN_ICS_WORK"]
            "#,
        )
        .expect("a manifest");
        let declared = Settables::derive(MODEL_TOKEN, std::iter::empty(), None, &[manifest]);
        assert!(Settable::mint(&declared, "PLUGIN_ICS_WORK").is_ok());
        assert!(Settable::mint(&declared, "PLUGIN_ICS_HOME").is_err());
        assert_eq!(declared.names().len(), 2);
    }

    #[test]
    fn a_name_says_what_declares_it() {
        let declared = Settables::derive(MODEL_TOKEN, [connect::Channel::Ntfy], None, &[]);
        let by = |name: &str| {
            declared
                .declarations()
                .find(|(settable, _)| settable.name() == name)
                .map(|(_, by)| by.iter().cloned().collect::<Vec<_>>())
        };
        assert_eq!(by(MODEL_TOKEN), Some(vec![Declaration::Model]));
        assert_eq!(
            by("NTFY_TOPIC"),
            Some(vec![Declaration::Channel(connect::Channel::Ntfy)])
        );
    }

    fn generated() -> impl proptest::strategy::Strategy<Value = Settables> {
        use proptest::prelude::*;
        let declaration = prop_oneof![
            Just(Declaration::Coding),
            "[a-c]".prop_map(Declaration::GoogleGrant),
            proptest::sample::select(connect::Channel::ALL.to_vec()).prop_map(Declaration::Channel),
        ];
        proptest::collection::vec(("[A-C]", declaration), 0..6).prop_map(|pairs| {
            pairs
                .into_iter()
                .map(|(name, by)| Settables::declared(&name, by))
                .fold(Settables::default(), Settables::join)
        })
    }

    proptest::proptest! {
        /// Restricting the declarations commutes with the join and keeps all when it keeps all.
        #[test]
        fn restricting_is_a_homomorphism(
            a in generated(),
            b in generated(),
            keep in proptest::sample::subsequence(vec!["a", "b", "c"], 0..=3),
            channels in proptest::sample::subsequence(connect::Channel::ALL.to_vec(), 0..=2),
        ) {
            let k = |by: &Declaration| match by {
                Declaration::GoogleGrant(name) => keep.contains(&name.as_str()),
                Declaration::Channel(channel) => channels.contains(channel),
                Declaration::Model
                | Declaration::Coding
                | Declaration::DataPush
                | Declaration::Repos
                | Declaration::Plugin(_)
                | Declaration::GoogleClient => true,
            };
            proptest::prop_assert_eq!(
                a.clone().join(b.clone()).restricted(k),
                a.restricted(k).join(b.restricted(k))
            );
            proptest::prop_assert_eq!(a.restricted(|_| true), a.clone());
        }

        /// The join is a semilattice with the empty list as its identity.
        #[test]
        fn the_join_is_a_semilattice(a in generated(), b in generated(), c in generated()) {
            proptest::prop_assert_eq!(a.clone().join(b.clone()), b.clone().join(a.clone()));
            proptest::prop_assert_eq!(
                a.clone().join(b.clone()).join(c.clone()),
                a.clone().join(b.clone().join(c))
            );
            proptest::prop_assert_eq!(a.clone().join(a.clone()), a.clone());
            proptest::prop_assert_eq!(a.clone().join(Settables::default()), a);
        }

        /// Deriving is a homomorphism of the join.
        #[test]
        fn deriving_is_a_homomorphism(
            left in proptest::collection::vec(
                proptest::sample::select(connect::Channel::ALL.to_vec()), 0..3),
            right in proptest::collection::vec(
                proptest::sample::select(connect::Channel::ALL.to_vec()), 0..3),
        ) {
            proptest::prop_assert_eq!(
                Settables::derive(MODEL_TOKEN, left.iter().chain(&right).copied(), None, &[]),
                Settables::derive(MODEL_TOKEN, left.iter().copied(), None, &[])
                    .join(Settables::derive(MODEL_TOKEN, right.iter().copied(), None, &[]))
            );
        }
    }
}
