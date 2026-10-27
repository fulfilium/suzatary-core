use std::collections::BTreeSet;

use prodrome_core::event::{Actor, Hash};
use prodrome_core::literal::Datetime;

use super::{DecisionEvent, DecisionId, Draft, Ledger, Statement};
use crate::capability::Typeset;
use crate::refusal::{Refusal, shown};
use crate::store::Prodrome;

/// The decisions' store, and its three writes.
///
/// Every reading is the [`Ledger`] of the objects and takes no lock. Each write asks its
/// non-monotone questions (is this id new, does it still stand, would this link close a cycle)
/// under the store's hold against the reading, then appends; the store's own append refuses a
/// restatement of a withdrawn decision whatever was asked.
#[derive(Clone, Copy)]
pub struct Decisions<'a> {
    /// The objects.
    pub store: &'a dyn Prodrome<DecisionEvent>,
}

impl Decisions<'_> {
    /// The reading.
    ///
    /// # Errors
    /// The store's own.
    pub fn ledger(&self) -> Result<Ledger, Refusal> {
        Ok(Ledger::of(&self.store.nodes()?))
    }

    /// Record a new decision, its body through the markup door.
    ///
    /// # Errors
    /// [`Refusal::conflict`] for an id already decided; [`Refusal::bad_request`] for a link to a
    /// decision not held, a link that would close a cycle, or a statement or body refused.
    pub fn decide(
        &self,
        draft: &Draft,
        at: Datetime,
        actor: &Actor,
        typeset: &dyn Typeset,
    ) -> Result<Hash, Refusal> {
        let _hold = self.store.locked()?;
        let event = super::decided(
            draft.id().clone(),
            at,
            actor.clone(),
            fresh(&self.ledger()?, draft, typeset)?,
            draft.supersedes().clone(),
        )
        .map_err(|refused| Refusal::bad_request(refused.to_string()))?;
        Ok(self.store.append(event)?)
    }

    /// Whether [`Decisions::decide`] would record `draft` now: what a proposal is checked by
    /// before it is stored, so a card the owner taps is one that would land. A non-monotone
    /// answer, asked again under the hold when the tap decides.
    ///
    /// # Errors
    /// As [`Decisions::decide`].
    pub fn admits(&self, draft: &Draft, typeset: &dyn Typeset) -> Result<(), Refusal> {
        fresh(&self.ledger()?, draft, typeset).map(drop)
    }

    /// Restate a standing decision, its links joining those it already made.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for an id never decided; [`Refusal::conflict`] for a withdrawn
    /// one; otherwise as [`Decisions::decide`].
    pub fn revise(
        &self,
        draft: &Draft,
        at: Datetime,
        actor: &Actor,
        typeset: &dyn Typeset,
    ) -> Result<Hash, Refusal> {
        let _hold = self.store.locked()?;
        let ledger = self.ledger()?;
        standing(&ledger, draft.id())?;
        linkable(&ledger, draft.id(), draft.supersedes())?;
        let event = super::revised(
            draft.id().clone(),
            at,
            actor.clone(),
            statement(draft, typeset)?,
            draft.supersedes().clone(),
        )
        .map_err(|refused| Refusal::bad_request(refused.to_string()))?;
        Ok(self.store.append(event)?)
    }

    /// Withdraw a standing decision.
    ///
    /// # Errors
    /// [`Refusal::not_found`] for an id never decided; [`Refusal::conflict`] for one already
    /// withdrawn; [`Refusal::bad_request`] for an empty reason.
    pub fn withdraw(
        &self,
        id: &DecisionId,
        reason: &str,
        at: Datetime,
        actor: &Actor,
    ) -> Result<Hash, Refusal> {
        let _hold = self.store.locked()?;
        standing(&self.ledger()?, id)?;
        let event = super::withdrawn(id.clone(), at, actor.clone(), reason.trim())
            .map_err(|refused| Refusal::bad_request(refused.to_string()))?;
        Ok(self.store.append(event)?)
    }
}

/// A new decision's statement: its id not yet decided, its links linkable.
fn fresh(ledger: &Ledger, draft: &Draft, typeset: &dyn Typeset) -> Result<Statement, Refusal> {
    if ledger.get(draft.id()).is_some() {
        return Err(Refusal::conflict(format!(
            "{} is already decided: revise it, or decide anew under another id",
            draft.id()
        )));
    }
    linkable(ledger, draft.id(), draft.supersedes())?;
    statement(draft, typeset)
}

/// `draft`'s statement, its body admitted by the one door.
fn statement(draft: &Draft, typeset: &dyn Typeset) -> Result<Statement, Refusal> {
    let body = crate::authoring::admitted("body", draft.body(), typeset)?;
    Statement::new(draft.title(), body, draft.rationale())
        .map_err(|refused| Refusal::bad_request(refused.to_string()))
}

/// `id` is decided and not withdrawn.
fn standing(ledger: &Ledger, id: &DecisionId) -> Result<(), Refusal> {
    match ledger.get(id) {
        None => Err(Refusal::not_found(format!(
            "no decision is named {}",
            shown(id.as_str())
        ))),
        Some(decision) if decision.withdrawn.is_some() => Err(Refusal::conflict(format!(
            "{id} was withdrawn, and a withdrawn decision never stands again: decide anew"
        ))),
        Some(_) => Ok(()),
    }
}

/// Every link names a decision held, and none would put `id` beneath itself.
fn linkable(
    ledger: &Ledger,
    id: &DecisionId,
    supersedes: &BTreeSet<DecisionId>,
) -> Result<(), Refusal> {
    for link in supersedes {
        if ledger.get(link).is_none() {
            return Err(Refusal::bad_request(format!(
                "{id} cannot supersede {}: no decision is named so",
                shown(link.as_str())
            )));
        }
        if ledger.below(id, link) {
            return Err(Refusal::bad_request(format!(
                "{id} cannot supersede {link}, which already stands above it"
            )));
        }
    }
    Ok(())
}
