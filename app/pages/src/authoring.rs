//! The one path every edit of a todo takes: the box prints the event (`POST /api/<chain>/author`)
//! and appends nothing, and the page that asked seals what it printed onto the chain's replica.

use suzatary_api::wire::{AuthoredPayload, Authoring};

use crate::places::Question;
use crate::runtime::{Act, Answer, Body, Effect, Secret, Told, answered};
use crate::todoparts::Seal;

/// Ask the box to print `body` for `chain`, answered once as `ask`.
#[must_use]
pub(crate) fn author<H, A, S>(ask: A, chain: &str, body: Authoring) -> Effect<H, A, S> {
    Effect::Post {
        ask,
        path: Question::Author(chain).path(),
        body: Body::Author(body),
    }
}

/// The todo an event is about, and the seals of what the box printed.
pub(crate) type Sealed<H, A, S> = (String, Vec<Effect<H, A, S>>);

/// What the box printed, sealed onto `chain`'s replica in its order (a subtodo's parent revision
/// after the subtodo), each answered as `ask` names it and its todo; the todo beside them.
///
/// # Errors
/// The box refused, in its words.
pub(crate) fn sealed<H, A, S>(
    chain: &str,
    answer: Answer,
    ask: impl Fn(&str, Seal) -> A,
) -> Result<Sealed<H, A, S>, String> {
    let printed: AuthoredPayload = answered(answer)?;
    let todo = printed.todo;
    let seals = std::iter::once((Seal::Event, printed.event))
        .chain(printed.revision.map(|r| (Seal::Parent, r)))
        .map(|(seal, event)| Effect::Act {
            ask: ask(&todo, seal),
            act: Act::Seal {
                chain: chain.to_owned(),
                event,
            },
            secret: Secret::None,
            told: Told::Answered,
        })
        .collect();
    Ok((todo, seals))
}
