//! Whether performing a proposal sends mail, by a match the compiler holds complete.

use crate::google::Scope;
use crate::secretary::acts::Act;
use crate::secretary::store::Proposal;

/// Whether a tap on a proposal puts mail on a wire, or gives the box the consent to.
/// The greater of two sends when either does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sending {
    /// Its tap sends no mail and grants no way to send any.
    SendsNothing,
    /// Its tap sends mail through SMTP or a provider's send API, or grants the box a way to.
    Sends,
}

impl Sending {
    /// What a tap on `proposal` does to mail.
    #[must_use]
    pub fn of(proposal: &Proposal) -> Self {
        match proposal {
            Proposal::Email(_) | Proposal::Forward(_) => Sending::Sends,
            Proposal::Changes(_) | Proposal::Remember { .. } | Proposal::Decide(_) => {
                Sending::SendsNothing
            }
            Proposal::Act(act) => match act {
                // The SMTP grant is sending's own consent, separate from reading's.
                Act::AuthSend { .. } => Sending::Sends,
                Act::GoogleSignIn { scopes, .. } => scopes
                    .iter()
                    .map(Sending::granting)
                    .max()
                    .unwrap_or(Sending::SendsNothing),
                Act::Auth { .. }
                | Act::Secret { .. }
                | Act::Web { .. }
                | Act::Patch { .. }
                | Act::DriveRead { .. }
                | Act::CodingSession { .. }
                | Act::Device(_)
                | Act::SignIn { .. }
                | Act::SignInCode { .. }
                | Act::UnlockStore { .. }
                | Act::Connect { .. }
                | Act::Disconnect { .. }
                | Act::Grant { .. }
                | Act::GoogleClient { .. }
                // A new event names no attendee, so the calendar invites nobody by mail.
                | Act::CalendarEvent { .. }
                | Act::Erase { .. }
                | Act::Retract { .. }
                | Act::Afresh { .. }
                | Act::Noise { .. } => Sending::SendsNothing,
            },
        }
    }

    /// What a grant of `scope` gives: Gmail's `gmail.modify` sends as well as reads.
    #[must_use]
    pub const fn granting(scope: Scope) -> Self {
        match scope {
            Scope::Mail => Sending::Sends,
            Scope::Calendar => Sending::SendsNothing,
        }
    }
}
