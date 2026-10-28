//! The closed list of secret names this app may write, and the gates and edits around it.
//!
//! The list is derived from what the instance file says the deployment reads, never from
//! `secretspec.toml`, so release and mail-transport secrets are refused by absence.

mod declaration;
mod help;
pub mod overview;
pub mod remote;

use std::collections::{BTreeMap, BTreeSet};

pub use declaration::Declaration;
pub use help::{Help, Page, Words};

/// The name the tests' deployments store their model's credential under.
#[cfg(test)]
pub(crate) const MODEL_TOKEN: &str = "MODEL_TOKEN";

/// The token `scripts/safe-push.sh` pushes the data repository with, through
/// `scripts/git-credential-secretspec.sh`: scoped to that one repository, never the code's.
pub const DATA_PUSH_TOKEN: &str = "SUZATARY_DATA_PUSH_TOKEN";

/// Every secret name this deployment has declared it needs, each with what declares it; the
/// authority for [`Settable::mint`].
///
/// The free join-semilattice on (name, declaration) pairs, read as a map: [`Settables::join`] is
/// the pointwise union, [`Settables::default`] its identity, and [`Settables::derive`] the join
/// of one pair per declaring table, so deriving from two deployments' tables is joining what
/// each derives.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settables(BTreeMap<String, BTreeSet<Declaration>>);

impl Settables {
    /// One name, declared by one table.
    #[must_use]
    pub fn declared(name: &str, by: Declaration) -> Self {
        Self(BTreeMap::from([(name.to_owned(), BTreeSet::from([by]))]))
    }

    /// The join: every name of either, declared by every table either says declares it.
    #[must_use]
    pub fn join(mut self, other: Self) -> Self {
        for (name, by) in other.0 {
            self.0.entry(name).or_default().extend(by);
        }
        self
    }

    /// Every name a deployment may set: what [`Settables::derive`] reads of its tables, the
    /// GitHub tokens its repositories need and what its Google accounts read. The one derivation a server starts and re-derives
    /// with, so a name never drops out after a tap that moved the instance file.
    #[must_use]
    pub fn of_deployment(
        model: &str,
        channels: impl IntoIterator<Item = crate::connect::Channel>,
        coding: Option<&crate::coding::Coding>,
        (plugins, accounts): (
            &[crate::plugin::Manifest],
            &[crate::mail::account::MailAccount],
        ),
        repos: &crate::repos::Declared,
    ) -> Self {
        Self::derive(model, channels, coding, plugins)
            .join(Self::github(repos))
            .join(Self::google(accounts))
    }

    /// Derives the list from `model`, the name the deployment stores its model's credential
    /// under, notify channels, `[coding]` and every
    /// credential a `[[plugins]]` manifest declares (a calendar's link, by name; a manifest can
    /// only name its own `PLUGIN_<NAME>_` namespace, so no plugin widens this to the app's keys).
    ///
    /// The model's credential is always present.
    #[must_use]
    pub fn derive(
        model: &str,
        channels: impl IntoIterator<Item = crate::connect::Channel>,
        coding: Option<&crate::coding::Coding>,
        plugins: &[crate::plugin::Manifest],
    ) -> Self {
        let model = (model.to_owned(), Declaration::Model);
        let routine =
            coding.map(|_| (crate::coding::ROUTINE_TOKEN.to_owned(), Declaration::Coding));
        let notified = channels.into_iter().flat_map(|channel| {
            crate::notify::channel_secrets(channel)
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

    /// What a deployment's Google accounts read: the box's one Google Cloud client while any is
    /// declared, and each account's own sign-in with Google. Each is asked for by its own card,
    /// never a Secret card ([`Declaration::own_card`]).
    #[must_use]
    pub fn google(accounts: &[crate::mail::account::MailAccount]) -> Self {
        let addresses: Vec<&crate::mail::address::EmailAddress> = accounts
            .iter()
            .filter(|account| matches!(account.sync, crate::mail::account::Sync::Google))
            .map(|account| account.email.address())
            .collect();
        let client = addresses.first().map(|_| {
            Self::declared(crate::google::CLIENT_ID, Declaration::GoogleClient).join(
                Self::declared(crate::google::CLIENT_SECRET, Declaration::GoogleClient),
            )
        });
        addresses
            .iter()
            .map(|account| {
                Self::declared(
                    &crate::google::grant_name(account),
                    Declaration::GoogleGrant(account.as_str().to_owned()),
                )
            })
            .chain(client)
            .fold(Self::default(), Self::join)
    }

    /// A deployment's GitHub tokens, one per purpose so no token does another's job: the data
    /// repository's push, which every deployment makes, and, when the secretary reaches
    /// `repos`, the token it reaches them with.
    #[must_use]
    pub fn github(repos: &crate::repos::Declared) -> Self {
        let reached = repos
            .iter()
            .next()
            .map(|_| Self::declared(crate::repos::TOKEN, Declaration::Repos));
        reached.into_iter().fold(
            Self::declared(DATA_PUSH_TOKEN, Declaration::DataPush),
            Self::join,
        )
    }

    /// The declarations `keep` keeps, a name none of its declarations kept being dropped: the
    /// restriction, a homomorphism of the join.
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

    /// Every declared name, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.0.keys().map(String::as_str).collect()
    }

    /// Every declared name as a [`Settable`], sorted; a member is settable by construction.
    pub fn iter(&self) -> impl Iterator<Item = Settable> + '_ {
        self.0.keys().cloned().map(Settable)
    }

    /// Every declared name as a [`Settable`], with what declares it.
    pub fn declarations(&self) -> impl Iterator<Item = (Settable, &BTreeSet<Declaration>)> + '_ {
        self.0.iter().map(|(name, by)| (Settable(name.clone()), by))
    }

    /// The name the model's credential is stored under, which the deployment chose.
    #[must_use]
    pub fn model(&self) -> Option<Settable> {
        self.declarations()
            .find(|(_, by)| by.contains(&Declaration::Model))
            .map(|(name, _)| name)
    }

    /// The tables that declare `name`, `None` for a name no longer declared.
    #[must_use]
    pub fn declaration(&self, name: &Settable) -> Option<&BTreeSet<Declaration>> {
        self.0.get(name.name())
    }
}

/// A secret name this deployment declared, the only name a write route accepts.
///
/// [`Settable::mint`] is deliberately its only constructor, with no `From<String>` and no
/// reader: a card stores the name as text, and its tap mints it against the names declared then.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Settable(String);

impl Settable {
    /// Mint one against the declared list, or say which name was refused.
    ///
    /// # Errors
    /// [`SettableError::Undeclared`] when `candidate` is not a member of `declared`.
    pub fn mint(declared: &Settables, candidate: &str) -> Result<Self, SettableError> {
        if declared.0.contains_key(candidate) {
            Ok(Self(candidate.to_owned()))
        } else {
            Err(SettableError::Undeclared(candidate.to_owned()))
        }
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0
    }
}

/// Why a candidate name did not mint a [`Settable`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettableError {
    /// Not a name this deployment declared.
    #[error("{0:?} is not a name this deployment has declared it needs")]
    Undeclared(String),
}

/// Writes a secret through the one store; held in no capability bundle, so no verb reaches it.
pub trait Writes {
    /// Writes `name` to `value`: a name this deployment declares, so the store's other secrets
    /// cannot be named.
    ///
    /// # Errors
    /// The store's own words, summarised, never carrying the value.
    fn write_secret(
        &self,
        name: &Settable,
        value: &crate::capability::Secret,
    ) -> Result<(), String>;

    /// Deletes what is stored under `name`, so a removed connection's secret resolves no more.
    ///
    /// # Errors
    /// The store's own words, summarised.
    fn clear_secret(&self, name: &Settable) -> Result<(), String>;
}

/// Declares to the one store every name a deployment may set, since the store refuses a name
/// it was never told of; held in no capability record, like [`Writes`].
pub trait Declares {
    /// Declares exactly `names` beside the store's own, so every [`Writes`] and every read
    /// through the store takes each of them from now on.
    ///
    /// # Errors
    /// Why they could not be declared; the names declared before stay declared.
    fn declare(&self, names: &Settables) -> Result<(), String>;
}

/// One test call before a model's credential is kept; held in no capability record, like
/// [`Writes`], so only a tap reaches it.
pub trait Verify {
    /// Whether `value`, kept under `name`, answers one call of the model that reads it; `Ok` for
    /// a name the model does not read, since there is nothing to call.
    ///
    /// # Errors
    /// Why the call did not answer, in words that never carry the value.
    fn verify(&self, name: &Settable, value: &crate::capability::Secret) -> Result<(), String>;
}

/// What runs a plugin's sync once, at the tap that stored one of its links, so the link's line
/// says what its check found then and not at the plugin's next timer; held in no capability
/// record, as [`Verify`] is not.
pub trait Checks {
    /// Sync `plugin` once: every link of it is fetched, and what each came to joins its health.
    fn check(&self, plugin: &crate::plugin::PluginName);
}

/// Edits the instance file; held in no capability bundle, like [`Writes`].
pub trait Edits: Configuration {
    /// Lets the secretary read what `rule` covers without asking; allowing twice is no error.
    ///
    /// # Errors
    /// [`EditError::Io`] or [`EditError::Malformed`].
    fn allow_web(&self, rule: &crate::web::AllowRule) -> Result<(), EditError>;

    /// Lets the secretary read files under `folder` without asking; allowing twice is no error.
    ///
    /// # Errors
    /// [`EditError::Io`] or [`EditError::Malformed`].
    fn allow_drive(&self, folder: &crate::drive::DrivePath) -> Result<(), EditError>;

    /// Writes `connection`'s table, keeping every other byte of the file: an add followed by
    /// its remove leaves the file as it was.
    ///
    /// # Errors
    /// [`EditError::Connection`] for what the algebra refuses (a duplicate first), else
    /// [`EditError::Io`] or [`EditError::Malformed`].
    fn add_connection(&self, connection: &crate::connect::Connection) -> Result<(), EditError>;

    /// Takes `connection`'s table out, the same way.
    ///
    /// # Errors
    /// As [`Edits::add_connection`].
    fn remove_connection(&self, connection: &crate::connect::Connection) -> Result<(), EditError>;

    /// Lets `plugin` reach `origin`; granting twice is no error.
    ///
    /// # Errors
    /// As [`Edits::add_connection`].
    fn add_origin(
        &self,
        plugin: &crate::plugin::PluginName,
        origin: &crate::plugin::Origin,
    ) -> Result<(), EditError>;
}

/// Reads the tables connections are read from, from the instance file as it is now; a read
/// alone, so a record that files cards may hold it.
pub trait Configuration: Send + Sync {
    /// The tables, as the file declares them now, so a tap acts on the file rather than on what
    /// a process read when it started, and a Connect card is filed only for what the file does
    /// not hold yet.
    ///
    /// # Errors
    /// [`EditError::Io`] or [`EditError::Malformed`].
    fn declared(&self) -> Result<crate::connect::Declared, EditError>;
}

/// Why an edit to the instance file did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// The file could not be read or written.
    #[error("could not edit the instance file: {0}")]
    Io(String),
    /// The file did not parse as TOML, or as the document this edit expects.
    #[error("the instance file is malformed: {0}")]
    Malformed(String),
    /// The connection algebra refused the edit, which touched nothing.
    #[error(transparent)]
    Connection(#[from] crate::connect::Refused),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> Settables {
        Settables::derive(MODEL_TOKEN, crate::connect::Channel::ALL, None, &[])
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

    /// Pinned against the widest list this module can build.
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

    /// One token per purpose: the data repository's push always, the repositories' token only
    /// where the secretary reaches one, and neither is the other's name.
    #[test]
    fn each_github_token_is_declared_for_its_own_purpose() {
        let none = Settables::github(&crate::repos::Declared::default());
        assert_eq!(none.names(), vec![DATA_PUSH_TOKEN]);
        let repos = crate::repos::Declared::new([(
            "ada/notes".parse().expect("a name"),
            crate::repos::RepoTarget::default(),
        )])
        .expect("a list");
        let both = Settables::github(&repos);
        assert_eq!(both.names(), vec![DATA_PUSH_TOKEN, crate::repos::TOKEN]);
        for name in [DATA_PUSH_TOKEN, crate::repos::TOKEN] {
            assert!(Settable::mint(&both, name).is_ok(), "{name}");
        }
        assert_ne!(DATA_PUSH_TOKEN, crate::repos::TOKEN);
    }

    #[test]
    fn a_declared_routine_makes_its_token_settable() {
        let coding = crate::coding::Coding {
            routine: "trig_01fake".parse().expect("a routine id"),
            repo: "bmabsout/Suzatary".parse().expect("a repository"),
            chain: crate::chains::ChainName::new("suzatary").expect("a chain"),
        };
        let with = Settables::derive(MODEL_TOKEN, std::iter::empty(), Some(&coding), &[]);
        assert!(Settable::mint(&with, crate::coding::ROUTINE_TOKEN).is_ok());
        let without = Settables::derive(MODEL_TOKEN, std::iter::empty(), None, &[]);
        assert!(Settable::mint(&without, crate::coding::ROUTINE_TOKEN).is_err());
    }

    /// A calendar's link is set by name like any other secret, and only the names its manifest
    /// declares, in its own namespace, are added.
    #[test]
    fn a_plugin_s_declared_credentials_are_settable_by_name() {
        let manifest: crate::plugin::Manifest = toml::from_str(
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

    /// A name says what declares it, and the model's credential its model.
    #[test]
    fn a_name_says_what_declares_it() {
        let declared = Settables::derive(MODEL_TOKEN, [crate::connect::Channel::Ntfy], None, &[]);
        let by = |name: &str| {
            declared
                .declarations()
                .find(|(settable, _)| settable.name() == name)
                .map(|(_, by)| by.iter().cloned().collect::<Vec<_>>())
        };
        assert_eq!(by(MODEL_TOKEN), Some(vec![Declaration::Model]));
        assert_eq!(
            by("NTFY_TOPIC"),
            Some(vec![Declaration::Channel(crate::connect::Channel::Ntfy)])
        );
    }

    fn generated() -> impl proptest::strategy::Strategy<Value = Settables> {
        use proptest::prelude::*;
        let declaration = prop_oneof![
            Just(Declaration::Coding),
            "[a-c]".prop_map(Declaration::GoogleGrant),
            proptest::sample::select(crate::connect::Channel::ALL.to_vec())
                .prop_map(Declaration::Channel),
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
            channels in proptest::sample::subsequence(crate::connect::Channel::ALL.to_vec(), 0..=2),
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

        /// Deriving is a homomorphism: the tables of two deployments derive the join of what
        /// each derives.
        #[test]
        fn deriving_is_a_homomorphism(
            left in proptest::collection::vec(
                proptest::sample::select(crate::connect::Channel::ALL.to_vec()), 0..3),
            right in proptest::collection::vec(
                proptest::sample::select(crate::connect::Channel::ALL.to_vec()), 0..3),
        ) {
            proptest::prop_assert_eq!(
                Settables::derive(MODEL_TOKEN, left.iter().chain(&right).copied(), None, &[]),
                Settables::derive(MODEL_TOKEN, left.iter().copied(), None, &[])
                    .join(Settables::derive(MODEL_TOKEN, right.iter().copied(), None, &[]))
            );
        }
    }
}
