//! What pruning the lines lets go: every line no key opens a word of, now or later, is kept as its
//! husk ([`crate::secretary::husks::dead`]). The census reads the lines and writes nothing; the store's
//! prune (`suzatary_io::husked::Husked::husk`) writes what it names.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;

use crate::secretary::conversations::Conversations;
use suzatary_base::refusal::Refusal;

/// Objects and their bytes on disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub objects: usize,
    pub bytes: usize,
}

impl Tally {
    fn add(&mut self, bytes: usize) {
        self.objects += 1;
        self.bytes += bytes;
    }
}

/// The lines as a prune would leave them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Census {
    pub all: Tally,
    /// The lines held whole that no key opens a word of, now or later: what a prune husks.
    pub dead: BTreeSet<Hash>,
    /// Their bytes on disk.
    pub dead_bytes: usize,
    /// The bytes of their husks.
    pub husks: usize,
    /// The lines the box sends as husks and keeps whole, since a key of theirs is retired and
    /// not destroyed yet.
    pub retired: Tally,
    /// Kept whole and past the threshold, by shape.
    pub large: BTreeMap<String, Tally>,
}

/// The census of the lines, an object past `large` bytes counted as large.
pub fn census(conversations: Conversations<'_>, large: usize) -> Result<Census, Refusal> {
    let graph = conversations.lines.graph()?;
    let names: Vec<_> = graph.objects().map(|(name, _)| name.clone()).collect();
    let sizes = conversations.lines.sizes(&names)?;
    let husks = crate::secretary::husks::husks(conversations, &graph, &names)?;
    let mut dead = crate::secretary::husks::dead(conversations, &graph, &names)?;
    dead.retain(|name| {
        graph.dag().get(name).is_some_and(|object| match object {
            prodrome_core::event::Envelope::Change(change) => {
                suzatary_store::store::Husking::holds_sealed(&change.event)
            }
            _ => false,
        })
    });
    let mut census = Census::default();
    for (name, bytes) in names
        .iter()
        .zip(sizes)
        .filter_map(|(name, size)| Some((name, size?)))
    {
        census.all.add(bytes);
        let line = graph.dag().get(name).and_then(|object| object.event());
        match husks.get(name) {
            Some(husk) if dead.contains(name) => {
                census.dead_bytes += bytes;
                census.husks += husk.len();
            }
            Some(_) if line.is_some_and(suzatary_store::store::Husking::holds_sealed) => {
                census.retired.add(bytes);
            }
            None if bytes > large => {
                let kind = line.map_or_else(
                    || "structure".to_owned(),
                    |line| line.shape().name().to_owned(),
                );
                census.large.entry(kind).or_default().add(bytes);
            }
            // Held as a husk already, or kept whole and small.
            Some(_) | None => {}
        }
    }
    census.dead = dead;
    Ok(census)
}

/// A finding for each line `was` names, beside the bytes a commit held under its name, unless the
/// commit held the whole the name hashes, the store's file (`now`) is that whole's husk, and no
/// key opens a word of it now or later.
pub fn admitted(
    conversations: Conversations<'_>,
    was: &[(Hash, Vec<u8>)],
    now: impl Fn(&Hash) -> Option<Vec<u8>>,
) -> Result<Vec<String>, Refusal> {
    use crate::secretary::line_event::{LineEvent, LineVocabulary};
    use suzatary_store::store::HuskObject;
    let graph = conversations.lines.graph()?;
    let names: Vec<Hash> = was.iter().map(|(name, _)| name.clone()).collect();
    let dead = crate::secretary::husks::dead(conversations, &graph, &names)?;
    Ok(was
        .iter()
        .filter(|(name, whole)| {
            let whole = prodrome_core::dag::decode::<LineEvent>(&LineVocabulary, name, whole).ok();
            let husk = now(name)
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .and_then(|text| HuskObject::<LineEvent>::read(name, &text).ok());
            husk.is_none()
                || husk
                    != match whole {
                        Some(prodrome_core::event::Envelope::Change(change)) => {
                            HuskObject::of(&change)
                        }
                        _ => None,
                    }
                || !dead.contains(name)
        })
        .map(|(name, _)| {
            format!(
                "{}: not the husk of the commit's whole, or of a dead line",
                name.as_str()
            )
        })
        .collect())
}

/// A finding for each husk the lines hold under a line a key may still open: one the prune did not
/// write, which reads as words gone that are not.
pub fn stray(conversations: Conversations<'_>) -> Result<Vec<String>, Refusal> {
    use suzatary_store::store::Husking as _;
    let graph = conversations.lines.graph()?;
    let husks: Vec<Hash> = graph
        .objects()
        .filter(|(_, object)| {
            object
                .event()
                .is_some_and(|line| line.keys().next().is_some() && !line.holds_sealed())
        })
        .map(|(name, _)| name.clone())
        .collect();
    let dead = crate::secretary::husks::dead(conversations, &graph, &husks)?;
    Ok(husks
        .iter()
        .filter(|name| !dead.contains(*name))
        .map(|name| format!("{} is a husk of a line a key may still open", name.as_str()))
        .collect())
}
