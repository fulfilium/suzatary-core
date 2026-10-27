//! Recorded decisions: designs and plans as items of a store of their own, each a statement the
//! project holds, and the links that say which later statement is the newer word on what an
//! earlier one decided. Specs and laws stay files; a decision names the file it summarises.
//!
//! Not a `decision` todo, which is a choice awaiting the owner's tap: a recorded decision is one
//! already made. Its schema ([`DecisionEvent`]) is keyed by [`DecisionId`] and writes three
//! registers: the [`Statement`] (discrete, so two concurrent revisions are a conflict shown
//! whole), the [`Life`] (inflationary, withdrawn the top, so a withdrawn decision never stands
//! again), and the links it supersedes, a grow-only set read by union. What a decision MEANS at
//! a moment is the [`Ledger`]: every decision with its statement and life, ordered by
//! supersession, a superseding decision above what it supersedes. Every reading is a function of
//! the object set.

mod draft;
mod event;
mod id;
mod ledger;
mod life;
mod refused;
mod registers;
mod statement;
mod store;
mod view;

pub use draft::{DECIDE, Draft};
pub use event::{DecisionEvent, DecisionVocabulary, Said, decided, revised, withdrawn};
pub use id::DecisionId;
pub use ledger::{Decision, Held, Ledger, Withdrawal};
pub use life::Life;
pub use refused::NotADecision;
pub use registers::{ContentRegister, DecisionRegister, DecisionRegisters, LifeRegister};
pub use statement::Statement;
pub use store::Decisions;
pub use view::document;
