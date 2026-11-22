//! One act the secretary wants performed and will not perform itself, with its reasons.

use crate::secretary::changes::Changeset;
use prodrome_core::literal::Value;
use prodrome_core::literal::codec::Constructor;
mod draft;
mod forward;
mod reasons;

pub use draft::{Draft, EMAIL};
pub use forward::{EML, Forward, Forwarded, Style};
pub(crate) use forward::{FORWARD, FORWARDED};
pub use reasons::{NoReasons, Reasons};

use serde::{Deserialize, Serialize};
use suzatary_memory::secretary::memory;
use suzatary_memory::secretary::memory::Replaces;
use suzatary_memory::secretary::salted_text;

/// One tap answers an email, a changeset and a credential-shaped
/// [`Act`](crate::secretary::acts::Act) alike, because a `Tap` witnesses only that the owner asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "act", rename_all = "lowercase")]
pub enum Proposal {
    /// Mail to send — the whole message, so the text shown is the text sent.
    Email(Draft),
    /// A stored message to forward, around a note: the card names the original, and the tap
    /// reads it again unmasked.
    Forward(Forward),
    /// Chain acts to append as the trusted actor, against one chain; the acts are stored and the
    /// events are not, since an `Authored` built at proposal time would undo whatever moved in
    /// between.
    Changes(Changeset),
    /// An act for the box to perform, on the same tap.
    Act(crate::secretary::acts::Act),
    /// A fact to remember from a round that had read something not the owner's own, since a
    /// memory written because an email said so is the attack; it names the words and never holds
    /// them, so dismissing the card or forgetting the fact deletes them while this record stays.
    Remember {
        /// The words, by the name the memory's text store keeps them under.
        text: salted_text::TextName,
        kind: memory::Kind,
        replaces: memory::Replaces,
    },
    /// A decision to record, whole, as the owner's own once he taps it.
    Decide(suzatary_todo::decisions::Draft),
}

impl Proposal {
    /// Whether the box's own card for it is asked again in a new episode, so is its instant as
    /// well as its content.
    #[must_use]
    pub(crate) fn episodic(&self) -> bool {
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

    /// Whether asking this again later is a new request, for every proposer but the box
    /// ([`crate::secretary::inbox::Asked::recurs`]): an effect whose repetition does harm is its
    /// content alone, so it is never performed twice, and an act meant to recur is also its
    /// instant.
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
                | Act::Noise { .. }
| Act::Restore { .. }
                | Act::SignInCode { .. }
                // An event written twice is two events.
                | Act::CalendarEvent { .. }
                | Act::Erase { .. }
                | Act::Retract { .. }
                | Act::Afresh { .. } => false,
            },
        }
    }

    /// A one-line summary for a list, never for a confirmation: the card shows every field.
    #[must_use]
    pub fn headline(
        &self,
        words: impl Fn(&salted_text::TextName) -> crate::secretary::inbox_reading::Words,
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
            Proposal::Remember {
                text,
                kind,
                replaces,
            } => {
                let verb = match replaces {
                    Replaces::Nothing => "remember".to_owned(),
                    Replaces::Facts(sources) => format!("merge {sources} into one"),
                };
                match words(text) {
                    Words::Kept(fact) => format!("{verb} ({}): {fact}", kind.as_str()),
                    Words::Gone => format!("{verb} ({}): its words are deleted", kind.as_str()),
                    Words::Unread => format!("{verb} ({})", kind.as_str()),
                }
            }
            Proposal::Decide(draft) => crate::secretary::decide::sentence(draft),
        }
    }
}

pub const REMEMBER: Constructor = ("Remember", &["text", "kind"]);
/// `Consolidate(text, kind, from)`: a remember card that replaces the facts `from`.
pub const CONSOLIDATE: Constructor = ("Consolidate", &["text", "kind", "from"]);

// By hand: most variants print as the record they hold, and one prints two constructors.
impl prodrome_core::literal::codec::Literal for Proposal {
    fn to_value(&self) -> Value {
        use prodrome_core::literal::codec::record;
        match self {
            Proposal::Email(draft) => draft.to_value(),
            Proposal::Forward(forward) => forward.to_value(),
            Proposal::Changes(changeset) => changeset.to_value(),
            Proposal::Act(act) => act.to_value(),
            Proposal::Remember {
                text,
                kind,
                replaces: Replaces::Nothing,
            } => record(REMEMBER, vec![text.to_value(), kind.to_value()]),
            Proposal::Remember {
                text,
                kind,
                replaces: Replaces::Facts(from),
            } => record(
                CONSOLIDATE,
                vec![text.to_value(), kind.to_value(), from.to_value()],
            ),
            Proposal::Decide(draft) => draft.to_value(),
        }
    }

    fn from_value(value: &Value) -> Result<Self, prodrome_core::literal::ProdromeError> {
        use prodrome_core::literal::codec::{Fields, arm};
        Ok(match arm(value)? {
            "Email" => Proposal::Email(Draft::from_value(value)?),
            "Forward" => Proposal::Forward(Forward::from_value(value)?),
            "Changes" => Proposal::Changes(Changeset::from_value(value)?),
            "Remember" => {
                let fields = Fields::of(value, REMEMBER)?;
                Proposal::Remember {
                    text: fields.get("text")?,
                    kind: fields.get("kind")?,
                    replaces: Replaces::Nothing,
                }
            }
            "Consolidate" => {
                let fields = Fields::of(value, CONSOLIDATE)?;
                Proposal::Remember {
                    text: fields.get("text")?,
                    kind: fields.get("kind")?,
                    replaces: Replaces::Facts(fields.get("from")?),
                }
            }
            "Decide" => Proposal::Decide(suzatary_todo::decisions::Draft::from_value(value)?),
            _ => Proposal::Act(crate::secretary::acts::Act::from_value(value)?),
        })
    }
}
