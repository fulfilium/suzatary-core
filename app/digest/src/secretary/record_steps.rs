//! Where a call's record is frozen: the part before the call's cache mark stands still while the
//! log grows, and what the log gained since stands after the mark.
//!
//! Write the log's lines as `ℓ₀ … ℓₙ₋₁`, line `i` costing `cᵢ` bytes of the record's print, and
//! `cum(k) = Σ_{i<k} cᵢ`. A line shown in words costs its words and its handle; any other costs
//! the most that it adds to a run of lines shown by their kinds.
//!
//! The frozen point is `F = max { k : cum(k) ≤ S·⌊cum(n)/S⌋ }` for a step `S`: a function of the
//! log alone, with no stored cursor and no clock, so two replicas holding the same lines freeze at
//! the same line. The record is `frozen ++ tail`. `frozen` is the view of the first `F` lines
//! under two waters ([`Waters`]), and `tail` is the lines `F … n`, one part each. Between two
//! steps `F` does not move, so `frozen` does not, and an arrival adds to `tail` alone. A step
//! moves `F` up, and `frozen` then grows at its end unless a merge falls in it (law 89 of the
//! Prodrome's SPEC; law 91 says a late summary can also change it). `tail` prints in fewer than
//! [`TAIL_BYTES`] bytes, so the part of a call that is paid in full at every call is bounded by
//! the step.
//!
//! The mark sits on the newest turn, so the record before it is read from the cache while it is
//! the same bytes as the call before's, which `frozen` is between steps.

use prodrome_digest::{Leaf, Measure, Node, Part, Tree, Waters};

use crate::secretary::conversations_log::Conversations;
use crate::secretary::digest::Digest;
use crate::secretary::digest_label::Label;
use crate::secretary::digest_view::{self, LINE_BYTES, Shown, VIEW_BYTES};
use crate::secretary::told;
use std::collections::BTreeMap;
use suzatary_conversations::secretary::store::Entry;

use prodrome_data::event::Hash;

/// How far the log grows, in bytes of lines, between two moves of the frozen point.
pub const STEP_BYTES: usize = 8_192;

/// The most bytes the record prints after the mark: a step of lines and the longest line that
/// passes it, with the heading that leads them and a handle.
pub const TAIL_BYTES: usize = STEP_BYTES + LINE_BYTES + 256;

/// The most a handle and its space add to a line of the record.
const HANDLE_BYTES: u64 = 17;

/// The most a run of lines shown by their kinds prints, for each line of it.
const HIDDEN_BYTES: u64 = 96;

/// How many bytes a merge of the frozen view closes below its high water, so a merge is rare: one
/// for each this many bytes the log grows.
pub const MERGE_BYTES: usize = 32_768;

/// How much of the record a call is told, and where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Room {
    /// The view of the whole log under one budget, all of it before the mark: what a reader whose
    /// record is told once is given, and the reading the stepped one is held to.
    Whole(usize),
    /// The record frozen at a point that moves in steps.
    Stepped(Steps),
}

impl Room {
    /// The most bytes of lines the record holds, the cut to the newest that fit.
    #[must_use]
    pub fn budget(self) -> usize {
        match self {
            Room::Whole(budget) => budget,
            Room::Stepped(_) => VIEW_BYTES,
        }
    }
}

impl From<usize> for Room {
    fn from(budget: usize) -> Self {
        Room::Whole(budget)
    }
}

impl From<Steps> for Room {
    fn from(steps: Steps) -> Self {
        Room::Stepped(steps)
    }
}

/// A frozen point's step, and the two waters of the view before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Steps {
    step: u64,
    high: u64,
    gap: u64,
}

impl Steps {
    /// The secretary's call: the frozen view's high water leaves [`VIEW_BYTES`] room for the
    /// longest tail ([`TAIL_BYTES`]), so the record never holds more than one budget.
    #[must_use]
    pub fn call() -> Self {
        Self::new(STEP_BYTES, VIEW_BYTES - TAIL_BYTES, MERGE_BYTES)
    }

    /// Steps of `step` bytes (at least one) over a frozen view that a merge closes `gap` bytes
    /// below its `high` water, or to nothing where it is less.
    #[must_use]
    pub fn new(step: usize, high: usize, gap: usize) -> Self {
        let bytes = |bytes: usize| u64::try_from(bytes).unwrap_or(u64::MAX);
        Self {
            step: bytes(step).max(1),
            high: bytes(high),
            gap: bytes(gap),
        }
    }

    /// The waters of the view before the mark: the low one never above the high one.
    fn waters(self) -> Waters {
        Waters::new(self.high, self.high.saturating_sub(self.gap))
            .unwrap_or_else(|| Waters::at(self.high))
    }
}

/// The record as parts: before the mark, and after it.
#[derive(Debug, Default)]
pub(crate) struct Split {
    pub frozen: Vec<Shown>,
    pub tail: Vec<Shown>,
}

/// The record of `digest` frozen at `steps`' point, each part shown to a reader that `shows` by
/// the rule of [`digest_view::view`]: the same labels, the same words, a line a part of its own
/// in the tail.
pub(crate) fn split(
    digest: &Digest,
    entries: &BTreeMap<&Hash, &Entry>,
    steps: Steps,
    shows: fn(&Label) -> bool,
) -> Split {
    let Some(tree) = digest.tree() else {
        return Split::default();
    };
    let log = Conversations::new(digest, entries);
    let lines = leaves(tree.root());
    let at = frozen_point(&lines, steps.step, |line| {
        if told::line_in_words(&digest.label(line), shows) {
            line.bytes() + HANDLE_BYTES
        } else {
            HIDDEN_BYTES
        }
    });
    let frozen = prefix(&lines[..at])
        .map(|prefix| {
            digest
                .view_over(&prefix, steps.waters())
                .parts
                .into_iter()
                .filter_map(|part| {
                    let node = named(&prefix, &part)?;
                    digest_view::shown_node(&log, part, node, shows)
                })
                .collect()
        })
        .unwrap_or_default();
    let tail = lines[at..]
        .iter()
        .filter_map(|&node| {
            let part = Part {
                node: node.name().clone(),
                span: node.span(),
            };
            digest_view::shown_node(&log, part, node, shows)
        })
        .collect();
    Split { frozen, tail }
}

/// How many of `lines` stand before the mark: `F`, over what each line costs in the record's
/// print (`cost`).
fn frozen_point<M>(lines: &[&Node<M>], step: u64, cost: impl Fn(&Node<M>) -> u64) -> usize {
    let all: u64 = lines.iter().map(|line| cost(line)).sum();
    let ceiling = all / step * step;
    let mut bytes = 0;
    lines
        .iter()
        .take_while(|line| {
            bytes += cost(line);
            bytes <= ceiling
        })
        .count()
}

/// The lines of the tree under `root`, in the log's order.
fn leaves<M>(root: &Node<M>) -> Vec<&Node<M>> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_leaf() {
            found.push(node);
        } else {
            stack.extend(node.children().iter().rev());
        }
    }
    found
}

/// The tree over `lines` alone, each resting on the one before, so the order is theirs: the
/// tree the log would have if it ended there. A node it closes by a line's name holds as in the
/// whole log's tree; the nodes at its right edge are its own.
fn prefix<M: Measure>(lines: &[&Node<M>]) -> Option<Tree<M>> {
    let chain = lines
        .iter()
        .enumerate()
        .map(|(at, node)| Leaf {
            name: node.name().clone(),
            parents: at
                .checked_sub(1)
                .map(|before| lines[before].name().clone())
                .into_iter()
                .collect(),
            bytes: u32::try_from(node.bytes()).unwrap_or(u32::MAX),
            measure: node.measure().clone(),
        })
        .collect();
    // Lines resting each on the one before have no cycle, and none at all have no tree.
    prodrome_digest::tree(chain).ok().flatten()
}

/// The node of `tree` that `part` stands for: of the nodes spanning its lines, the one it names.
fn named<'t, M>(tree: &'t Tree<M>, part: &Part) -> Option<&'t Node<M>> {
    let covering = tree.cover(part.span.clone())?;
    let &[mut node] = covering.as_slice() else {
        return None;
    };
    loop {
        if node.name() == &part.node {
            return Some(node);
        }
        let [only] = node.children() else {
            return None;
        };
        node = only;
    }
}
