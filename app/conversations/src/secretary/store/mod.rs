//! What a conversation is, and what is stored of one: a prodrome of its own, never turns on the
//! todo chain, its value the fold [`Stored::of`] of its [`Entry`] lines.
//!
//! A proposal is data, never a call: the tap carries an index, never a body, so the text the
//! owner confirms is the text that goes.

mod aside;
mod asking;
mod began;
mod decision;
mod entry;
mod kind;
mod line;
mod lines;
mod namer;
mod retired;
mod review;
mod review_failure;
mod rewind;
mod said;
mod scope;
mod standing;
mod start;
mod state;
mod stored;
mod title;

pub use aside::Aside;
pub use asking::Asking;
pub use began::{COVERED, Covered, RunId};
pub use decision::{Decision, Outcome, Settle};
pub use entry::Entry;
pub use kind::Kind;
pub use line::{Line, Unheld, Unplaced};
pub use lines::Lines;
pub use namer::Namer;
pub use retired::Retired;
pub use review::Review;
pub use review_failure::ReviewFailure;
pub use rewind::Rewind;
pub use said::Said;
pub use scope::Scope;
pub use standing::{Revised, Standing, refused};
pub use start::{Begun, Start};
pub use state::ConversationState;
pub use stored::{Carried, Folded, NotAConversation, Stored, heading, stopped_by};
pub use title::{NotATitle, Title};

#[cfg(test)]
mod tests;

/// The instant a test names: an ISO stamp, or `t<n>` for second `n` of one morning (`t` is `t0`).
#[cfg(any(test, feature = "nothing"))]
#[must_use]
pub fn instant(at: &str) -> suzatary_base::capability::Datetime {
    match at.strip_prefix('t').and_then(|n| {
        if n.is_empty() {
            Some(0)
        } else {
            n.parse::<u32>().ok()
        }
    }) {
        Some(n) => suzatary_base::capability::Datetime::new(2026, 9, 9, 9, n / 60, n % 60, 0)
            .expect("an instant"),
        None => prodrome_data::instant::datetime_of(
            prodrome_data::instant::parse_iso(at).expect("an ISO stamp"),
        )
        .expect("an instant"),
    }
}
