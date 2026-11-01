//! Where a secret stands, never what it is: the only answer this crate can see is a
//! [`SecretState`].

use crate::mail::account::{MailAccount, Sync};
use crate::settings::Settable;

/// One account's OAuth consents held by this box: for Microsoft, IMAP and SMTP are separate grants
/// in separate slots; for a Google account, its one sign-in with Google is its reading grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MailGrants {
    /// Whether this account holds the grant its mail is read with: the IMAP slot, or the sign-in
    /// with Google.
    pub imap: bool,
    /// Whether this account holds an SMTP grant.
    pub smtp: bool,
}

/// Where one mail account's credential lives, a sum over [`Sync`] so an account whose grant this
/// box does not hold is never shown as broken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "MailCredentialRow")]
pub enum MailCredential {
    /// Microsoft over IMAP and XOAUTH2, the only strategy whose grant lives in this box's store.
    Slots {
        /// Whether this account holds an IMAP grant.
        imap: bool,
        /// Whether this account holds an SMTP grant, separately.
        smtp: bool,
    },
    /// Gmail through lieer, which owns its own credential outside this box's token store.
    Lieer,
    /// A Google account whose sign-in with Google the box keeps in its secret store.
    Google {
        /// Whether the box holds it.
        granted: bool,
    },
    /// Proton through the local Bridge, which owns its own credential the same way.
    Bridge,
}

impl MailCredential {
    /// Pick the arm `account.sync` names, filling [`MailCredential::Slots`] only for Microsoft.
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

/// Where a secret stands: what a lookup found, with the value dropped.
///
/// "The store is locked" is not "nothing is set": a page that read a locked
/// store as unset would ask the owner to set again what is already there. Nor
/// is a store that cannot be run locked: unlocking it fixes nothing. Ordered
/// `Missing < Failed < Locked < Set`: false, the two unknowns with the one an
/// unlock does not mend lower, and true, so [`SecretState::and`] is its meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum SecretState {
    /// Nothing holds the name.
    Missing,
    /// The store could not be run, or answered with what is not a value.
    Failed,
    /// The store could not say, as a locked one cannot.
    Locked,
    /// It resolves.
    Set,
}

impl SecretState {
    /// Where several secrets stand together, "every one is set": the meet,
    /// whose unit is [`SecretState::Set`]; one missing decides it whatever
    /// another's store says.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        self.min(other)
    }

    /// [`SecretState::and`] over `states`, `Set` for none.
    #[must_use]
    pub fn all(states: impl IntoIterator<Item = Self>) -> Self {
        states.into_iter().fold(Self::Set, Self::and)
    }

    /// Whether it resolves.
    #[must_use]
    pub fn is_set(self) -> bool {
        self == Self::Set
    }
}

/// The store could not say, as a locked one cannot: an answer about the store, never about
/// a name, and the one that costs a timeout to learn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locked;

/// What the store answered for one name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Nothing holds it.
    Missing,
    /// The store could not be run for it, or held what is not a value.
    Failed,
    /// It resolves.
    Set,
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

/// Where every declared name stood at one reading: each name's answer, or [`Locked`]. Read
/// once per request, so a locked store costs one timeout however many names are declared and
/// however often a request asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot(Result<std::collections::BTreeMap<Settable, Answer>, Locked>);

impl Snapshot {
    /// Ask `secrets` for each of `names` once, stopping at the first answer that the store is
    /// locked: asking the rest would only wait out the same timeout again for each. Any other
    /// answer is the name's own, so one value that cannot be read leaves the others standing.
    #[must_use]
    pub fn read(secrets: &dyn Secrets, names: impl IntoIterator<Item = Settable>) -> Self {
        let mut answers = std::collections::BTreeMap::new();
        for name in names {
            let answer = match secrets.secret_state(&name) {
                SecretState::Locked => return Self(Err(Locked)),
                SecretState::Missing => Answer::Missing,
                SecretState::Failed => Answer::Failed,
                SecretState::Set => Answer::Set,
            };
            answers.insert(name, answer);
        }
        Self(Ok(answers))
    }

    /// Each name's answer, or [`Locked`].
    ///
    /// # Errors
    /// [`Locked`], when the store could not say.
    pub fn answers(&self) -> Result<&std::collections::BTreeMap<Settable, Answer>, Locked> {
        self.0.as_ref().map_err(|locked| *locked)
    }

    /// Where `name` stood: its answer, or locked; a name the reading did not ask is missing.
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

/// The state of this deployment's secrets, and nothing about their content.
pub trait Secrets: std::marker::Sync {
    /// Where this declared name stands now.
    fn secret_state(&self, settable: &Settable) -> SecretState;

    /// This box's own Microsoft OAuth slots for this account, read only through
    /// [`MailCredential::of`].
    fn mail_grants(&self, account: &MailAccount) -> MailGrants;

    /// Whether `account` is signed in with Google, and the parts its grant holds: read from the
    /// kept grant, of which only the parts cross.
    fn google_grant(&self, account: &crate::mail::address::EmailAddress) -> crate::google::Held;
}

/// Each answer a secret store gave, by its question: the store as it stood when last asked.
/// Empty until something asks; [`SecretAnswers::forget`] empties it again.
#[derive(Debug, Clone, Default)]
pub struct SecretAnswers {
    states: std::collections::BTreeMap<String, SecretState>,
    grants: std::collections::BTreeMap<String, MailGrants>,
    google: std::collections::BTreeMap<String, crate::google::Held>,
}

impl SecretAnswers {
    /// Every answer dropped, so each question is asked of the store again.
    pub fn forget(&mut self) {
        *self = Self::default();
    }
}

/// `store` asked through `answers`: a question already answered there is answered from there,
/// and any other is asked of `store` once and its answer kept. The store is asked with nothing
/// held, so a slow question holds up no other reader.
pub struct Remembering<'a> {
    /// The store asked.
    pub store: &'a dyn Secrets,
    /// What it answered so far.
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

    fn mail_grants(&self, account: &MailAccount) -> MailGrants {
        self.answer(
            |answers| &mut answers.grants,
            &account.email.to_string(),
            || self.store.mail_grants(account),
        )
    }

    fn google_grant(&self, account: &crate::mail::address::EmailAddress) -> crate::google::Held {
        self.answer(
            |answers| &mut answers.google,
            &account.to_string(),
            || self.store.google_grant(account),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SecretState::{self, Failed, Locked, Missing, Set};

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
