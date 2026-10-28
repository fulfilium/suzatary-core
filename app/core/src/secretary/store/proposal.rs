//! One act the secretary wants performed and will not perform itself.

use super::Draft;
use serde::{Deserialize, Serialize};

/// One act the secretary wants performed and will not perform itself, whole.
///
/// An email is irreversible, a changeset changes the owner's commitments, and an
/// [`Act`](crate::secretary::acts::Act) is credential-shaped; one tap answers all, because a
/// `Tap` witnesses only that the owner asked. A sum, so each card and performer is exhaustive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "act", rename_all = "lowercase")]
pub enum Proposal {
    /// Mail to send — the whole message, so the text shown is the text sent.
    Email(Draft),
    /// A stored message to forward, around a note: the card names the original, and the tap
    /// reads it again unmasked.
    Forward(super::Forward),
    /// Chain acts to append as the trusted actor, against one chain.
    ///
    /// The acts are stored and the events are not, since an `Authored` built at proposal time would
    /// undo whatever moved in between ([`crate::secretary::changes`]).
    Changes(crate::secretary::changes::Changeset),
    /// An act for the box to perform, on the same tap.
    Act(crate::secretary::acts::Act),
    /// A fact to remember, asked for in a round that had read something not the owner's own: a
    /// memory written because an email said so is the attack, so his tap writes it instead.
    ///
    /// It names the fact's words and never holds them: they are kept in the memory's text store
    /// ([`crate::secretary::memories`]), so dismissing the card, or forgetting the fact it became,
    /// deletes them while this record stays.
    Remember {
        /// The words, by the name the memory's text store keeps them under.
        text: crate::secretary::salted_text::TextName,
        /// What it is about.
        kind: crate::secretary::memory::Kind,
    },
    /// A decision to record, whole, as the owner's own once he taps it.
    Decide(crate::decisions::Draft),
}

impl Proposal {
    /// Whether the box's own card for it is asked again in a new episode, so is its instant as
    /// well as its content: an account's sign-in, asked again once the account is connected
    /// again, a plugin's grant, asked again whenever a link is pasted from the origin, and a
    /// secret, asked again once a value stored from its card is lost.
    #[must_use]
    pub fn episodic(&self) -> bool {
        matches!(
            self,
            Proposal::Act(
                crate::secretary::acts::Act::Auth { .. }
                    | crate::secretary::acts::Act::AuthSend { .. }
                    | crate::secretary::acts::Act::GoogleSignIn { .. }
                    | crate::secretary::acts::Act::Grant { .. }
                    | crate::secretary::acts::Act::Secret { .. }
            )
        )
    }

    /// Whether asking this again later is a new request: the inbox's identity rule
    /// ([`crate::secretary::inbox::ProposalId::of`]), for every proposer but the box
    /// ([`crate::secretary::inbox::Asked::recurs`]).
    ///
    /// An effect whose repetition does harm (an email, a changeset, a fact, a push, a coding task)
    /// is its content alone, so it is never performed twice; an act meant to recur (a sign-in, a
    /// secret, a page or document read) is also its instant.
    #[must_use]
    pub fn recurs(&self) -> bool {
        use crate::secretary::acts::Act;
        match self {
            Proposal::Email(_)
            | Proposal::Forward(_)
            | Proposal::Changes(_)
            | Proposal::Remember { .. }
            | Proposal::Decide(_) => false,
            Proposal::Act(act) => match act {
                Act::Auth { .. }
                | Act::AuthSend { .. }
                | Act::Secret { .. }
                | Act::Web { .. }
                | Act::DriveRead { .. }
                | Act::SignIn { .. }
                | Act::UnlockStore { .. }
                // A connection made, removed and made again is three requests.
                | Act::Connect { .. }
                | Act::Disconnect { .. }
                | Act::Grant { .. }
                // A client kept and lost is asked again, and a sign-in with Google lapses.
                | Act::GoogleClient { .. }
                | Act::GoogleSignIn { .. } => true,
                // One key is one registration, however often it asks; one link is one sign-in.
                Act::Patch { .. }
                | Act::CodingSession { .. }
                | Act::Device(_)
                | Act::SignInCode { .. }
                // An event written twice is two events.
                | Act::CalendarEvent { .. }
                | Act::Erase { .. } => false,
            },
        }
    }

    /// A one-line summary for a list, never for a confirmation: the card shows every field. A
    /// remember card's words are found by `words`, and a card whose words are gone says so.
    #[must_use]
    pub fn headline(
        &self,
        words: impl Fn(
            &crate::secretary::salted_text::TextName,
        ) -> crate::secretary::inbox_reading::Words,
    ) -> String {
        use crate::secretary::inbox_reading::Words;
        match self {
            Proposal::Email(draft) => {
                format!("email {} — {}", draft.to.join(", "), draft.subject)
            }
            Proposal::Forward(forward) => format!(
                "forward {} to {}",
                forward.message.subject,
                forward.to.join(", ")
            ),
            Proposal::Changes(changeset) => format!(
                "{} change{} on {}: {}",
                changeset.acts().len(),
                if changeset.acts().len() == 1 { "" } else { "s" },
                changeset.chain(),
                changeset
                    .acts()
                    .iter()
                    .map(|act| format!("{} {}", act.word(), act.todo()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            // The act's own sentence, so a list and a card cannot print two words for one act.
            Proposal::Act(act) => act.sentence(),
            Proposal::Remember { text, kind } => match words(text) {
                Words::Kept(fact) => format!("remember ({}): {fact}", kind.as_str()),
                Words::Gone => format!("remember ({}): its words are deleted", kind.as_str()),
                Words::Unread => format!("remember ({})", kind.as_str()),
            },
            Proposal::Decide(draft) => crate::secretary::decide::sentence(draft),
        }
    }
}

/// `Remember(text, kind)`.
pub const REMEMBER: prodrome_core::literal::codec::Constructor = ("Remember", &["text", "kind"]);

/// An act is its arm's own constructor: `Email`, `Forward`, `Changes`, `Remember`, `Decide`, or
/// one of the acts the box performs.
impl prodrome_core::literal::codec::Literal for Proposal {
    fn to_value(&self) -> prodrome_core::literal::Value {
        use prodrome_core::literal::codec::record;
        match self {
            Proposal::Email(draft) => draft.to_value(),
            Proposal::Forward(forward) => forward.to_value(),
            Proposal::Changes(changeset) => changeset.to_value(),
            Proposal::Act(act) => act.to_value(),
            Proposal::Remember { text, kind } => {
                record(REMEMBER, vec![text.to_value(), kind.to_value()])
            }
            Proposal::Decide(draft) => draft.to_value(),
        }
    }

    fn from_value(
        value: &prodrome_core::literal::Value,
    ) -> Result<Self, prodrome_core::literal::ProdromeError> {
        use prodrome_core::literal::codec::{Fields, arm};
        Ok(match arm(value)? {
            "Email" => Proposal::Email(Draft::from_value(value)?),
            "Forward" => Proposal::Forward(super::Forward::from_value(value)?),
            "Changes" => {
                Proposal::Changes(crate::secretary::changes::Changeset::from_value(value)?)
            }
            "Remember" => {
                let fields = Fields::of(value, REMEMBER)?;
                Proposal::Remember {
                    text: fields.get("text")?,
                    kind: fields.get("kind")?,
                }
            }
            "Decide" => Proposal::Decide(crate::decisions::Draft::from_value(value)?),
            _ => Proposal::Act(crate::secretary::acts::Act::from_value(value)?),
        })
    }
}
