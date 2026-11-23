//! The inert pushes and sign-in consents: test support, never production.

use crate::notify::{Delivered, Notify, NotifyError, Push};
use suzatary_base::capability::Nothing;
use suzatary_inbox::secretary::responded;

impl Notify for Nothing {
    fn push(&self, _message: &Push) -> Result<Delivered, NotifyError> {
        Err(NotifyError::Unreachable("nothing is pushed".into()))
    }
}

impl crate::secretary::google_sign_in::Consents for Nothing {
    fn redirect(&self) -> Option<&suzatary_google::google::RedirectUri> {
        None
    }

    fn begin(
        &self,
        _: &suzatary_inbox::secretary::inbox::ProposalId,
        _: &suzatary_mail::mail::address::EmailAddress,
        _: &suzatary_google::google::Scopes,
        _: &prodrome_data::event::Actor,
        _: suzatary_base::capability::Datetime,
        _: crate::secretary::google_sign_in::Then,
    ) -> responded::Performed {
        responded::Performed::Tried(responded::Tried::NotSent {
            reason: "nothing opens a sign-in with Google here".to_owned(),
        })
    }
}
