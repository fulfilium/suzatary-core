//! Writing the summaries of the decision log with the secretary's compactor. Only a verb the main
//! session or the box runs asks the model, never the brief.

use std::collections::BTreeSet;
use suzatary_base::call_log::Journal;

use prodrome_data::event::{Actor, Hash};
use prodrome_data::literal::Datetime;
use suzatary_base::refusal::Refusal;
use suzatary_todo::decisions::Decisions;

use super::decision_log::DecisionLog;
use crate::secretary::compact::Compact;
use crate::secretary::compact_ask::{Ask, shown_log};
use crate::secretary::compact_logs::Logs;
use crate::secretary::compactor::{self, Outcome};

/// What asking about one node came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Wrote {
        node: Hash,
        words: String,
    },
    /// No answer was a line that fits, so the node stays without a summary.
    Unfit {
        node: Hash,
    },
}

/// The nodes of the log of the decisions in force that wait for a summary, asked about in turn,
/// the smallest first, each `step` shown to `on`. A node whose answers did not fit is not asked
/// again in this run.
///
/// # Errors
/// [`Refusal`] from the store, or from a model that could not answer.
pub fn summarize(
    decisions: &Decisions<'_>,
    compactor: &dyn Compact,
    journal: &dyn Journal,
    at: Datetime,
    actor: &Actor,
    mut on: impl FnMut(&Step),
) -> Result<usize, Refusal> {
    let mut unfit = BTreeSet::new();
    let mut wrote = 0;
    let mut logs = Logs::<()>::default();
    loop {
        let log = DecisionLog::of(&decisions.ledger()?, &decisions.summaries()?)
            .map_err(|cyclic| Refusal::internal(cyclic.to_string()))?;
        let waiting: Vec<Hash> = log
            .pending()
            .into_iter()
            .filter(|node| !unfit.contains(node))
            .collect();
        let Some((node, rest)) = waiting.split_first() else {
            return Ok(wrote);
        };
        let node = node.clone();
        let Some(span) = log.span(&node) else {
            return Err(Refusal::internal(format!(
                "node {node} is pending and its children do not read"
            )));
        };
        let (shown, sharing) = logs.of((), rest.len(), || shown_log(&log, |()| true));
        let ask = Ask::of_decisions(shown, span).shared(sharing);
        let step = match compactor::compact(compactor, journal, ask, &mut |_| Ok(()))? {
            Outcome::Fits { line } => {
                decisions.summarize(&node, line.as_str(), at, actor)?;
                wrote += 1;
                Step::Wrote {
                    node,
                    words: line.as_str().to_owned(),
                }
            }
            Outcome::Unfit { .. } => {
                unfit.insert(node.clone());
                Step::Unfit { node }
            }
        };
        on(&step);
    }
}
