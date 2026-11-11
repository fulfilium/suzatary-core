//! The plain digest: every line's shape and the history read afresh, what the box's kept reading
//! ([`super::Shape`] in the lines' fold, [`super::Kept`] per node) is tested to equal, for tests.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use prodrome_core::digest::Cycle;
use prodrome_core::event::{Actor, Envelope, Hash, parents_of};
use prodrome_core::registers::Node;

use super::{Digest, Kept, Placed, Shape, under_kept};
use crate::secretary::digest_event::DigestEvent;
use crate::secretary::digest_line::Line;
use crate::secretary::line_event::LineEvent;
use crate::secretary::salted_text::TextName;
use crate::secretary::store::Entry;

/// The digest of `lines` (every conversation's objects, in the store's order, each read as
/// `opened` reads it) under `history`: [`under`] of their [`Shape`].
///
/// # Errors
/// [`Cycle`] for lines that rest on one another in a cycle, which no store of objects can hold.
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

/// The digest of the lines `shape` holds under `history`, each summary's words read by `words`,
/// a retraction believed only from one of `owners`.
pub fn under(
    shape: Arc<Shape>,
    history: &[Node<DigestEvent>],
    owners: &BTreeSet<Actor>,
    words: impl Fn(&TextName) -> Option<Line>,
) -> Digest {
    under_kept(shape, history, owners, words, &mut Kept::default())
}
