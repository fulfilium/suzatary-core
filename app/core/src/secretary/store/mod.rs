//! What a conversation is, and what is stored of one.
//!
//! A conversation is a prodrome of its own, never turns on the todo chain: each [`Entry`] is a
//! line of the conversations' schema ([`crate::secretary::line_event`]), its words through the
//! redaction door and locked, and read back in causal order by [`crate::secretary::transcript`].
//! One type per file; this module is their one name.
//!
//! The value is a fold: [`Stored::of`] replays the [`Entry`] lines, so nothing is kept in step by
//! hand.
//!
//! A proposal is data, never a call: the tap carries an index, never a body, so the text the
//! owner confirms is the text that goes. The proposal and what became of it live in the inbox
//! prodrome; a line names the object and [`Stored::standing`] joins the two.

mod aside;
mod asking;
mod conversation_id;
mod decision;
mod draft;
mod entry;
mod forward;
mod kept;
mod kind;
mod line;
mod lines;
mod namer;
mod need;
mod proposal;
mod reasons;
mod review;
mod review_failure;
mod rewind;
mod said;
mod scope;
mod standing;
mod state;
mod stored;
mod title;

pub use aside::Aside;
pub use asking::Asking;
pub use conversation_id::{ConversationId, DEFAULT, NotAnId};
pub use decision::{Decision, Outcome, Settle};
pub use draft::{Draft, EMAIL};
pub use entry::{Entry, stamp};
pub use forward::{EML, FORWARD, FORWARDED, Forward, Forwarded, SEPARATOR, Style};
pub use kept::Kept;
pub use kind::Kind;
pub use line::{Line, Unheld, Unplaced};
pub use lines::Lines;
pub use namer::Namer;
pub use need::Need;
pub use proposal::{Proposal, REMEMBER};
pub use reasons::{NoReasons, Reasons};
pub use review::Review;
pub use review_failure::ReviewFailure;
pub use rewind::Rewind;
pub use said::Said;
pub use scope::Scope;
pub use standing::{Revised, Standing, refused};
pub use state::{ConversationState, state};
pub use stored::{NotAConversation, Stored, heading, stopped_by};
pub use title::{NotATitle, Title};

#[cfg(test)]
mod tests;

/// The instant a test names: an ISO stamp, or `t<n>` for second `n` of one morning (`t` is `t0`).
#[cfg(test)]
pub(crate) fn instant(at: &str) -> crate::capability::Datetime {
    match at.strip_prefix('t').and_then(|n| {
        if n.is_empty() {
            Some(0)
        } else {
            n.parse::<u32>().ok()
        }
    }) {
        Some(n) => {
            crate::capability::Datetime::new(2026, 9, 9, 9, n / 60, n % 60, 0).expect("an instant")
        }
        None => prodrome_core::fpl::datetime_of(
            prodrome_core::fpl::parse_iso(at).expect("an ISO stamp"),
        )
        .expect("an instant"),
    }
}
