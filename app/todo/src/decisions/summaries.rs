use std::collections::BTreeMap;

use prodrome_dag::registers::{Node, fold};
use prodrome_data::event::Hash;
use prodrome_data::policy::Everything;

use super::{DecisionEvent, DecisionId, DecisionRegisters, SummaryRegister};

/// The words written for each node of the digest of the decisions in force, read from the
/// history: a node with two concurrent summaries reads as the one whose write has the least name,
/// as a conflicting statement does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Summaries(BTreeMap<DecisionId, String>);

impl Summaries {
    #[must_use]
    pub fn of(nodes: &[Node<DecisionEvent>]) -> Self {
        let mut words = BTreeMap::new();
        for (key, stream) in fold(nodes).entities() {
            let registers: DecisionRegisters<'_> =
                prodrome_dag::fold::read(stream, None, &Everything);
            let first = registers
                .summary
                .reading::<SummaryRegister>()
                .into_iter()
                .filter_map(|stamp| match stamp.event.body() {
                    super::Said::Summarized { words } => Some((&stamp.name, words)),
                    super::Said::Decided { .. }
                    | super::Said::Revised { .. }
                    | super::Said::Withdrawn { .. }
                    | super::Said::Governs { .. } => None,
                })
                .min_by_key(|(name, _)| *name);
            if let Some((_, text)) = first {
                words.insert(key.clone(), text.clone());
            }
        }
        Self(words)
    }

    /// What `node` is summed up as, if anything was written for it.
    #[must_use]
    pub fn get(&self, node: &Hash) -> Option<&str> {
        self.0.get(&DecisionId::of_node(node)).map(String::as_str)
    }
}
