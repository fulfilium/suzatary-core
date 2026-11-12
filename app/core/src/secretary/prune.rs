//! What pruning the lines would let go: every line no key the box holds opens a word of (a seal a
//! reseal superseded, or an erased slot) is kept and sent as its husk, so its sealed bytes are
//! dead weight wherever they stand. The census reads the lines and writes nothing; it is the dry
//! run of the store's own prune, which the Prodrome's store must first learn to read
//! (a print kept under a name it does not hash to).

use std::collections::BTreeMap;

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
    /// The lines no key opens.
    pub dead: Tally,
    /// The bytes of their husks.
    pub husks: usize,
    /// Kept whole and past the threshold, by shape.
    pub large: BTreeMap<String, Tally>,
}

/// The census of the lines, an object past `large` bytes counted as large.
pub fn census(conversations: Conversations<'_>, large: usize) -> Result<Census, Refusal> {
    let graph = conversations.lines.graph()?;
    let names: Vec<_> = graph.objects().map(|(name, _)| name.clone()).collect();
    let sizes = conversations.lines.sizes(&names)?;
    let husks = crate::secretary::husks::husks(conversations, &graph, &names)?;
    let mut census = Census::default();
    for (name, bytes) in names
        .iter()
        .zip(sizes)
        .filter_map(|(name, size)| Some((name, size?)))
    {
        census.all.add(bytes);
        if let Some(husk) = husks.get(name) {
            census.dead.add(bytes);
            census.husks += husk.len();
        } else if bytes > large {
            let kind = graph
                .dag()
                .get(name)
                .and_then(|object| object.event())
                .map_or_else(
                    || "structure".to_owned(),
                    |line| line.shape().name().to_owned(),
                );
            census.large.entry(kind).or_default().add(bytes);
        }
    }
    Ok(census)
}
