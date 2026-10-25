//! The triage agent's whole surface: every verb a pure function of an [`Agent`] record,
//! which holds nothing that sends, confirms or names a peer.
//!
//! `verbs` holds one type per verb and `surface` sums them. `reads` holds the bodies the
//! secretary's verbs of the same names share; a shared writing verb builds its act once and hands
//! it to its surface's [`Holds::write`]: the triage agent files a card, and the secretary writes.

mod call;
mod holds;
pub(crate) mod priced;
pub(crate) mod reads;
mod remembered;
mod says;
pub(crate) mod sourced;
mod surface;
pub mod verbs;

use std::collections::BTreeMap;

use serde_json::{Map, Value};

pub use call::Call;
pub use holds::{Direct, Holds};
pub use says::Says;
pub use surface::{TriageCall, TriageSurface, TriageVerb, tools};
// Public so `app/cli/tests/live.rs` can weigh the answers on the live manuals.
pub use reads::{CONTEXT_CAP, CONTEXT_HEADROOM, context, manual};

use crate::capability::Agent;
use crate::deployment::Deployment;
use crate::record::TodoEvent;
use crate::refusal::{Refusal, shown};
use crate::store::Reads;

/// Who this surface is: a constant, never an argument. Its writes are cards, which the owner's
/// tap performs.
pub const WRITES_AS: crate::authoring::Writer = crate::authoring::Writer::Triage;

/// One call as the agent sent it: read whole once, then run against the record of the chain it
/// names, absent meaning the first.
///
/// # Errors
/// An unknown verb or chain, a missing or ill-typed argument, or an effect's own failure.
pub fn call(
    records: &BTreeMap<String, Agent<'_>>,
    cards: &crate::secretary::filed::TriageCards<'_>,
    deployment: &Deployment,
    verb: &str,
    args: &Map<String, Value>,
) -> Result<Value, Refusal> {
    let Some(verb) = TriageVerb::named(verb) else {
        return Err(Refusal::not_found(format!(
            "no verb named {} — this server exposes {}",
            shown(verb),
            TriageVerb::ALL
                .iter()
                .map(|verb| verb.name())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let request = verb.request(args)?;
    let chain = deployment.chain(request.chain.as_deref()).ok_or_else(|| {
        Refusal::not_found(format!(
            "no chain named {} — this deployment serves {}",
            shown(request.chain.as_deref().unwrap_or("")),
            deployment.chains.names().join(", ")
        ))
    })?;
    let caps = records.get(chain.name.as_str()).ok_or_else(|| {
        Refusal::internal(format!("no interpreters are bound to chain {}", chain.name))
    })?;
    // No identity number reaches the agent, whatever a verb read (`crate::held_value`).
    request
        .call
        .run(&Call {
            caps,
            deployment,
            chain,
            cards,
        })
        .map(crate::held_value::withheld_value)
}

/// Any effect's failure, as the words the agent reads.
pub(crate) fn refused<E: std::fmt::Display>(failed: &E) -> Refusal {
    Refusal::internal(failed.to_string())
}

/// A typed answer as the JSON a verb answers.
pub(crate) fn answered<T: serde::Serialize>(value: &T) -> Result<serde_json::Value, Refusal> {
    serde_json::to_value(value).map_err(|failed| refused(&failed))
}

fn events_of(chain: &dyn Reads<TodoEvent>) -> Result<Vec<TodoEvent>, Refusal> {
    let nodes = chain.nodes()?;
    Ok(crate::view::events_of(&nodes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_only_actor_this_surface_can_write_as_is_triage() {
        assert_eq!(WRITES_AS.actor(), "triage");
        assert_ne!(WRITES_AS, crate::authoring::Writer::Assistant);
    }
}
