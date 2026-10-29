//! The digest as a reading: every conversation's lines as ONE summary tree, read under the
//! summaries the digest's history records.
//!
//! What it means: the tree is the Prodrome's [`prodrome_core::digest::tree`] over the set of
//! lines, every line a leaf labelled by [`Label::of`] with its round's taint, so its shape is a function of the lines
//! and not of when each arrived; each node's summary is its register's reading in the history
//! ([`crate::secretary::digest_event`]), a discrete register, so two summaries of one node are a
//! conflict, shown whole, and a reading that can show one takes the candidate of the least object
//! name. A summary whose words no longer read is no summary. Nothing here calls a model or reads
//! a clock: the reading is a function of the two object sets and the words.
//!
//! A node is named by the lines it spans, so a line arriving late renames the spine above it,
//! and every summary of a renamed node stops being named by the tree. Those words are then dead
//! ([`Digest::naming`]), and the one sweep deletes them: an erasure needs no case of its own.
//!
//! A leaf is the line a line carries ([`Carried::From`](crate::secretary::carried::Carried::From)), or the line itself, so a store carried
//! forward reads as the same tree and every summary stands. A line whose words are erased stays in
//! the order, its leaf renamed over the keys that went
//! and its label marked erased, so the nodes above it rename, and with them, where a renamed node
//! moves a chunk's cut (a name ends a chunk one time in four), the chunk after it at that level:
//! no summary written while the words stood is named by the tree, every other summary stands, and
//! the compactor summarises again from what is left.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::digest::{self, Cycle, Leaf, Measure as _, Tree, View};
use prodrome_core::event::{Actor, Envelope, Hash, parents_of};
use prodrome_core::fold::Product as _;
use prodrome_core::fold::sole::{One, Only, Whole};
use prodrome_core::literal::Datetime;
use prodrome_core::registers::{Node, fold};
use prodrome_core::schema::Schema as _;
use prodrome_core::topo;

use crate::model_name::ModelName;
use crate::rule_version::Under;
use crate::secretary::compact_ask::{Asking, Beside};
use crate::secretary::digest_event::{DigestEvent, Said};
use crate::secretary::digest_label::Label;
use crate::secretary::digest_line::Line;
use crate::secretary::digest_policy::Owners;
use crate::secretary::digest_spent::Spent;
use crate::secretary::line_event::LineEvent;
use crate::secretary::salted_text::TextName;
use crate::secretary::store::Entry;
use crate::secretary::taint::Taint;
use crate::secretary::texts::{Liveness, Naming};

/// The conversations read through their summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digest {
    tree: Option<Tree<Label>>,
    said: BTreeMap<Hash, Vec<Candidate>>,
    /// Every summary's words the history names: its object, its node and its text.
    named: Vec<(Hash, Hash, TextName)>,
    /// Every call's object, instant and what it spent, in the history's order.
    attempts: Vec<(Hash, Datetime, Spent)>,
    /// The line of every leaf renamed because words of it were erased.
    renamed: BTreeMap<Hash, Hash>,
    /// Every summary object written over a retracted summary, at any depth: stale until its node
    /// is asked again.
    over_retracted: BTreeSet<Hash>,
}

/// One write of a node's reading: a summary, or its retraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The object that wrote it.
    pub object: Hash,
    /// What it says.
    pub said: Written,
}

/// Whether a summary is true to how a node is asked now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Standing {
    /// Asked under the current asking, over what the node's children read now, and over no
    /// retracted summary at any depth.
    Current,
    /// Asked under an older asking, or over a child's summary since replaced: owed again.
    Stale,
    /// Written over a summary the owner retracted, however deep, so its words may restate the
    /// line he said was wrong: owed again, and shown to no reader until it is.
    OverRetracted,
}

/// What a candidate says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Written {
    /// A summary.
    Summary(Summary),
    /// No answer fitted a node.
    Unfit {
        /// The model asked.
        model: ModelName,
        /// The most bytes a line could hold then.
        limit: u32,
        /// The shortest answer's length.
        shortest: u32,
    },
    /// The summary is wrong.
    Retracted,
}

/// A summary as the history records it. Its words are private to this module, so a reader
/// reaches them only through [`Digest::summary`], which shows no stale summary of an owner's node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Its words, `None` once deleted or lost.
    words: Option<Line>,
    /// Their name.
    pub text: TextName,
    /// What it was written from.
    pub over: Vec<Hash>,
    /// The model that wrote it.
    pub model: ModelName,
    /// Its length.
    pub bytes: u32,
    /// The node's taint when it was written.
    pub taint: Taint,
    /// The version of the asking it was written under.
    pub asked: Under<Asking>,
}

/// The digest of `lines` (every conversation's objects, each read as `opened` reads it) under
/// `history`, each summary's words read by `words`, a retraction believed only from one of
/// `owners`.
///
/// # Errors
/// [`Cycle`] for lines that rest on one another in a cycle, which no store of objects can hold.
pub fn digest(
    lines: &[(Hash, Envelope<LineEvent>)],
    opened: &BTreeMap<Hash, Entry>,
    history: &[Node<DigestEvent>],
    owners: &BTreeSet<Actor>,
    words: impl Fn(&TextName) -> Option<Line>,
) -> Result<Digest, Cycle> {
    let policy = Owners(owners);
    let taints = taints(lines, opened);
    let (carries, leaf_of) = leaves_of(lines);
    let order = line_order(lines, &carries)?;
    let leaves = order
        .iter()
        .enumerate()
        .filter_map(|(at, name)| {
            let line = opened.get(name)?;
            let (leaf, erased) = &leaf_of[name];
            let measure = Label::of(line, taints[name]);
            Some(Leaf {
                name: leaf.clone(),
                parents: at
                    .checked_sub(1)
                    .map(|before| leaf_of[&order[before]].0.clone())
                    .into_iter()
                    .collect(),
                // What showing the line costs is its words as the view prints them, never the
                // sealed print, so a store reads alike whether its words are locked or not.
                bytes: u32::try_from(crate::secretary::digest_view::rendered(line).len())
                    .unwrap_or(u32::MAX),
                measure: if *erased {
                    measure.join(&Label::erased())
                } else {
                    measure
                },
            })
        })
        .collect();
    let tree = digest::tree(leaves)?;
    let mut said: BTreeMap<Hash, Vec<Candidate>> = BTreeMap::new();
    let mut retracted = BTreeSet::new();
    for (node, stream) in fold(history).entities() {
        // A summary is retracted when a retraction the policy believes descends from it.
        for retraction in stream.iter().filter(|stamp| {
            matches!(stamp.event.body(), Said::Retracted)
                && matches!(
                    prodrome_core::policy::Policy::standing(&policy, &stamp.name, &stamp.event),
                    prodrome_core::policy::Standing::Binds
                )
        }) {
            retracted.extend(
                stream
                    .iter()
                    .filter(|stamp| matches!(stamp.event.body(), Said::Summarized { .. }))
                    .filter(|summary| retraction.descends(summary))
                    .map(|summary| summary.name.clone()),
            );
        }
        let registers: One<'_, Whole<DigestEvent>> =
            prodrome_core::fold::read(stream, None, &policy);
        said.entry(node.clone()).or_default().extend(
            registers.reading(Only).into_iter().filter_map(|stamp| {
                Some(Candidate {
                    object: stamp.name.clone(),
                    said: written(&stamp.event, &words)?,
                })
            }),
        );
    }
    for candidates in said.values_mut() {
        candidates.sort_by(|one, two| one.object.cmp(&two.object));
    }
    let named = history
        .iter()
        .filter_map(|object| match object.event()?.body() {
            Said::Summarized { text, .. } => Some((
                object.name().clone(),
                object.event()?.key().clone(),
                text.clone(),
            )),
            Said::Unfit { .. } | Said::Asked { .. } | Said::Retracted => None,
        })
        .collect();
    let attempts = attempts(history);
    let over_retracted = over_retracted(history, &retracted);
    Ok(Digest {
        tree,
        said,
        named,
        attempts,
        renamed: leaf_of
            .into_iter()
            .filter(|(line, (leaf, _))| leaf != *line)
            .map(|(line, (leaf, _))| (leaf, line.clone()))
            .collect(),
        over_retracted,
    })
}

/// Every summary in `history` written over one of `retracted`, or over a summary that is, so
/// a retraction reads as stale every summary of the spine above it.
fn over_retracted(history: &[Node<DigestEvent>], retracted: &BTreeSet<Hash>) -> BTreeSet<Hash> {
    let overs: BTreeMap<&Hash, &[Hash]> = history
        .iter()
        .filter_map(|object| match object.event()?.body() {
            Said::Summarized { over, .. } => Some((object.name(), over.as_slice())),
            Said::Unfit { .. } | Said::Asked { .. } | Said::Retracted => None,
        })
        .collect();
    let mut known = BTreeMap::new();
    overs
        .keys()
        .filter(|summary| stale(summary, &overs, retracted, &mut known))
        .map(|summary| (*summary).clone())
        .collect()
}

/// Whether `summary` is written over one of `retracted`, at any depth, each answer kept in
/// `known`. A summary is written over objects that exist before it, so the walk down ends.
fn stale<'h>(
    summary: &'h Hash,
    overs: &BTreeMap<&'h Hash, &'h [Hash]>,
    retracted: &BTreeSet<Hash>,
    known: &mut BTreeMap<&'h Hash, bool>,
) -> bool {
    if let Some(&read) = known.get(summary) {
        return read;
    }
    let read = overs.get(summary).is_some_and(|over| {
        over.iter()
            .any(|beneath| retracted.contains(beneath) || stale(beneath, overs, retracted, known))
    });
    known.insert(summary, read);
    read
}

/// Every call `history` records: its object, instant and what it spent, in the history's order.
/// The budget's fold, read without the lines.
#[must_use]
pub fn attempts(history: &[Node<DigestEvent>]) -> Vec<(Hash, Datetime, Spent)> {
    history
        .iter()
        .filter_map(|object| {
            let event = object.event()?;
            let sent = event.body().sent()?;
            Some((object.name().clone(), event.at(), Spent::call(sent)))
        })
        .collect()
}

/// Each line's place in the tree: the line it carries, and its leaf, that line renamed over its
/// keys an erasure names, and whether it was.
#[expect(
    clippy::type_complexity,
    reason = "two maps over the same lines, read together"
)]
fn leaves_of(
    lines: &[(Hash, Envelope<LineEvent>)],
) -> (BTreeMap<&Hash, &Hash>, BTreeMap<&Hash, (Hash, bool)>) {
    let gone = crate::secretary::transcript::erased(lines);
    let mut carries = BTreeMap::new();
    let mut leaves = BTreeMap::new();
    for (name, object) in lines {
        let Some(line) = object.event() else {
            continue;
        };
        // A reseal holds another slot's words, read where that slot stands: it is no leaf.
        if resealing(line) {
            continue;
        }
        let identity = line.identity(name);
        let keys: Vec<Hash> = line
            .keys()
            .filter(|key| gone.contains_key(*key))
            .filter_map(|key| Hash::new(key.as_str()).ok())
            .collect();
        let leaf = if keys.is_empty() {
            (identity.clone(), false)
        } else {
            let renamed =
                prodrome_core::memo::name(b"erased line", std::iter::once(identity).chain(&keys));
            (renamed, true)
        };
        carries.insert(name, identity);
        leaves.insert(name, leaf);
    }
    (carries, leaves)
}

/// Every line's taint in its round ([`crate::secretary::taint::rounds`]), each conversation's
/// lines taken in the order given, which is the store's causal order. A line whose words are
/// erased still counts: a mail read whose words went still taints the answer after it.
fn taints<'l>(
    lines: &'l [(Hash, Envelope<LineEvent>)],
    opened: &'l BTreeMap<Hash, Entry>,
) -> BTreeMap<&'l Hash, Taint> {
    let mut conversations: BTreeMap<_, Vec<(&Hash, &Entry)>> = BTreeMap::new();
    for (name, object) in lines {
        if let (Some(line), Some(entry)) = (object.event(), opened.get(name)) {
            conversations
                .entry(line.conversation())
                .or_default()
                .push((name, entry));
        }
    }
    conversations
        .into_iter()
        .flat_map(|(id, said)| {
            let taints = crate::secretary::taint::rounds(id, said.iter().map(|(_, entry)| *entry));
            said.into_iter().map(|(name, _)| name).zip(taints)
        })
        .collect()
}

/// The lines in the crate's one causal order ([`topo::linear`]) over the lines they carry, so a
/// carried store reads in its old order. Each line rests on the one before it, so the tree reads
/// exactly this order.
///
/// # Errors
/// [`Cycle`] for lines in a cycle, which no store of objects can hold.
fn line_order<'l>(
    lines: &'l [(Hash, Envelope<LineEvent>)],
    carries: &BTreeMap<&'l Hash, &'l Hash>,
) -> Result<Vec<&'l Hash>, Cycle> {
    // A reseal is no leaf, so a line resting on one rests on what the reseal rests on.
    let reseals: BTreeMap<&Hash, Vec<Hash>> = lines
        .iter()
        .filter(|(_, object)| object.event().is_some_and(resealing))
        .map(|(name, object)| (name, parents_of(object)))
        .collect();
    let parents: BTreeMap<&Hash, Vec<&Hash>> = lines
        .iter()
        .filter_map(|(name, object)| Some((carries.get(name).copied()?, object)))
        .map(|(carried, object)| {
            let mut pending = parents_of(object);
            let mut seen = BTreeSet::new();
            let mut found = Vec::new();
            while let Some(parent) = pending.pop() {
                if !seen.insert(parent.clone()) {
                    continue;
                }
                match (carries.get(&parent), reseals.get(&parent)) {
                    (Some(line), _) => found.push(*line),
                    (None, Some(beneath)) => pending.extend(beneath.iter().cloned()),
                    (None, None) => {}
                }
            }
            found.sort();
            found.dedup();
            (carried, found)
        })
        .collect();
    let line: BTreeMap<&Hash, &Hash> = carries
        .iter()
        .map(|(&name, &carried)| (carried, name))
        .collect();
    let order = topo::linear(&parents).map_err(|stuck| Cycle(stuck.clone()))?;
    Ok(order
        .into_iter()
        .filter_map(|carried| line.get(carried).copied())
        .collect())
}

/// Whether `line` seals another slot's words again.
fn resealing(line: &LineEvent) -> bool {
    matches!(
        line.shape(),
        crate::secretary::line_shape::LineShape::Resealed { .. }
    )
}

/// What a register's write says; `None` for a call, which writes no register.
fn written(event: &DigestEvent, words: impl Fn(&TextName) -> Option<Line>) -> Option<Written> {
    Some(match event.body() {
        Said::Summarized {
            text,
            over,
            model,
            bytes,
            taint,
            asked,
        } => Written::Summary(Summary {
            words: words(text),
            text: text.clone(),
            over: over.clone(),
            model: model.clone(),
            bytes: *bytes,
            taint: *taint,
            asked: asked.clone(),
        }),
        Said::Unfit {
            model,
            limit,
            shortest,
        } => Written::Unfit {
            model: model.clone(),
            limit: *limit,
            shortest: *shortest,
        },
        Said::Asked { .. } => return None,
        Said::Retracted => Written::Retracted,
    })
}

impl Digest {
    /// The line a leaf is: its own name, or the line whose leaf it is when that line was carried
    /// or words of it were erased.
    #[must_use]
    pub fn line<'n>(&'n self, leaf: &'n Hash) -> &'n Hash {
        self.renamed.get(leaf).unwrap_or(leaf)
    }

    /// The tree, `None` while there are no lines.
    #[must_use]
    pub fn tree(&self) -> Option<&Tree<Label>> {
        self.tree.as_ref()
    }

    /// Node `node`'s reading: one candidate, or a conflict's, by least object name first; empty
    /// for a node nothing was said of.
    #[must_use]
    pub fn candidates(&self, node: &Hash) -> &[Candidate] {
        self.said.get(node).map_or(&[], Vec::as_slice)
    }

    /// The summary a single-record reading shows: the least-named candidate's, when it is a
    /// summary whose words read, unless it is stale and its node is labelled the owner's alone,
    /// or it is written over a retracted summary. The first may have been asked beside more than
    /// the owner's words, and the second from the line he said was wrong, so no reader, the
    /// compactor included, is shown either: the node reads as unsummarized until it is asked
    /// again.
    #[must_use]
    pub fn summary(&self, node: &Hash) -> Option<&Line> {
        let (words, standing) = self.shown(node)?;
        match standing {
            Standing::Current => Some(words),
            Standing::Stale => {
                let owners = self.tree.as_ref()?.node(node)?.measure().is_owners();
                (!owners).then_some(words)
            }
            Standing::OverRetracted => None,
        }
    }

    /// How the summary [`Digest::summary`] would show stands, when its words read, whether it
    /// is shown or not: the census's reading, which names no word.
    #[must_use]
    pub fn standing(&self, node: &Hash) -> Option<Standing> {
        self.shown(node).map(|(_, standing)| standing)
    }

    /// The least-named candidate's summary, when its words read, and how it stands: current
    /// when it was asked under its node's class's current asking over what the node's children
    /// read now. Private, so a stale summary of an owner's node reaches no reader.
    fn shown(&self, node: &Hash) -> Option<(&Line, Standing)> {
        let candidate = self.candidates(node).first()?;
        let Written::Summary(Summary {
            words, over, asked, ..
        }) = &candidate.said
        else {
            return None;
        };
        let beside = Beside::of(self.tree.as_ref()?.node(node)?.measure());
        let standing = if self.over_retracted.contains(&candidate.object) {
            Standing::OverRetracted
        } else if asked.is_current(beside) && self.over(node).as_ref() == Some(over) {
            Standing::Current
        } else {
            Standing::Stale
        };
        Some((words.as_ref()?, standing))
    }

    /// What a summary of `node` is written over now, in the log's order: each child line's
    /// leaf, and each inner child's least-named candidate. `None` for a node the tree does not
    /// hold, or one with an inner child nothing was said of.
    #[must_use]
    pub fn over(&self, node: &Hash) -> Option<Vec<Hash>> {
        self.tree
            .as_ref()?
            .node(node)?
            .children()
            .iter()
            .map(|child| {
                Some(if child.is_leaf() {
                    child.name().clone()
                } else {
                    self.candidates(child.name()).first()?.object.clone()
                })
            })
            .collect()
    }

    /// The taint of the summary [`Digest::summary`] shows: the taint its event records joined
    /// with the node's own, so neither a summary written before a line's class was known nor one
    /// whose record says less than its lines reads cleaner than the lines it stands for.
    #[must_use]
    pub fn summary_taint(&self, node: &Hash) -> Option<Taint> {
        self.summary(node)?;
        let recorded = match &self.candidates(node).first()?.said {
            Written::Summary(summary) => summary.taint,
            Written::Unfit { .. } | Written::Retracted => return None,
        };
        let spans = self.tree.as_ref()?.node(node)?.measure().taint;
        Some(recorded.join(spans))
    }

    /// The reading's content name: the tree as read under its summaries
    /// ([`digest::read`]), every candidate of every node and every call, so any object the
    /// history gains that moves a register, or counts a call, moves the name.
    #[must_use]
    pub fn name(&self) -> crate::rule::ReadingName {
        let tree = self
            .tree
            .as_ref()
            .map(|tree| digest::read(tree, |node| self.summarized(node)));
        crate::rule::ReadingName::of(
            tree.iter()
                .chain(
                    self.said
                        .values()
                        .flatten()
                        .map(|candidate| &candidate.object),
                )
                .chain(self.attempts.iter().map(|(object, _, _)| object))
                .map(Hash::as_str),
        )
    }

    /// Every call the history records, by its instant and what it spent: what a budget folds.
    pub fn attempts(&self) -> impl Iterator<Item = (Datetime, Spent)> + '_ {
        self.attempts.iter().map(|(_, at, spent)| (*at, *spent))
    }

    /// What `node`'s summary costs in a view, if it has one to show.
    #[must_use]
    pub fn summarized(&self, node: &Hash) -> Option<u32> {
        self.summary(node)
            .map(|line| u32::try_from(line.as_str().len()).unwrap_or(u32::MAX))
    }

    /// The nodes whose summary can be written now ([`digest::pending`]), reading a stale
    /// summary as none: a node with no current summary whose children are lines or have current
    /// ones. So a stale node is asked again only over current children, lowest level first, and
    /// a parent written over a child's older summary is stale once the child is asked again.
    #[must_use]
    pub fn pending(&self) -> Vec<Hash> {
        self.tree.as_ref().map_or_else(Vec::new, |tree| {
            digest::pending(tree, |node| match self.shown(node)? {
                (words, Standing::Current) => {
                    Some(u32::try_from(words.as_str().len()).unwrap_or(u32::MAX))
                }
                (_, Standing::Stale | Standing::OverRetracted) => None,
            })
        })
    }

    /// The view under `budget` bytes ([`digest::view`]); `None` while there are no lines.
    #[must_use]
    pub fn view(&self, budget: u64) -> Option<View> {
        self.tree
            .as_ref()
            .map(|tree| digest::view(tree, |node| self.summarized(node), budget))
    }

    /// The texts the history names: a summary's, live while its node is in the tree and the
    /// summary is a candidate of the node's reading; past once superseded, retracted, or written
    /// of a node the lines no longer name.
    #[must_use]
    pub fn naming(&self) -> Naming<Line> {
        let mut in_tree = BTreeSet::new();
        let mut stack: Vec<_> = self.tree.iter().map(Tree::root).collect();
        while let Some(node) = stack.pop() {
            in_tree.insert(node.name());
            stack.extend(node.children());
        }
        self.named
            .iter()
            .map(|(object, node, text)| {
                let live = in_tree.contains(node)
                    && self
                        .candidates(node)
                        .iter()
                        .any(|candidate| &candidate.object == object);
                (
                    text.clone(),
                    if live { Liveness::Live } else { Liveness::Past },
                )
            })
            .collect()
    }
}
