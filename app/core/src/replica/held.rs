//! What a replica holds, all of it, and what that means.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;

use crate::replica::frontier::Frontier;
use crate::replica::word::Word;
use crate::wire::store::SinceRead;

/// A node of a store, as a replica holds it: by its name, beside what it rests on.
pub trait Node: Clone {
    /// The node's name.
    fn name(&self) -> &Hash;
    /// What it rests on.
    fn rests_on(&self) -> &[Hash];
    /// The prodrome the node names itself as in, where it names one: a genesis names its own.
    fn prodrome(&self) -> Option<&Hash>;
}

/// What a replica holds: the state a join unites, read for its meaning by [`Held::read`]. The
/// nodes of a prodrome the box no longer holds are kept, since a later word may serve it again.
#[derive(Debug, Clone, PartialEq)]
pub struct Held<N> {
    /// How many resets came before it: a reset starts a later generation, which no copy from
    /// before it reaches.
    generation: u64,
    nodes: BTreeMap<Hash, N>,
    /// One per key, in key order, all of the latest epoch, less each word whose heads rest
    /// strictly under another's: such a word never wins.
    words: Vec<Word>,
    frontier: Frontier,
}

/// What one answer of the box says, about the nodes it sends.
#[derive(Debug, Clone)]
pub struct Said<N> {
    /// The box's heads.
    pub heads: BTreeSet<Hash>,
    /// The prodromes it holds.
    pub geneses: BTreeSet<Hash>,
    /// Its clock over the store.
    pub epoch: u64,
    /// What it sent.
    pub nodes: Vec<N>,
    /// How it read the frontier asked from.
    pub since: SinceRead,
    /// What it recorded the frontier it names under.
    pub digest: Hash,
    /// The nodes at or under that frontier.
    pub count: u64,
}

impl<N: Node> Held<N> {
    /// Nothing held, in generation `generation`.
    #[must_use]
    pub fn empty(generation: u64) -> Self {
        Self::standing(generation, Frontier::Nothing)
    }

    /// Nothing held, in generation `generation`, standing at `frontier`: where a piece asked from
    /// a replica starts.
    #[must_use]
    pub fn standing(generation: u64, frontier: Frontier) -> Self {
        Held {
            generation,
            nodes: BTreeMap::new(),
            words: Vec::new(),
            frontier,
        }
    }

    /// `nodes` and `words` held in generation `generation` at `frontier`, the words kept as a
    /// join keeps them.
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

    /// How many resets came before it.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Every node held, read or not, by name.
    pub fn nodes(&self) -> impl Iterator<Item = &N> {
        self.nodes.values()
    }

    /// What the box said, in key order.
    #[must_use]
    pub fn words(&self) -> &[Word] {
        &self.words
    }

    /// Where it stands with the box.
    #[must_use]
    pub fn frontier(&self) -> &Frontier {
        &self.frontier
    }

    /// Whether it holds nothing, said nothing and stands nowhere: the unit of the join within its
    /// generation.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.words.is_empty() && self.frontier == Frontier::Nothing
    }

    /// The epoch of the box's words it holds, 0 before the box has spoken: every epoch the box
    /// answers with is 1 or more.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.words.iter().map(|w| w.epoch).max().unwrap_or(0)
    }

    /// The join: within a generation the union of the nodes, the words and the frontiers, so its
    /// laws are the union's; a later generation kept whole over an earlier one, so a reset stands
    /// whatever copy from before it is joined in. [`Held::empty`] of generation 0 is its unit.
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

    /// What the box answered, `said` heard at `fetched_at`, joined in: its nodes, and its word,
    /// with the prodromes it holds; the frontier it names read against this one's.
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

    /// The box's word of those held, all of one epoch: the one whose heads reach the most nodes,
    /// then the later contact, then the greater key.
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

    /// What it holds means: the box's word, the prodromes that word no longer names, and the
    /// nodes it forgets, each of a gone prodrome or resting on one.
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

/// A replica read: the box's word, kept whole, and the nodes of the prodromes it holds.
pub struct Reading<'h, N> {
    held: &'h Held<N>,
    word: Option<&'h Word>,
    gone: BTreeSet<Hash>,
    lost: BTreeSet<Hash>,
}

impl<'h, N: Node> Reading<'h, N> {
    /// The box's word, or none before it has spoken.
    #[must_use]
    pub fn word(&self) -> Option<&'h Word> {
        self.word
    }

    /// The prodromes held that the box's word no longer names.
    #[must_use]
    pub fn gone(&self) -> &BTreeSet<Hash> {
        &self.gone
    }

    /// Every node read: those of a prodrome the box holds, and those of none.
    pub fn nodes(&self) -> impl Iterator<Item = &'h N> + '_ {
        self.held
            .nodes
            .values()
            .filter(|node| !self.lost.contains(node.name()))
    }

    /// Whether `name` is read.
    #[must_use]
    pub fn holds(&self, name: &Hash) -> bool {
        self.held.nodes.contains_key(name) && !self.lost.contains(name)
    }

    /// The nodes read that no node read rests on.
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

/// Whether `a` rests strictly under `b`: another set, every name of which `b` rests on, both
/// read off nodes held.
fn below<N: Node>(nodes: &BTreeMap<Hash, N>, a: &BTreeSet<Hash>, b: &BTreeSet<Hash>) -> bool {
    a != b
        && under(nodes, a).is_some()
        && under(nodes, b).is_some_and(|y| a.iter().all(|name| y.contains(name)))
}

/// The union of words, read as the box's register: the latest epoch's alone; within it one per
/// key, its latest contact, less each word whose heads rest strictly under another's. Under is
/// read off the nodes, which only grow and never change what a held node rests on, so a word once
/// under another stays under it in every larger union, and the union stays associative.
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

/// The nodes of a gone prodrome, and every node resting on one.
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
