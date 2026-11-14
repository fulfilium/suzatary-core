use serde_json::{Value, json};

use crate::secretary::changes::{self, Building, Changeset};
use crate::secretary::filed::triage_card;
use crate::secretary::inbox::{self, Object};
use crate::secretary::{Bound, Call, SecretarySurface, bound_named};
use crate::verb::{NoArgs, Verb};
use suzatary_base::refusal::Refusal;

/// Every card the triage agent filed that still waits for the owner.
pub struct TriageCards;

impl Verb for TriageCards {
    type Surface = SecretarySurface;
    type Args = NoArgs;
    type Answer = Value;
    const NAME: &'static str = "triage_cards";
    const DESCRIPTION: &'static str = "EVERY OPEN CARD THE TRIAGE AGENT FILED, on every chain, as the \
        owner's Inbox draws it: the card's reference, the agent's `why` and `sources`, and \
        every change as a readable diff — what is being added, completed, cancelled, \
        repriced or reworded, and the before and after of each field.\n\n\
        A card holds every act the agent asked for about ONE todo, in order. \
        `pending_todo` on a row means it records an outcome for a todo only ANOTHER card \
        would put on the chain. Only the owner's tap performs a card; you may withdraw one \
        with `triage_withdraw`.";

    fn run(call: &Call<'_>, _: NoArgs) -> Result<Value, Refusal> {
        Ok(answered(&listed(call.chains, call.deployment)?))
    }
}

/// One open triage card and its changeset.
fn open_card(object: &Object) -> Option<&Changeset> {
    triage_card(object).filter(|_| object.reading.performs())
}

/// Every open triage card: its reference and its changeset.
///
/// # Errors
/// The inbox does not read.
pub fn standing(
    inbox: &dyn suzatary_store::store::Prodrome<crate::secretary::inbox_event::InboxEvent>,
) -> Result<Vec<(suzatary_base::reference::ProposalRef, Changeset)>, Refusal> {
    Ok(inbox::fold(inbox)?
        .objects()
        .filter_map(|object| {
            open_card(object).map(|changeset| (object.asked.asked.clone(), changeset.clone()))
        })
        .collect())
}

/// Every open triage card, each drawn against its chain as the tap would build it.
///
/// # Errors
/// The inbox or a chain that does not read.
pub fn listed(
    chains: &[Bound<'_>],
    deployment: &crate::deployment::Deployment,
) -> Result<Vec<Value>, Refusal> {
    let Some(first) = chains.first() else {
        return Ok(Vec::new());
    };
    let caps = &first.caps;
    let fold = inbox::fold(caps.inbox)?;
    let categories: Vec<&str> = deployment.categories.keys().map(String::as_str).collect();
    let now = suzatary_base::zone::Moment {
        at: caps.clock.now(),
        zone: deployment.zone,
    };
    fold.objects()
        .filter_map(|object| open_card(object).map(|changeset| (object, changeset)))
        .map(|(object, changeset)| {
            let (rows, problem) = match bound_named(chains, changeset.chain()) {
                None => (
                    Vec::new(),
                    format!("this box serves no chain named {}", changeset.chain()),
                ),
                Some(bound) => {
                    let preview = crate::believed::Preview::of(bound.caps.chain.objects()?)
                        .map_err(|failed| Refusal::internal(failed.to_string()))?;
                    changes::shown(
                        changeset,
                        &preview,
                        &Building {
                            now,
                            by: crate::secretary::WRITES_AS,
                            categories: &categories,
                            untrusted: &bound.policy.untrusted,
                            typeset: bound.caps.render,
                        },
                    )
                }
            };
            Ok(json!({
                "card": object.asked.asked.to_string(),
                "chain": changeset.chain(),
                "why": object.asked.reasons.why(),
                "sources": object
                    .asked
                    .reasons
                    .sources()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
                "changes": rows,
                "problem": problem,
            }))
        })
        .collect()
}

/// What the verb answers over `listed`: the cards, and what to do with them.
#[must_use]
pub fn answered(listed: &[Value]) -> Value {
    json!({
        "cards": listed,
        "next": "Withdraw a duplicate or noise with triage_withdraw, naming the card and saying \
                 WHY. Leave every other card for the owner's tap, and say in your answer which \
                 ones you left and why. When in doubt, leave it.",
    })
}
