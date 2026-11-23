//! What a replica holds, all of it, and what that means.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;

use crate::replica::frontier::Frontier;
use crate::replica::word::Word;
use crate::wire::store::SinceRead;

/// A node of a store, as a replica holds it: by its name, beside what it rests on.
pub trait Node: Clone {
    fn name(&self) -> &Hash;
    fn rests_on(&self) -> &[Hash];
    /// The prodrome the node names itself as in; a genesis names its own.
    fn prodrome(&self) -> Option<&Hash>;
}

/// What a replica holds, joined as a semilattice. Nodes of a prodrome the box no longer holds
/// are kept, since a later word may serve it again.
#[derive(Debug, Clone, PartialEq)]
pub struct Held<N> {
    /// How many resets came before it; no copy from before a reset reaches a later generation.
    generation: u64,
    nodes: BTreeMap<Hash, N>,
    /// One per key, in key order, all of the latest epoch, less each word whose heads rest
    /// strictly under another's.
    words: Vec<Word>,
    frontier: Frontier,
}

/// What one answer of the box says.
#[derive(Debug, Clone)]
pub struct Said<N> {
    pub heads: BTreeSet<Hash>,
    pub geneses: BTreeSet<Hash>,
    pub epoch: u64,
    pub nodes: Vec<N>,
    pub since: SinceRead,
    pub digest: Hash,
    pub count: u64,
}

impl<N: Node> Held<N> {
    #[must_use]
    pub fn empty(generation: u64) -> Self {
        Self::standing(generation, Frontier::Nothing)
    }

    /// Nothing held, standing at `frontier`: where a piece asked from a replica starts.
    #[must_use]
    pub fn standing(generation: u64, frontier: Frontier) -> Self {
        Held {
            generation,
            nodes: BTreeMap::new(),
            words: Vec::new(),
            frontier,
        }
    }

    /// The words are kept as a join keeps them.
    #[must_use]
    pub fn of(
        generation: u64,
        nodes: impl IntoIterator<Item = N>,
        words: Vec<Word>,
        frontier: Frontier,
    ) -> Self {
        let nodes: BTreeMap<Hash, N> = nodes
            .into_iter()
            .map(|node| (node.name().clone(), node))
            .collect();
        let words = spoken_both(&nodes, words);
        Held {
            generation,
            nodes,
            words,
            frontier,
        }
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Every node held, read or not.
    pub fn nodes(&self) -> impl Iterator<Item = &N> {
        self.nodes.values()
    }

    /// Whether the node `name` is held, read or not.
    #[must_use]
    pub fn contains(&self, name: &Hash) -> bool {
        self.nodes.contains_key(name)
    }

    #[must_use]
    pub fn words(&self) -> &[Word] {
        &self.words
    }

    #[must_use]
    pub fn frontier(&self) -> &Frontier {
        &self.frontier
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.words.is_empty() && self.frontier == Frontier::Nothing
    }

    /// 0 before the box has spoken: every epoch the box answers with is 1 or more.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.words.iter().map(|w| w.epoch).max().unwrap_or(0)
    }

    /// Within a generation the union of nodes, words and frontiers; a later generation wins whole.
    /// [`Held::empty`] of generation 0 is its unit.
    #[must_use]
    pub fn join(self, other: &Self) -> Self {
        match self.generation.cmp(&other.generation) {
            Ordering::Less => other.clone(),
            Ordering::Greater => self,
            Ordering::Equal => {
                let mut nodes = self.nodes;
                for (name, node) in &other.nodes {
                    nodes.entry(name.clone()).or_insert_with(|| node.clone());
                }
                let words = spoken_both(
                    &nodes,
                    self.words.into_iter().chain(other.words.iter().cloned()),
                );
                Held {
                    generation: self.generation,
                    nodes,
                    words,
                    frontier: self.frontier.later(other.frontier.clone()),
                }
            }
        }
    }

    /// What the box answered, heard at `fetched_at`, joined in.
    #[must_use]
    pub fn pulled(self, said: Said<N>, fetched_at: u64) -> Self {
        let Said {
            heads,
            geneses,
            epoch,
            nodes,
            since,
            digest,
            count,
        } = said;
        let frontier = self.frontier.answered(since, digest, count);
        let word = Word {
            epoch,
            heads,
            geneses,
            fetched_at,
        };
        let theirs = Held::of(self.generation, nodes, vec![word], frontier);
        self.join(&theirs)
    }

    /// The word whose heads reach the most nodes, then the later contact, then the greater key.
    #[must_use]
    pub fn spoken(&self) -> Option<&Word> {
        let one = self.words.len() == 1;
        self.words
            .iter()
            .map(|word| {
                let reach = if one {
                    0
                } else {
                    reached(&self.nodes, &word.heads)
                };
                (reach, word)
            })
            .max_by(|(r, a), (s, b)| (r, a.fetched_at, a.key()).cmp(&(s, b.fetched_at, b.key())))
            .map(|(_, word)| word)
    }

    /// The box's word, the prodromes it no longer names, and the nodes forgotten with them.
    #[must_use]
    pub fn read(&self) -> Reading<'_, N> {
        let word = self.spoken();
        let gone: BTreeSet<Hash> = match word {
            None => BTreeSet::new(),
            Some(word) => self
                .nodes
                .values()
                .filter_map(Node::prodrome)
                .filter(|genesis| !word.geneses.contains(*genesis))
                .cloned()
                .collect(),
        };
        let lost = forgotten(&self.nodes, &gone);
        Reading {
            held: self,
            word,
            gone,
            lost,
        }
    }
}

pub struct Reading<'h, N> {
    held: &'h Held<N>,
    word: Option<&'h Word>,
    gone: BTreeSet<Hash>,
    lost: BTreeSet<Hash>,
}

impl<'h, N: Node> Reading<'h, N> {
    #[must_use]
    pub fn word(&self) -> Option<&'h Word> {
        self.word
    }

    #[must_use]
    pub fn gone(&self) -> &BTreeSet<Hash> {
        &self.gone
    }

    /// Every node of a prodrome the box holds, and of none.
    pub fn nodes(&self) -> impl Iterator<Item = &'h N> + '_ {
        self.held
            .nodes
            .values()
            .filter(|node| !self.lost.contains(node.name()))
    }

    #[must_use]
    pub fn holds(&self, name: &Hash) -> bool {
        self.held.nodes.contains_key(name) && !self.lost.contains(name)
    }

    #[must_use]
    pub fn heads(&self) -> BTreeSet<Hash> {
        let named: BTreeSet<&Hash> = self.nodes().flat_map(Node::rests_on).collect();
        self.nodes()
            .map(Node::name)
            .filter(|name| !named.contains(name))
            .cloned()
            .collect()
    }

    /// Whether the box's `heads` name a node not read: a move of the store, or a pull cut short.
    #[must_use]
    pub fn behind(&self, heads: &BTreeSet<Hash>) -> bool {
        heads.iter().any(|head| !self.holds(head))
    }
}

/// Every node `names` rest on, themselves included, or none when one of them is not held.
fn under<'n, N: Node>(
    nodes: &'n BTreeMap<Hash, N>,
    names: &'n BTreeSet<Hash>,
) -> Option<BTreeSet<&'n Hash>> {
    let mut seen = BTreeSet::new();
    let mut left: Vec<&Hash> = names.iter().collect();
    while let Some(name) = left.pop() {
        if seen.contains(name) {
            continue;
        }
        let node = nodes.get(name)?;
        seen.insert(node.name());
        left.extend(node.rests_on());
    }
    Some(seen)
}

/// How many held nodes `names` rest on, themselves included, passing over what is not held.
fn reached<N: Node>(nodes: &BTreeMap<Hash, N>, names: &BTreeSet<Hash>) -> usize {
    let mut seen = BTreeSet::new();
    let mut left: Vec<&Hash> = names.iter().collect();
    while let Some(name) = left.pop() {
        if seen.contains(name) {
            continue;
        }
        if let Some(node) = nodes.get(name) {
            seen.insert(node.name());
            left.extend(node.rests_on());
        }
    }
    seen.len()
}

/// Whether `a` differs from `b` and `b` rests on every name of `a`, read off nodes held.
fn below<N: Node>(nodes: &BTreeMap<Hash, N>, a: &BTreeSet<Hash>, b: &BTreeSet<Hash>) -> bool {
    a != b
        && under(nodes, a).is_some()
        && under(nodes, b).is_some_and(|y| a.iter().all(|name| y.contains(name)))
}

/// The latest epoch's words, one per key at its latest contact, less each whose heads rest strictly
/// under another's. Nodes only grow, so a word once under another stays under it, and the union
/// stays associative.
fn spoken_both<N: Node>(
    nodes: &BTreeMap<Hash, N>,
    words: impl IntoIterator<Item = Word>,
) -> Vec<Word> {
    let words: Vec<Word> = words.into_iter().collect();
    let Some(latest) = words.iter().map(|w| w.epoch).max() else {
        return words;
    };
    let mut all: Vec<Word> = words.into_iter().filter(|w| w.epoch == latest).collect();
    all.sort_by(|a, b| a.key().cmp(&b.key()).then(b.fetched_at.cmp(&a.fetched_at)));
    all.dedup_by(|later, kept| later.key() == kept.key());
    if all.len() < 2 {
        return all;
    }
    let keep: Vec<bool> = all
        .iter()
        .map(|w| !all.iter().any(|v| below(nodes, &w.heads, &v.heads)))
        .collect();
    all.into_iter()
        .zip(keep)
        .filter_map(|(word, kept)| kept.then_some(word))
        .collect()
}

fn forgotten<N: Node>(nodes: &BTreeMap<Hash, N>, gone: &BTreeSet<Hash>) -> BTreeSet<Hash> {
    let mut lost = BTreeSet::new();
    if gone.is_empty() {
        return lost;
    }
    let mut children: BTreeMap<&Hash, Vec<&Hash>> = BTreeMap::new();
    for node in nodes.values() {
        for parent in node.rests_on() {
            children.entry(parent).or_default().push(node.name());
        }
    }
    let mut left: Vec<&Hash> = nodes
        .values()
        .filter(|node| node.prodrome().is_some_and(|p| gone.contains(p)))
        .map(Node::name)
        .collect();
    while let Some(name) = left.pop() {
        if lost.insert(name.clone()) {
            left.extend(children.get(name).into_iter().flatten());
        }
    }
    lost
}
