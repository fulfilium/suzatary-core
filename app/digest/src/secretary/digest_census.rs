//! The digest's summaries counted, for the operator: counts and names only, never a word.

use std::collections::BTreeMap;

use prodrome_core::digest::Node;
use prodrome_core::event::Hash;

use crate::secretary::compaction::{Compaction, InView, Reach, Rung, short};
use crate::secretary::digest::{Digest, Standing};
use crate::secretary::digest_label::Label;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Owners,
    /// Clean, and more than the owner's words.
    Clean,
    /// Somebody else's words may be in it.
    Tainted,
}

impl Group {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Current,
    /// A summary owed again, a summary written over a retracted one among them.
    Stale,
    /// No summary whose words read.
    Unsummarized,
    /// No summary, and every model of the ladder found no line for it.
    Unfit,
}

/// Every inner node, counted by group, level (a node over lines alone is level 1) and state, and
/// those under the view a reader is shown by state, the part the compactor asks again first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Census {
    counts: BTreeMap<(Group, usize, State), usize>,
    in_view: BTreeMap<State, usize>,
    unfit: Vec<Hash>,
    /// Summaries of nodes the tree no longer holds, counted in no other line.
    orphaned: usize,
}

impl Census {
    #[must_use]
    pub fn of<R: Rung>(digest: &Digest, rule: &Compaction<R>) -> Self {
        fn walk(
            (digest, unfit, view): (&Digest, &[Hash], &InView),
            node: &Node<Label>,
            census: &mut Census,
        ) -> usize {
            if node.is_leaf() {
                return 0;
            }
            let level = 1 + node
                .children()
                .iter()
                .map(|child| walk((digest, unfit, view), child, census))
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
            match view.reach(&node.span()) {
                Reach::InView => *census.in_view.entry(state).or_default() += 1,
                Reach::Elsewhere => {}
            }
            level
        }
        let mut census = Census {
            unfit: rule.unfit(digest),
            orphaned: digest.orphaned(),
            ..Census::default()
        };
        if let Some(tree) = digest.tree() {
            let unfit = census.unfit.clone();
            walk(
                (digest, &unfit, &InView::of(digest)),
                tree.root(),
                &mut census,
            );
        }
        census
    }

    #[must_use]
    pub fn count(&self, keep: impl Fn(Group, usize, State) -> bool) -> usize {
        self.counts
            .iter()
            .filter(|((group, level, state), _)| keep(*group, *level, *state))
            .map(|(_, count)| count)
            .sum()
    }
}

/// A total line, the view's, the orphaned summaries', then a line per group and level.
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
        let in_view = |state| self.in_view.get(&state).copied().unwrap_or(0);
        writeln!(
            f,
            "under the view: {} current, {} stale, {} unsummarized, {} unfit",
            in_view(State::Current),
            in_view(State::Stale),
            in_view(State::Unsummarized),
            in_view(State::Unfit)
        )?;
        writeln!(
            f,
            "summaries of spans no longer in the tree: {}",
            self.orphaned
        )?;
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
