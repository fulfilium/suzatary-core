use prodrome_core::literal::Datetime;
use schemars::JsonSchema;
use serde::Serialize;

use super::{Decision, DecisionId, Ledger};

/// The decisions as their template (`rendering::DECISIONS`) draws them: a section per lineage,
/// newest first, each decision above what it supersedes, with where it stands.
///
/// Every title, rationale and reason is data, drawn as text; a body is markup the door admitted,
/// evaluated in the closed world as its parts were.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DecisionsPage {
    /// How many are recorded.
    pub recorded: usize,
    /// How many are in force.
    pub in_force: usize,
    /// The lineages, each newest first.
    pub lineages: Vec<Vec<Shown>>,
}

/// One decision as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Shown {
    /// Its id.
    pub id: String,
    /// What was decided, in one line.
    pub title: String,
    /// The day it was decided.
    pub decided: String,
    /// Where it stands.
    pub standing: Vec<Standing>,
    /// Whether two writers each superseded the other through it.
    pub cycle: bool,
    /// How many statements stand, where more than one does.
    pub conflict: Option<usize>,
    /// Its body: markup the door admitted.
    pub body: String,
    /// Why.
    pub rationale: String,
    /// What it supersedes.
    pub supersedes: Vec<String>,
}

/// Where a decision stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Standing {
    /// Withdrawn on a day, for a reason.
    Withdrawn {
        /// The day.
        day: String,
        /// Why.
        reason: String,
    },
    /// Superseded by these.
    Superseded {
        /// The decisions that supersede it.
        by: Vec<String>,
    },
}

/// The decisions as one page.
#[must_use]
pub fn document(ledger: &Ledger) -> DecisionsPage {
    let cycles = ledger.cycles();
    DecisionsPage {
        recorded: ledger.len(),
        in_force: ledger.iter().filter(|(id, _)| ledger.in_force(id)).count(),
        lineages: ledger
            .lineages()
            .into_iter()
            .map(|lineage| {
                lineage
                    .into_iter()
                    .filter_map(|id| {
                        ledger
                            .get(id)
                            .map(|decision| shown(ledger, id, decision, cycles.contains(id)))
                    })
                    .collect()
            })
            .collect(),
    }
}

fn shown(ledger: &Ledger, id: &DecisionId, decision: &Decision, cycle: bool) -> Shown {
    let statement = decision.statement();
    let by: Vec<String> = ledger
        .superseded_by(id)
        .into_iter()
        .map(ToString::to_string)
        .collect();
    Shown {
        id: id.to_string(),
        title: statement.title().to_owned(),
        decided: day(decision.decided),
        standing: decision
            .withdrawn
            .iter()
            .map(|withdrawn| Standing::Withdrawn {
                day: day(withdrawn.at),
                reason: withdrawn.reason.clone(),
            })
            .chain((!by.is_empty()).then_some(Standing::Superseded { by }))
            .collect(),
        cycle,
        conflict: decision.is_conflict().then(|| decision.statements().len()),
        body: statement.body().as_str().to_owned(),
        rationale: statement.rationale().to_owned(),
        supersedes: decision
            .supersedes
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

fn day(at: Datetime) -> String {
    format!("{:04}-{:02}-{:02}", at.year(), at.month(), at.day())
}
