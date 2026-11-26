//! The decisions in force as a log, one leaf each, the oldest decided first.
//!
//! A leaf is named by the objects that wrote its statement, so a restated decision is a new leaf and
//! every summary over it is dead.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use prodrome_data::event::Hash;
use prodrome_digest::{Cycle, Leaf, Node, Tree, View, Waters};
use suzatary_memory::secretary::salted_text::Words as _;
use suzatary_todo::decisions::{Area, Decision, DecisionId, Ledger, Summaries};

use crate::log::Log;
use crate::secretary::digest_line::Line;
use crate::secretary::indexed_tree::IndexedTree;

/// Two decisions in force rest on each other, which the objects that wrote them do not allow.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the decisions in force stand in a cycle at {0}")]
pub struct Cyclic(Hash);

impl From<Cycle> for Cyclic {
    fn from(Cycle(at): Cycle) -> Self {
        Self(at)
    }
}

/// One leaf as a view and a zoom show it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
    /// `id: title`.
    line: String,
    whole: String,
}

#[derive(Debug, Clone, Default)]
pub struct DecisionLog {
    tree: Option<IndexedTree<()>>,
    leaves: BTreeMap<Hash, Placed>,
    summaries: BTreeMap<Hash, Line>,
}

/// A decision in force whole: its id and title, when it was decided and what it governs, its body
/// and why.
#[must_use]
pub(crate) fn full(id: &DecisionId, decision: &Decision) -> String {
    whole(id, decision, &standing(decision))
}

fn whole(id: &DecisionId, decision: &Decision, status: &str) -> String {
    let statement = decision.statement();
    format!(
        "{id}: {}\n{status}\n\n{}\n\nWhy: {}",
        statement.title(),
        statement.body().as_str(),
        statement.rationale()
    )
}

/// Where a decision in force stands: when it was decided, and what it governs.
fn standing(decision: &Decision) -> String {
    let mut said = format!("decided {}", decision.decided.isoformat());
    if !decision.areas.is_empty() {
        let areas: Vec<&str> = decision.areas.iter().map(Area::as_str).collect();
        let _ = write!(said, "; governs {}", areas.join(", "));
    }
    said
}

impl DecisionLog {
    /// The log of the decisions of `ledger` in force, under the summaries written for its nodes.
    ///
    /// # Errors
    /// [`Cyclic`] when the decisions rest on one another.
    pub fn of(ledger: &Ledger, summaries: &Summaries) -> Result<Self, Cyclic> {
        let mut standing_now: Vec<(&DecisionId, &Decision)> = ledger
            .iter()
            .filter(|(id, _)| ledger.in_force(id))
            .collect();
        standing_now.sort_by(|(one, first), (two, second)| {
            first
                .decided
                .cmp(&second.decided)
                .then_with(|| one.cmp(two))
        });
        let mut leaves = Vec::new();
        let mut placed = BTreeMap::new();
        let mut before: Option<Hash> = None;
        for (id, decision) in standing_now {
            let name = prodrome_memo::name(b"decision", decision.written());
            let line = format!("{id}: {}", decision.statement().title());
            leaves.push(Leaf {
                name: name.clone(),
                parents: before.replace(name.clone()).into_iter().collect(),
                bytes: u32::try_from(line.len()).unwrap_or(u32::MAX),
                measure: (),
            });
            placed.insert(
                name,
                Placed {
                    line,
                    whole: whole(id, decision, &standing(decision)),
                },
            );
        }
        let tree = prodrome_digest::tree(leaves)?.map(IndexedTree::of);
        let said = tree
            .as_ref()
            .map(|tree| inner_nodes(tree.tree()))
            .into_iter()
            .flatten()
            .filter_map(|node| {
                let line = Line::read(summaries.get(&node)?)?;
                Some((node, line))
            })
            .collect();
        Ok(Self {
            tree,
            leaves: placed,
            summaries: said,
        })
    }

    /// What `node`'s summary costs in a view, if it has one.
    fn summarized(&self, node: &Hash) -> Option<u32> {
        self.summary(node)
            .map(|line| u32::try_from(line.as_str().len()).unwrap_or(u32::MAX))
    }

    /// The nodes whose summary can be written now, the smallest and then the leftmost first
    /// ([`prodrome_digest::pending`]).
    #[must_use]
    pub(crate) fn pending(&self) -> Vec<Hash> {
        self.tree().map_or_else(Vec::new, |tree| {
            prodrome_digest::pending(tree, |node| self.summarized(node))
        })
    }

    /// The lines of the children of `node`, each a decision's line or a child's summary, which a
    /// summary of `node` is written from. `None` for a node the log does not hold or a child with
    /// no summary to show.
    #[must_use]
    pub(crate) fn span(&self, node: &Hash) -> Option<Vec<String>> {
        self.node(node)?
            .children()
            .iter()
            .map(|child| {
                if child.is_leaf() {
                    self.line(child.name())
                } else {
                    self.summary(child.name())
                        .map(|line| line.as_str().to_owned())
                }
            })
            .collect()
    }
}

/// Every inner node of `tree`, by name.
fn inner_nodes<M>(tree: &Tree<M>) -> Vec<Hash> {
    let mut found = Vec::new();
    let mut stack = vec![tree.root()];
    while let Some(node) = stack.pop() {
        if !node.is_leaf() {
            found.push(node.name().clone());
            stack.extend(node.children());
        }
    }
    found
}

impl Log for DecisionLog {
    type Label = ();

    fn tree(&self) -> Option<&Tree<()>> {
        self.tree.as_ref().map(IndexedTree::tree)
    }

    fn node(&self, name: &Hash) -> Option<&Node<()>> {
        self.tree.as_ref()?.node(name)
    }

    fn view(&self, budget: u64) -> Option<View> {
        self.tree().map(|tree| {
            prodrome_digest::view(
                tree,
                Node::bytes,
                |node| self.summarized(node),
                Waters::at(budget),
            )
        })
    }

    fn summary(&self, node: &Hash) -> Option<&Line> {
        self.summaries.get(node)
    }

    fn label(&self, _: &Node<()>) {}

    fn line(&self, leaf: &Hash) -> Option<String> {
        self.leaves.get(leaf).map(|placed| placed.line.clone())
    }

    fn whole(&self, leaf: &Hash) -> Option<String> {
        self.leaves.get(leaf).map(|placed| placed.whole.clone())
    }
}
