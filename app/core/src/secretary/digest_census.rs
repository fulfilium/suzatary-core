//! The digest's summaries counted, for the operator: every inner node of the tree by its label's
//! group, its level and how its summary stands. Counts only, never a word.

use std::collections::BTreeMap;

use prodrome_core::digest::Node;

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
    /// A summary owed again.
    Stale,
    /// No summary whose words read.
    Unsummarized,
}

/// Every inner node, counted by group, level (a node over lines alone is level 1) and state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Census {
    counts: BTreeMap<(Group, usize, State), usize>,
}

impl Census {
    /// The census of `digest`.
    #[must_use]
    pub fn of(digest: &Digest) -> Self {
        fn walk(digest: &Digest, node: &Node<Label>, census: &mut Census) -> usize {
            if node.is_leaf() {
                return 0;
            }
            let level = 1 + node
                .children()
                .iter()
                .map(|child| walk(digest, child, census))
                .max()
                .unwrap_or(0);
            let state = match digest.standing(node.name()) {
                Some(Standing::Current) => State::Current,
                Some(Standing::Stale) => State::Stale,
                None => State::Unsummarized,
            };
            *census
                .counts
                .entry((Group::of(node.measure()), level, state))
                .or_default() += 1;
            level
        }
        let mut census = Census::default();
        if let Some(tree) = digest.tree() {
            walk(digest, tree.root(), &mut census);
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
                "{} current, {} stale, {} unsummarized",
                of(State::Current),
                of(State::Stale),
                of(State::Unsummarized)
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
        Ok(())
    }
}
