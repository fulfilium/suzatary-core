//! The digest's summaries counted, for the operator: every inner node of the tree by its label's
//! group, its level and how its summary stands, and the name of each node no model of the
//! ladder found a line for. Counts and names only, never a word.

use std::collections::BTreeMap;

use prodrome_core::digest::Node;
use prodrome_core::event::Hash;

use crate::secretary::compaction::{Compaction, Rung, short};
use crate::secretary::digest::{Digest, Standing};
use crate::secretary::digest_label::Label;

/// Who a node's label says its lines can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// The owner's words alone ([`Label::is_owners`]).
    Owners,
    /// Clean, and more than the owner's words.
    Clean,
    /// Somebody else's words may be in it.
    Tainted,
}

impl Group {
    /// The group of a node labelled `label`.
    #[must_use]
    pub fn of(label: &Label) -> Self {
        if label.is_owners() {
            Group::Owners
        } else if label.is_clean() {
            Group::Clean
        } else {
            Group::Tainted
        }
    }

    fn name(self) -> &'static str {
        match self {
            Group::Owners => "owner's words",
            Group::Clean => "clean",
            Group::Tainted => "tainted",
        }
    }
}

/// How a node's summary stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    /// A summary true to how the node is asked now.
    Current,
    /// A summary owed again, a summary written over a retracted one among them.
    Stale,
    /// No summary whose words read.
    Unsummarized,
    /// No summary, and every model of the ladder found no line for it.
    Unfit,
}

/// Every inner node, counted by group, level (a node over lines alone is level 1) and state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Census {
    counts: BTreeMap<(Group, usize, State), usize>,
    unfit: Vec<Hash>,
}

impl Census {
    /// The census of `digest`, a node's fit judged by `rule`'s ladder.
    #[must_use]
    pub fn of<R: Rung>(digest: &Digest, rule: &Compaction<R>) -> Self {
        fn walk(digest: &Digest, unfit: &[Hash], node: &Node<Label>, census: &mut Census) -> usize {
            if node.is_leaf() {
                return 0;
            }
            let level = 1 + node
                .children()
                .iter()
                .map(|child| walk(digest, unfit, child, census))
                .max()
                .unwrap_or(0);
            let state = match digest.standing(node.name()) {
                Some(Standing::Current) => State::Current,
                Some(Standing::Stale | Standing::OverRetracted) => State::Stale,
                None if unfit.contains(node.name()) => State::Unfit,
                None => State::Unsummarized,
            };
            *census
                .counts
                .entry((Group::of(node.measure()), level, state))
                .or_default() += 1;
            level
        }
        let mut census = Census {
            unfit: rule.unfit(digest),
            ..Census::default()
        };
        if let Some(tree) = digest.tree() {
            let unfit = census.unfit.clone();
            walk(digest, &unfit, tree.root(), &mut census);
        }
        census
    }

    /// How many nodes `keep` admits.
    #[must_use]
    pub fn count(&self, keep: impl Fn(Group, usize, State) -> bool) -> usize {
        self.counts
            .iter()
            .filter(|((group, level, state), _)| keep(*group, *level, *state))
            .map(|(_, count)| count)
            .sum()
    }
}

/// A total line, then a line per group and level.
impl std::fmt::Display for Census {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let line = |f: &mut std::fmt::Formatter<'_>, keep: &dyn Fn(Group, usize) -> bool| {
            let of = |state| self.count(|group, level, at| keep(group, level) && at == state);
            writeln!(
                f,
                "{} current, {} stale, {} unsummarized, {} unfit",
                of(State::Current),
                of(State::Stale),
                of(State::Unsummarized),
                of(State::Unfit)
            )
        };
        write!(f, "all nodes: ")?;
        line(f, &|_, _| true)?;
        let mut rows: Vec<(Group, usize)> = self
            .counts
            .keys()
            .map(|(group, level, _)| (*group, *level))
            .collect();
        rows.dedup();
        for (group, level) in rows {
            write!(f, "{}, level {level}: ", group.name())?;
            line(f, &|at, on| at == group && on == level)?;
        }
        if !self.unfit.is_empty() {
            let named: Vec<&str> = self.unfit.iter().map(short).collect();
            writeln!(f, "unfit for every model: {}", named.join(", "))?;
        }
        Ok(())
    }
}
