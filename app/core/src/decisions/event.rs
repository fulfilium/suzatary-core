use std::collections::BTreeSet;

use prodrome_core::event::Actor;
use prodrome_core::literal::{Datetime, ProdromeError, Value};

use super::registers::{DecisionRegister, DecisionRegisters};
use super::{DecisionId, NotADecision, Statement};
use crate::record::MarkupSource;
use prodrome_core::literal::codec::{Constructor, Fields, Literal};
use prodrome_core::schema::headed::{Body, Constructors, Event};

/// One event about one decision: which, when its writer recorded it (data, never an order), who,
/// and what was said of it.
pub type DecisionEvent = Event<Said>;

/// What an event says of its decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Said {
    /// It was decided: its statement, and the decisions it is the newer word on.
    Decided {
        /// What it says.
        statement: Statement,
        /// What it supersedes.
        supersedes: BTreeSet<DecisionId>,
    },
    /// It was restated, superseding its earlier statement; its links join the earlier ones.
    Revised {
        /// What it says now.
        statement: Statement,
        /// What it supersedes, beside what it already did.
        supersedes: BTreeSet<DecisionId>,
    },
    /// It no longer holds, and nothing replaces it.
    Withdrawn {
        /// Why.
        reason: String,
    },
}

/// Decision `decision` decided by `actor`.
///
/// # Errors
/// [`NotADecision::Itself`] where it names itself among what it supersedes.
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
///
/// # Errors
/// [`NotADecision::Itself`] where it names itself among what it supersedes.
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

/// Decision `decision` withdrawn by `actor`, for `reason`.
///
/// # Errors
/// [`NotADecision::Empty`] for an empty or untrimmed reason.
pub fn withdrawn(
    decision: DecisionId,
    at: Datetime,
    actor: Actor,
    reason: &str,
) -> Result<DecisionEvent, NotADecision> {
    let reason = reason.to_owned();
    Event::new(decision, at, actor, Said::Withdrawn { reason })
}

impl Said {
    /// The statement a `Decided` or `Revised` makes.
    #[must_use]
    pub fn statement(&self) -> Option<&Statement> {
        match self {
            Said::Decided { statement, .. } | Said::Revised { statement, .. } => Some(statement),
            Said::Withdrawn { .. } => None,
        }
    }

    /// The links a `Decided` or `Revised` makes; none for a withdrawal.
    #[must_use]
    pub fn supersedes(&self) -> Option<&BTreeSet<DecisionId>> {
        match self {
            Said::Decided { supersedes, .. } | Said::Revised { supersedes, .. } => Some(supersedes),
            Said::Withdrawn { .. } => None,
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
/// The constructors, in print order.
const SIGNATURES: &[Constructor] = &[DECIDED, REVISED, WITHDRAWN];

/// The decisions' whitelist: three constructors and nothing else, since no event nests a term.
pub type DecisionVocabulary = Constructors<Said>;

/// A decision's literal is its id.
impl Literal for DecisionId {
    fn to_value(&self) -> Value {
        Value::str(self.as_str())
    }

    fn from_value(value: &Value) -> Result<Self, ProdromeError> {
        DecisionId::new(String::from_value(value)?).map_err(invalid)
    }
}

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

impl Body for Said {
    type Key = DecisionId;
    type Register = DecisionRegister;
    type Registers<'a> = DecisionRegisters<'a>;
    type Refusal = NotADecision;

    const EVENTS: &'static [Constructor] = SIGNATURES;

    /// A statement supersedes in both registers, so a restatement of a withdrawn decision is
    /// refused by the life's order; a withdrawal only in the life. The links accumulate and
    /// supersede nothing.
    fn writes(&self) -> impl Iterator<Item = DecisionRegister> {
        let statement = matches!(self, Said::Decided { .. } | Said::Revised { .. })
            .then_some(DecisionRegister::Content);
        statement.into_iter().chain([DecisionRegister::Life])
    }

    /// A decision never supersedes itself, and a withdrawal's reason is trimmed and not empty.
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
            Said::Decided { .. } | Said::Revised { .. } | Said::Withdrawn { .. } => Ok(()),
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
            other => {
                return Err(prodrome_core::literal::codec::no_arm(
                    "a decision event",
                    other,
                ));
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
        crate::own::assert_own(SIGNATURES);
    }

    /// A set of links has one spelling: sorted, each once; any other print is not this event's.
    #[test]
    fn links_read_back_only_sorted_and_once() {
        use prodrome_core::event::{canonical, parse_event};
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
