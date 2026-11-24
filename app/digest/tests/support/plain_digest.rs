//! The plain digest: every line's shape and the history read afresh, what the box's kept reading
//! ([`super::Shape`] in the lines' fold, [`super::Kept`] per node) is tested to equal, for tests.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use prodrome_dag::registers::Node;
use prodrome_data::event::{Actor, Envelope, Hash, parents_of};
use prodrome_digest::Cycle;

use super::{Digest, Kept, Placed, Shape, under_kept};
use crate::secretary::digest_event::DigestEvent;
use crate::secretary::digest_line::Line;
use suzatary_conversations::secretary::store::Entry;
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_memory::secretary::salted_text::TextName;

/// The digest of `lines` (every conversation's objects, in the store's order) under `history`.
pub fn digest(
    lines: &[(&Hash, &Envelope<LineEvent>)],
    opened: &BTreeMap<Hash, Entry>,
    history: &[Node<DigestEvent>],
    owners: &BTreeSet<Actor>,
    words: impl Fn(&TextName) -> Option<Line>,
) -> Result<Digest, Cycle> {
    let parents: Vec<Vec<Hash>> = lines.iter().map(|(_, object)| parents_of(object)).collect();
    let placed: Vec<Placed<'_>> = lines
        .iter()
        .zip(&parents)
        .filter_map(|((name, object), parents)| Some((*name, object.event()?, parents.as_slice())))
        .collect();
    let shape = Shape::of(&placed, opened)?;
    Ok(under(Arc::new(shape), history, owners, words))
}

/// The digest of the lines `shape` holds under `history`, a retraction believed only from one of
/// `owners`.
pub fn under(
    shape: Arc<Shape>,
    history: &[Node<DigestEvent>],
    owners: &BTreeSet<Actor>,
    words: impl Fn(&TextName) -> Option<Line>,
) -> Digest {
    under_kept(shape, history, owners, words, &mut Kept::default())
}
