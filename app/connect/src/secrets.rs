//! Where a secret stands, never what it is: the only answer this crate can see is a
//! [`SecretState`].

use crate::settings::Settable;
use suzatary_google::google::Held;
use suzatary_mail::mail::account::{MailAccount, Sync};

/// One account's OAuth consents: Microsoft's IMAP and SMTP grants sit in separate slots, so a
/// leaked IMAP token cannot send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MailGrants {
    /// The IMAP slot, or for a Google account its sign-in with Google.
    pub imap: bool,
    pub smtp: bool,
}

/// Where one mail account's credential lives, so an account whose grant this box does not hold is
/// never shown as broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "MailCredentialRow")]
pub enum MailCredential {
    /// Microsoft over IMAP and XOAUTH2, whose grants live in this box's token store.
    Slots {
        /// The IMAP grant.
        imap: bool,
        /// The SMTP grant, held separately.
        smtp: bool,
    },
    /// Gmail through lieer, which keeps its own credential outside this box's token store.
    Lieer,
    /// The box keeps the sign-in with Google in its secret store.
    Google {
        /// Whether the box holds it.
        granted: bool,
    },
    /// Proton through the local Bridge, which keeps its own credential.
    Bridge,
}

impl MailCredential {
    #[must_use]
    pub fn of(account: &MailAccount, grants: MailGrants) -> Self {
        match &account.sync {
            Sync::MsImap(_) => MailCredential::Slots {
                imap: grants.imap,
                smtp: grants.smtp,
            },
            Sync::Gmail => MailCredential::Lieer,
            Sync::Google => MailCredential::Google {
                granted: grants.imap,
            },
            Sync::Proton(_) => MailCredential::Bridge,
        }
    }
}

/// Where a secret stands, with the value dropped.
///
/// A locked store is not an unset name: reading it as unset asks the owner to set a secret again.
/// A store that cannot be run is not locked either: unlocking it mends nothing.
/// The order `Missing < Failed < Locked < Set` makes [`SecretState::and`] the meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum SecretState {
    Missing,
    /// The store could not be run, or answered with something that is not a value.
    Failed,
    Locked,
    Set,
}

impl SecretState {
    /// "Every one is set": the meet, whose unit is [`SecretState::Set`].
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        self.min(other)
    }

    #[must_use]
    pub fn all(states: impl IntoIterator<Item = Self>) -> Self {
        states.into_iter().fold(Self::Set, Self::and)
    }

    #[must_use]
    pub fn is_set(self) -> bool {
        self == Self::Set
    }
}

/// The store is locked: an answer about the store, never about a name, and it costs a timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locked;

/// What the store answered for one name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Missing,
    Failed,
    Set,
}

impl Answer {
    pub fn of(state: SecretState) -> Result<Self, Locked> {
        match state {
            SecretState::Locked => Err(Locked),
            SecretState::Missing => Ok(Answer::Missing),
            SecretState::Failed => Ok(Answer::Failed),
            SecretState::Set => Ok(Answer::Set),
        }
    }
}

impl From<Answer> for SecretState {
    fn from(answer: Answer) -> Self {
        match answer {
            Answer::Missing => SecretState::Missing,
            Answer::Failed => SecretState::Failed,
            Answer::Set => SecretState::Set,
        }
    }
}

/// Every declared name's answer at one reading, or [`Locked`]; read once per request, so a locked
/// store costs one timeout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot(Result<std::collections::BTreeMap<Settable, Answer>, Locked>);

impl Snapshot {
    #[must_use]
    pub fn read(secrets: &dyn Secrets, names: impl IntoIterator<Item = Settable>) -> Self {
        secrets.snapshot(&names.into_iter().collect::<Vec<_>>())
    }

    /// Asks for each name once and stops at the first locked answer, so a locked store costs one
    /// timeout.
    #[must_use]
    pub fn each(secrets: &(impl Secrets + ?Sized), names: &[Settable]) -> Self {
        Self::stood(names, |name| secrets.secret_state(name))
    }

    fn stood(names: &[Settable], stood: impl Fn(&Settable) -> SecretState) -> Self {
        Self(
            names
                .iter()
                .map(|name| Ok((name.clone(), Answer::of(stood(name))?)))
                .collect(),
        )
    }

    #[must_use]
    pub fn of(answers: Result<std::collections::BTreeMap<Settable, Answer>, Locked>) -> Self {
        Self(answers)
    }

    pub fn answers(&self) -> Result<&std::collections::BTreeMap<Settable, Answer>, Locked> {
        self.0.as_ref().map_err(|locked| *locked)
    }

    /// A name the reading did not ask reads as missing.
    #[must_use]
    pub fn state(&self, name: &Settable) -> SecretState {
        match &self.0 {
            Ok(answers) => answers
                .get(name)
                .map_or(SecretState::Missing, |answer| (*answer).into()),
            Err(Locked) => SecretState::Locked,
        }
    }
}

/// The state of this deployment's secrets, never their values.
pub trait Secrets: std::marker::Sync {
    fn secret_state(&self, settable: &Settable) -> SecretState;

    /// A store may answer in fewer questions than [`Snapshot::each`], with the same reading.
    fn snapshot(&self, names: &[Settable]) -> Snapshot {
        Snapshot::each(self, names)
    }

    /// This box's Microsoft OAuth slots for this account, read only through [`MailCredential::of`].
    fn mail_grants(&self, account: &MailAccount) -> MailGrants;

    /// Only the grant's parts cross, never the grant itself.
    fn google_grant(&self, account: &suzatary_mail::mail::address::EmailAddress) -> Held;
}

/// Each answer a secret store gave, by its question, as it stood when last asked.
#[derive(Debug, Clone, Default)]
pub struct SecretAnswers {
    states: std::collections::BTreeMap<String, SecretState>,
    grants: std::collections::BTreeMap<String, MailGrants>,
    google: std::collections::BTreeMap<String, Held>,
}

impl SecretAnswers {
    pub fn forget(&mut self) {
        *self = Self::default();
    }
}

/// `store` cached through `answers`; the store is asked with the lock released, so a slow question
/// blocks no other reader.
pub struct Remembering<'a> {
    pub store: &'a dyn Secrets,
    pub answers: &'a std::sync::Mutex<SecretAnswers>,
}

impl Remembering<'_> {
    fn answer<K: Ord + Clone, V: Clone>(
        &self,
        table: impl Fn(&mut SecretAnswers) -> &mut std::collections::BTreeMap<K, V>,
        key: &K,
        ask: impl FnOnce() -> V,
    ) -> V {
        let held = || {
            self.answers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        if let Some(answer) = table(&mut held()).get(key) {
            return answer.clone();
        }
        let answer = ask();
        table(&mut held()).insert(key.clone(), answer.clone());
        answer
    }
}

impl Secrets for Remembering<'_> {
    fn secret_state(&self, settable: &Settable) -> SecretState {
        self.answer(
            |answers| &mut answers.states,
            &settable.name().to_owned(),
            || self.store.secret_state(settable),
        )
    }

    /// A locked store is kept as locked for each name it was asked.
    fn snapshot(&self, names: &[Settable]) -> Snapshot {
        let held = || {
            self.answers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        };
        let before = held().states.clone();
        let asking: Vec<Settable> = names
            .iter()
            .filter(|name| !before.contains_key(name.name()))
            .cloned()
            .collect();
        let read = self.store.snapshot(&asking);
        let mut answers = held();
        for name in &asking {
            answers
                .states
                .insert(name.name().to_owned(), read.state(name));
        }
        Snapshot::stood(names, |name| {
            before
                .get(name.name())
                .copied()
                .unwrap_or_else(|| read.state(name))
        })
    }

    fn mail_grants(&self, account: &MailAccount) -> MailGrants {
        self.answer(
            |answers| &mut answers.grants,
            &account.email.to_string(),
            || self.store.mail_grants(account),
        )
    }

    fn google_grant(&self, account: &suzatary_mail::mail::address::EmailAddress) -> Held {
        self.answer(
            |answers| &mut answers.google,
            &account.to_string(),
            || self.store.google_grant(account),
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::secrets::SecretState::{self, Failed, Locked, Missing, Set};

    const ALL: [SecretState; 4] = [Missing, Failed, Locked, Set];

    #[test]
    fn and_is_a_meet_with_set_its_unit() {
        for a in ALL {
            assert_eq!(a.and(Set), a);
            assert_eq!(a.and(a), a);
            for b in ALL {
                assert_eq!(a.and(b), b.and(a));
                for c in ALL {
                    assert_eq!(a.and(b).and(c), a.and(b.and(c)));
                }
            }
        }
        assert_eq!(SecretState::all([]), Set);
    }

    #[test]
    fn one_missing_decides_and_a_locked_store_is_never_read_as_set() {
        assert_eq!(SecretState::all([Set, Locked, Missing]), Missing);
        assert_eq!(SecretState::all([Set, Locked]), Locked);
        assert!(!Locked.is_set());
    }
}
