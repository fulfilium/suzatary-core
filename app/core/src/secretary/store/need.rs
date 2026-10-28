//! What a proposal asks the owner for, which two cards asking for the same thing share: one need
//! has one card, whichever rule asks for it and in whatever words.

use super::Proposal;
use crate::mail::address::EmailAddress;
use crate::secretary::acts::Act;

/// What a proposal asks the owner for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need<'a> {
    /// A secret, by its name: the secrets rule and a failing link both ask for it, each in its
    /// own words.
    Secret(&'a str),
    /// An account's sign-in with Google, by the account: a sign-in asking for one missing part
    /// and one asking for every part are one sign-in.
    GoogleSignIn(&'a EmailAddress),
    /// Any other proposal, by itself.
    Proposal(&'a Proposal),
}

impl Proposal {
    /// What it asks the owner for.
    #[must_use]
    pub fn need(&self) -> Need<'_> {
        match self {
            Proposal::Act(Act::Secret { name, .. }) => Need::Secret(name),
            Proposal::Act(Act::GoogleSignIn { account, .. }) => Need::GoogleSignIn(account),
            Proposal::Act(
                Act::Web { .. }
                | Act::Patch { .. }
                | Act::DriveRead { .. }
                | Act::CodingSession { .. }
                | Act::Device(_)
                | Act::Auth { .. }
                | Act::AuthSend { .. }
                | Act::SignIn { .. }
                | Act::SignInCode { .. }
                | Act::UnlockStore { .. }
                | Act::Connect { .. }
                | Act::Disconnect { .. }
                | Act::Grant { .. }
                | Act::Erase { .. }
                | Act::GoogleClient { .. }
                | Act::CalendarEvent { .. },
            )
            | Proposal::Email(_)
            | Proposal::Forward(_)
            | Proposal::Changes(_)
            | Proposal::Remember { .. }
            | Proposal::Decide(_) => Need::Proposal(self),
        }
    }
}
