//! The inbox and the two topic stores as a device holds them: each a replica of objects
//! ([`Objects`](crate::replica::objects::Objects)) at its own schema. This reads the box's
//! answers into what such a replica takes: a pull's piece ([`said`]) and a post's receipt
//! ([`answers`]).

use std::collections::BTreeSet;

use prodrome_core::event::Hash;
use prodrome_core::schema::Schema;

use crate::replica::held::Said;
use crate::replica::objects::{Answer, Object, Refusal};
use crate::secretary::held_replica::NotAnAnswer;
use crate::wire::lines::{ObjectsPayload, ReceiptPayload};
use crate::wire::signatures::RefusalKind;

/// The piece `answer` the box served, its objects read at `schema`.
///
/// # Errors
/// [`NotAnAnswer::Name`] for a head, a genesis or the frontier's digest that is no name.
pub fn said<E: Schema>(
    schema: &E::Vocabulary,
    answer: ObjectsPayload,
) -> Result<Said<Object>, NotAnAnswer> {
    let ObjectsPayload {
        heads,
        geneses,
        epoch,
        objects,
        since,
        frontier,
        ..
    } = answer;
    Ok(Said {
        heads: names(heads)?,
        geneses: names(geneses)?,
        epoch,
        nodes: objects
            .into_iter()
            .map(|o| Object::read::<E>(schema, o.literal))
            .collect(),
        since,
        digest: name(frontier.digest)?,
        count: u64::try_from(frontier.count).unwrap_or(u64::MAX),
    })
}

/// What `receipt` answers of the objects posted: each adopted or already held is taken, and each
/// refused is refused in the box's words. A refusal of bytes that were no object names nothing.
#[must_use]
pub fn answers(receipt: &ReceiptPayload) -> Vec<(Hash, Answer)> {
    let taken = receipt
        .adopted
        .iter()
        .chain(&receipt.already)
        .filter_map(|n| Hash::new(n.clone()).ok())
        .map(|n| (n, Answer::Taken));
    let refused = receipt.refused.iter().filter_map(|r| {
        Hash::new(r.hash.clone()).ok().map(|n| {
            let kind = match r.kind {
                RefusalKind::Final => Refusal::Final,
                RefusalKind::Trust => Refusal::Trust,
            };
            (
                n,
                Answer::Refused {
                    kind,
                    reason: r.reason.clone(),
                },
            )
        })
    });
    taken.chain(refused).collect()
}

fn name(text: String) -> Result<Hash, NotAnAnswer> {
    Hash::new(text.clone()).map_err(|_| NotAnAnswer::Name(text))
}

fn names(texts: Vec<String>) -> Result<BTreeSet<Hash>, NotAnAnswer> {
    texts.into_iter().map(name).collect()
}
