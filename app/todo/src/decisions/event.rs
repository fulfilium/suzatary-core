use std::collections::BTreeSet;

use prodrome_data::event::Actor;
use prodrome_data::literal::{Datetime, ProdromeError, Value};

use super::registers::{DecisionRegister, DecisionRegisters};
use super::{Area, DecisionId, NotADecision, Statement};
use prodrome_data::literal::codec::{Constructor, Fields, Literal};
use prodrome_data::schema::headed::{Body, Constructors, Event};
use suzatary_record::record::MarkupSource;

/// One event about one decision: which, when its writer recorded it (data, never an order), who,
/// and what was said of it.
pub type DecisionEvent = Event<Said>;

/// What an event says of its decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// It was decided: its statement, and the decisions it is the newer word on.
    Decided {
        statement: Statement,
        supersedes: BTreeSet<DecisionId>,
    },
    /// It was restated, superseding its earlier statement; its links join the earlier ones.
    Revised {
        statement: Statement,
        /// What it supersedes, beside what it already did.
        supersedes: BTreeSet<DecisionId>,
    },
    /// It no longer holds, and nothing replaces it.
    Withdrawn { reason: String },
    /// It governs these parts of the tree, beside those it already governed: no statement is
    /// restated, so no summary of the decisions goes stale.
    Governs { areas: BTreeSet<Area> },
    /// The digest's node that `decision` names ([`DecisionId::of_node`]) is summed up in one line.
    Summarized { words: String },
}

/// Decision `decision` decided by `actor`.
pub fn decided(
    decision: DecisionId,
    at: Datetime,
    actor: Actor,
    statement: Statement,
    supersedes: BTreeSet<DecisionId>,
) -> Result<DecisionEvent, NotADecision> {
    Event::new(
        decision,
        at,
        actor,
        Said::Decided {
            statement,
            supersedes,
        },
    )
}

/// Decision `decision` restated by `actor`.
pub fn revised(
    decision: DecisionId,
    at: Datetime,
    actor: Actor,
    statement: Statement,
    supersedes: BTreeSet<DecisionId>,
) -> Result<DecisionEvent, NotADecision> {
    Event::new(
        decision,
        at,
        actor,
        Said::Revised {
            statement,
            supersedes,
        },
    )
}

/// Decision `decision` withdrawn by `actor`, for `reason`, which must be trimmed and not empty.
pub fn withdrawn(
    decision: DecisionId,
    at: Datetime,
    actor: Actor,
    reason: &str,
) -> Result<DecisionEvent, NotADecision> {
    let reason = reason.to_owned();
    Event::new(decision, at, actor, Said::Withdrawn { reason })
}

/// Decision `decision` governs `areas`, beside those it already governed.
pub fn governs(
    decision: DecisionId,
    at: Datetime,
    actor: Actor,
    areas: BTreeSet<Area>,
) -> Result<DecisionEvent, NotADecision> {
    Event::new(decision, at, actor, Said::Governs { areas })
}

/// The node `node` of the digest of the decisions in force, summed up as `words`, which must be one
/// trimmed line.
pub fn summarized(
    node: &prodrome_data::event::Hash,
    at: Datetime,
    actor: Actor,
    words: &str,
) -> Result<DecisionEvent, NotADecision> {
    let words = words.to_owned();
    Event::new(
        DecisionId::of_node(node),
        at,
        actor,
        Said::Summarized { words },
    )
}

impl Said {
    /// The statement a `Decided` or `Revised` makes.
    #[must_use]
    pub fn statement(&self) -> Option<&Statement> {
        match self {
            Said::Decided { statement, .. } | Said::Revised { statement, .. } => Some(statement),
            Said::Withdrawn { .. } | Said::Governs { .. } | Said::Summarized { .. } => None,
        }
    }

    /// The links a `Decided` or `Revised` makes; none for a withdrawal.
    #[must_use]
    pub fn supersedes(&self) -> Option<&BTreeSet<DecisionId>> {
        match self {
            Said::Decided { supersedes, .. } | Said::Revised { supersedes, .. } => Some(supersedes),
            Said::Withdrawn { .. } | Said::Governs { .. } | Said::Summarized { .. } => None,
        }
    }
}

/// `Decided(decision, at, actor, title, body, rationale, supersedes)`.
const DECIDED: Constructor = (
    "Decided",
    &[
        "decision",
        "at",
        "actor",
        "title",
        "body",
        "rationale",
        "supersedes",
    ],
);
/// `Revised(decision, at, actor, title, body, rationale, supersedes)`.
const REVISED: Constructor = (
    "Revised",
    &[
        "decision",
        "at",
        "actor",
        "title",
        "body",
        "rationale",
        "supersedes",
    ],
);
/// `Withdrawn(decision, at, actor, reason)`.
const WITHDRAWN: Constructor = ("Withdrawn", &["decision", "at", "actor", "reason"]);
/// `Governs(decision, at, actor, areas)`.
const GOVERNS: Constructor = ("Governs", &["decision", "at", "actor", "areas"]);
/// `Summarized(decision, at, actor, words)`.
const SUMMARIZED: Constructor = ("Summarized", &["decision", "at", "actor", "words"]);
/// The constructors, in print order.
const SIGNATURES: &[Constructor] = &[DECIDED, REVISED, WITHDRAWN, GOVERNS, SUMMARIZED];

/// The decisions' whitelist: five constructors and nothing else, since no event nests a term.
pub type DecisionVocabulary = Constructors<Said>;

fn invalid(failed: impl std::fmt::Display) -> ProdromeError {
    ProdromeError::invalid(failed.to_string())
}

/// A statement's fields and its links, as `Decided` and `Revised` print them.
fn stated(statement: &Statement, supersedes: &BTreeSet<DecisionId>) -> Vec<Value> {
    vec![
        statement.title().to_owned().to_value(),
        statement.body().as_str().to_owned().to_value(),
        statement.rationale().to_owned().to_value(),
        supersedes.to_value(),
    ]
}

// By hand: an event body of `schema::headed`, which the derive does not write.
impl Body for Said {
    const NAME: &'static str = "Said";
    type Key = DecisionId;
    type Register = DecisionRegister;
    type Registers<'a> = DecisionRegisters<'a>;
    type Refusal = NotADecision;

    const EVENTS: &'static [Constructor] = SIGNATURES;

    /// A statement supersedes in both registers, so a restatement of a withdrawn decision is
    /// refused by the life's order; a withdrawal only in the life. The links and the areas
    /// accumulate and supersede nothing; a summary supersedes in the summary register alone.
    fn writes(&self) -> impl Iterator<Item = DecisionRegister> {
        let registers: &[DecisionRegister] = match self {
            Said::Decided { .. } | Said::Revised { .. } => {
                &[DecisionRegister::Content, DecisionRegister::Life]
            }
            Said::Withdrawn { .. } => &[DecisionRegister::Life],
            Said::Governs { .. } => &[],
            Said::Summarized { .. } => &[DecisionRegister::Summary],
        };
        registers.iter().copied()
    }

    /// A decision never supersedes itself, a withdrawal's reason is trimmed and not empty, a
    /// `Governs` names an area, and a summary is one trimmed line.
    fn admits(decision: &DecisionId, said: &Said) -> Result<(), NotADecision> {
        match said {
            Said::Decided { supersedes, .. } | Said::Revised { supersedes, .. }
                if supersedes.contains(decision) =>
            {
                Err(NotADecision::Itself(decision.to_string()))
            }
            Said::Withdrawn { reason } if reason.is_empty() || *reason != reason.trim() => {
                Err(NotADecision::Empty("reason"))
            }
            Said::Governs { areas } if areas.is_empty() => Err(NotADecision::Empty("areas")),
            Said::Summarized { words }
                if words.is_empty()
                    || *words != words.trim()
                    || words.chars().any(char::is_control) =>
            {
                Err(NotADecision::Summary)
            }
            Said::Decided { .. }
            | Said::Revised { .. }
            | Said::Withdrawn { .. }
            | Said::Governs { .. }
            | Said::Summarized { .. } => Ok(()),
        }
    }

    fn fields(&self) -> (Constructor, Vec<Value>) {
        match self {
            Said::Decided {
                statement,
                supersedes,
            } => (DECIDED, stated(statement, supersedes)),
            Said::Revised {
                statement,
                supersedes,
            } => (REVISED, stated(statement, supersedes)),
            Said::Withdrawn { reason } => (WITHDRAWN, vec![reason.to_value()]),
            Said::Governs { areas } => (GOVERNS, vec![areas.to_value()]),
            Said::Summarized { words } => (SUMMARIZED, vec![words.to_value()]),
        }
    }

    fn parse(fields: &Fields<'_>) -> Result<DecisionEvent, ProdromeError> {
        let (decision, at, actor) = (
            fields.get("decision")?,
            fields.get("at")?,
            fields.get("actor")?,
        );
        let statement = || -> Result<Statement, ProdromeError> {
            Statement::new(
                &fields.get::<String>("title")?,
                MarkupSource::from_chain(fields.get::<String>("body")?),
                &fields.get::<String>("rationale")?,
            )
            .map_err(invalid)
        };
        match fields.name() {
            "Decided" => decided(decision, at, actor, statement()?, fields.get("supersedes")?),
            "Revised" => revised(decision, at, actor, statement()?, fields.get("supersedes")?),
            "Withdrawn" => withdrawn(decision, at, actor, &fields.get::<String>("reason")?),
            "Governs" => governs(decision, at, actor, fields.get("areas")?),
            "Summarized" => {
                let words = fields.get::<String>("words")?;
                Event::new(decision, at, actor, Said::Summarized { words })
            }
            other => {
                return Err(prodrome_data::literal::codec::no_arm(Self::NAME, other));
            }
        }
        .map_err(invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        suzatary_base::own::assert_own(SIGNATURES);
    }

    /// A set of links has one spelling: sorted, each once; any other print is not this event's.
    #[test]
    fn links_read_back_only_sorted_and_once() {
        use prodrome_data::event::{canonical, parse_event};
        let at = Datetime::new(2026, 10, 3, 9, 0, 0, 0).expect("an instant");
        let statement =
            Statement::new("Plugins", MarkupSource::escaped("body"), "why").expect("a statement");
        let id = |text: &str| DecisionId::new(text).expect("an id");
        let event = decided(
            id("plugins"),
            at,
            Actor::new("ada").expect("an actor"),
            statement,
            BTreeSet::from([id("aaa"), id("bbb")]),
        )
        .expect("an event");
        let print = canonical(&event);
        assert_eq!(
            parse_event(&DecisionVocabulary::default(), &print),
            Ok(event)
        );
        for other in ["('bbb', 'aaa')", "('aaa', 'aaa', 'bbb')"] {
            let moved = print.replace("('aaa', 'bbb')", other);
            assert_ne!(moved, print);
            assert!(parse_event::<DecisionEvent>(&DecisionVocabulary::default(), &moved).is_err());
        }
    }

    #[test]
    fn a_decision_cannot_supersede_itself() {
        let at = Datetime::new(2026, 10, 3, 9, 0, 0, 0).expect("an instant");
        let id = DecisionId::new("plugins").expect("an id");
        let statement =
            Statement::new("Plugins", MarkupSource::escaped("body"), "why").expect("a statement");
        let refused = decided(
            id.clone(),
            at,
            Actor::new("ada").expect("an actor"),
            statement,
            BTreeSet::from([id]),
        );
        assert!(matches!(refused, Err(NotADecision::Itself(_))));
    }
}
