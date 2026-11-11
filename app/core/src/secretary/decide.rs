//! A recorded decision the secretary proposes and the owner's tap records: the proposal's
//! sentence, its wire shape, and the tap's perform, each arm the shared matches delegate to.
//!
//! The tap writes as the owner (the gate's actor), so a decision the secretary drafted from a
//! round that read mail stands only once he has read the card. A refusal at tap time (the id
//! decided meanwhile, a link gone, the markup refused) is an outcome, like a failed send.

use prodrome_core::event::Actor;

use crate::decisions::Decisions;
use crate::decisions::{DecisionId, Draft};
use crate::render::Render;
use crate::secretary::responded::{Done, Performed, Tried};
use crate::wire::inbox as wire;
use suzatary_base::capability::Clock;

/// The imperative a card and a list both print.
#[must_use]
pub fn sentence(draft: &Draft) -> String {
    let said = format!("record the decision {}: {}", draft.id(), draft.title());
    if draft.supersedes().is_empty() {
        return said;
    }
    let links: Vec<&str> = draft.supersedes().iter().map(DecisionId::as_str).collect();
    format!("{said}, superseding {}", links.join(", "))
}

/// The proposal as the page draws it.
#[must_use]
pub fn on_wire(draft: &Draft) -> wire::ProposedAct {
    wire::ProposedAct::Decision {
        id: draft.id().to_string(),
        title: draft.title().to_owned(),
        body: draft.body().to_owned(),
        rationale: draft.rationale().to_owned(),
        supersedes: draft.supersedes().iter().map(ToString::to_string).collect(),
        sentence: sentence(draft),
    }
}

/// Record a tapped decision as `by`, the gate's actor and never a request field.
#[must_use]
pub fn performed(
    (decisions, clock, render): (&Decisions<'_>, &dyn Clock, &dyn Render),
    draft: &Draft,
    by: &Actor,
) -> Performed {
    match decisions.decide(draft, clock.now(), by, render) {
        Ok(name) => Performed::Done(Done::Acted {
            what: format!("recorded the decision {} as {}", draft.id(), name.as_str()),
        }),
        Err(refusal) => Performed::Tried(Tried::NotSent {
            reason: refusal.message().to_owned(),
        }),
    }
}
