//! The noticed sets the rule's laws build, here and in core's integration tests: test support,
//! never production.

use super::{Noticed, Owed};
use suzatary_base::capability::Datetime;
use suzatary_inbox::secretary::acts::Act;
use suzatary_inbox::secretary::proposal::Proposal;

/// The instant `minute` minutes into a fixed hour.
#[must_use]
pub fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 7, 9, minute, 0, 0).expect("an instant")
}

/// The card asking for the secret `name`.
#[must_use]
pub fn secret(name: &str) -> Proposal {
    Proposal::Act(Act::Secret {
        name: name.to_owned(),
        why: format!("{name} is declared"),
    })
}

/// The cards owed for the secrets `names`.
#[must_use]
pub fn owed(names: &[&str]) -> Owed {
    names
        .iter()
        .map(|name| Noticed {
            proposal: secret(name),
            why: format!("{name} is owed"),
            since: None,
            about: Vec::new(),
        })
        .collect()
}

/// A store that answers `Missing` for everything but the names in `locked`, `failed` and
/// `set`.
#[derive(Default)]
pub struct Store {
    pub locked: Vec<&'static str>,
    pub failed: Vec<&'static str>,
    pub set: Vec<String>,
    pub asked: std::sync::atomic::AtomicUsize,
    /// The parts every Google account's grant holds, or `None` for no grant.
    pub granted: Option<suzatary_google::google::Scopes>,
}

impl suzatary_connect::secrets::Secrets for Store {
    fn secret_state(
        &self,
        settable: &suzatary_connect::settings::Settable,
    ) -> suzatary_connect::secrets::SecretState {
        self.asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.locked.contains(&settable.name()) {
            suzatary_connect::secrets::SecretState::Locked
        } else if self.failed.contains(&settable.name()) {
            suzatary_connect::secrets::SecretState::Failed
        } else if self.set.iter().any(|name| name == settable.name()) {
            suzatary_connect::secrets::SecretState::Set
        } else {
            suzatary_connect::secrets::SecretState::Missing
        }
    }

    fn mail_grants(
        &self,
        _account: &suzatary_mail::mail::account::MailAccount,
    ) -> suzatary_connect::secrets::MailGrants {
        suzatary_connect::secrets::MailGrants::default()
    }

    fn google_grant(
        &self,
        _: &suzatary_mail::mail::address::EmailAddress,
    ) -> suzatary_google::google::Held {
        self.granted.clone().map_or(
            suzatary_google::google::Held::NotSignedIn,
            suzatary_google::google::Held::Signed,
        )
    }
}
