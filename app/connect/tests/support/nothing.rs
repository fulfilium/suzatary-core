//! The inert configuration and secrets: test support, never production.

use suzatary_base::capability::Nothing;
use suzatary_google::google::Held;
use suzatary_mail::mail::account::MailAccount;
use suzatary_mail::mail::address::EmailAddress;

use crate::connect::Declared;
use crate::secrets::{MailGrants, SecretState, Secrets};
use crate::settings::{Configuration, EditError, Settable};

impl Configuration for Nothing {
    fn declared(&self) -> Result<Declared, EditError> {
        Ok(Declared::default())
    }
}

impl Secrets for Nothing {
    fn secret_state(&self, _settable: &Settable) -> SecretState {
        SecretState::Missing
    }
    fn mail_grants(&self, _account: &MailAccount) -> MailGrants {
        MailGrants::default()
    }

    fn google_grant(&self, _: &EmailAddress) -> Held {
        Held::NotSignedIn
    }
}
