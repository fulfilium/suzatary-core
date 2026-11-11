//! The plain readings of the lines, each opening every slot of every line it is handed: what
//! the box's fold ([`crate::secretary::held_lines::HeldLines`]) is tested to equal, for tests.

use std::collections::BTreeMap;

use prodrome_core::event::{Envelope, Hash};

use super::{
    Named, Through, Transcript, by_conversation, erasures, lines_of, reseals, superseded,
    transcripts,
};
use crate::secretary::line_event::LineEvent;
use crate::secretary::slots::Slots;
use crate::secretary::store::{Entry, NotAConversation};
use suzatary_base::conversation_id::ConversationId;

/// Every line the objects hold, by name, read: each slot opened by the key `key` answers, or
/// erased where an erasure line of its own conversation names its key.
#[must_use]
pub fn opened<'l>(
    objects: impl IntoIterator<Item = impl Named<'l>>,
    slots: &impl Slots,
) -> BTreeMap<Hash, Entry> {
    by_conversation(lines_of(objects))
        .into_values()
        .flat_map(|lines| {
            let erased = erasures(lines.iter().copied());
            let superseded = superseded(lines.iter().copied(), slots);
            let reseals = reseals(lines.iter().copied());
            let read = Through {
                erased: &erased,
                superseded: &superseded,
                reseals: &reseals,
                slots,
            };
            lines
                .into_iter()
                .map(|(name, line)| (name.clone(), line.entry(&read)))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Every conversation the objects hold, read, whatever order they are handed over in.
///
/// # Errors
/// [`ReadFailed`]: objects that are no prodrome, or lines the fold refuses.
pub fn of_objects(
    objects: impl IntoIterator<Item = (Hash, Envelope<LineEvent>)>,
    slots: &impl Slots,
) -> Result<BTreeMap<ConversationId, Transcript>, ReadFailed> {
    let nodes = prodrome_core::dag::Dag::from_iter(objects)
        .nodes()
        .map_err(|failed| ReadFailed::Unreadable(failed.to_string()))?;
    Ok(transcripts(&nodes, slots)?)
}

/// A store whose conversations do not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadFailed {
    /// Its objects are no prodrome.
    #[error("the conversations do not read: {0}")]
    Unreadable(String),
    /// A conversation's lines the fold refuses.
    #[error(transparent)]
    Unfolded(#[from] NotAConversation),
}
