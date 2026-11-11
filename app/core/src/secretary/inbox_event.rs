//! The inbox's schema: a proposal asked, and each response to it.
//!
//! An asking writes the content register; a response writes the state register, whose
//! [`Phase`](crate::secretary::phase::Phase)s are inflationary, so the store refuses a response
//! below or beside the reading it supersedes. Its valuation is a [`Price`] alone.

use prodrome_core::event::Actor;
use prodrome_core::fold::Product as _;
use prodrome_core::fpl::FplError;
use prodrome_core::literal::{Datetime, ProdromeError, Table, Value, Vocabulary};
use prodrome_core::schema::{Price, Schema};
use prodrome_core::term::Term;

use crate::secretary::inbox::{Asked, ProposalId};
use crate::secretary::inbox_registers::{InboxRegister, InboxRegisters};
use crate::secretary::responded::{Done, Responded, Tried, Withdrawal};
use prodrome_core::literal::codec::{Constructor, Fields, Literal as _, headed};

/// One event about one proposal: who, when, and what was said of it.
#[derive(Debug, Clone, PartialEq)]
pub struct InboxEvent {
    /// The proposal.
    pub proposal: ProposalId,
    /// When its writer recorded it: data, never an order.
    pub at: Datetime,
    /// Who.
    pub actor: Actor,
    /// What.
    pub said: Said,
}

/// What an event says: the proposal itself, or a response to it.
#[derive(Debug, Clone, PartialEq)]
pub enum Said {
    /// The proposal.
    Asked(Box<Asked>),
    /// What became of it.
    Responded(Responded),
}

/// The constructors and their declared fields, in print order.
pub const SIGNATURES: &[Constructor] = &[
    (
        "Asked",
        &[
            "proposal", "at", "actor", "act", "why", "sources", "asked", "serves", "spec",
        ],
    ),
    (
        "Noticed",
        &[
            "proposal", "at", "actor", "act", "why", "sources", "asked", "serves", "spec",
        ],
    ),
    (
        "Triaged",
        &[
            "proposal", "at", "actor", "act", "why", "sources", "asked", "serves", "spec",
        ],
    ),
    (
        "Offered",
        &[
            "proposal", "at", "actor", "plugin", "act", "why", "sources", "asked", "serves", "spec",
        ],
    ),
    ("NotSent", &["proposal", "at", "actor", "attempt", "reason"]),
    ("Lacks", &["proposal", "at", "actor", "attempt", "lacking"]),
    ("Refused", &["proposal", "at", "actor", "attempt", "reason"]),
    (
        "Unfinished",
        &["proposal", "at", "actor", "attempt", "reason", "link"],
    ),
    (
        "Started",
        &["proposal", "at", "actor", "attempt", "began", "expires_at"],
    ),
    (
        "Sent",
        &["proposal", "at", "actor", "recipients", "message_id"],
    ),
    ("Applied", &["proposal", "at", "actor", "objects", "chain"]),
    ("Acted", &["proposal", "at", "actor", "what"]),
    ("MayHaveGone", &["proposal", "at", "actor", "reason"]),
    ("Checked", &["proposal", "at", "actor"]),
    ("Dismissed", &["proposal", "at", "actor", "reason"]),
    ("Superseded", &["proposal", "at", "actor"]),
    ("NoLongerNeeded", &["proposal", "at", "actor", "why"]),
    ("DoneElsewhere", &["proposal", "at", "actor", "what"]),
];

/// The inbox's whitelist: its constructors, an act's, then §7's terms for a spec.
#[derive(Debug, Clone, Default)]
pub struct InboxVocabulary;

impl Vocabulary for InboxVocabulary {
    fn signature(&self, name: &str) -> Option<prodrome_core::literal::Signature<'_>> {
        Table(SIGNATURES)
            .find(name)
            .or_else(|| Table(crate::secretary::act_vocabulary::SIGNATURES).find(name))
            .or_else(|| prodrome_core::term::schema::signatures().signature(name))
    }
}

impl InboxEvent {
    fn body(&self) -> (Constructor, Vec<Value>) {
        match &self.said {
            Said::Asked(asked) => asked.body(),
            Said::Responded(responded) => responded.body(),
        }
    }

    fn said_of(fields: Fields<'_>) -> Result<Said, ProdromeError> {
        let attempt = || -> Result<u32, ProdromeError> {
            Some(fields.get::<u32>("attempt")?)
                .filter(|n| *n >= 1)
                .ok_or_else(|| {
                    ProdromeError::invalid(format!("{}.attempt must be 1 or more", fields.name()))
                })
        };
        let done = |done| Ok(Said::Responded(Responded::Done(done)));
        match fields.name() {
            "Asked" | "Triaged" | "Offered" | "Noticed" => {
                Ok(Said::Asked(Box::new(Asked::of_fields(fields)?)))
            }
            "NotSent" => Ok(Said::Responded(Responded::Tried {
                attempt: attempt()?,
                tried: Tried::NotSent {
                    reason: fields.get("reason")?,
                },
            })),
            "Refused" => Ok(Said::Responded(Responded::Tried {
                attempt: attempt()?,
                tried: Tried::Refused {
                    reason: fields.get("reason")?,
                },
            })),
            "Unfinished" => Ok(Said::Responded(Responded::Tried {
                attempt: attempt()?,
                tried: Tried::Unfinished {
                    reason: fields.get("reason")?,
                    link: fields.get("link")?,
                },
            })),
            "Lacks" => Ok(Said::Responded(Responded::Tried {
                attempt: attempt()?,
                tried: Tried::Lacks(fields.get("lacking")?),
            })),
            "Started" => Ok(Said::Responded(Responded::Tried {
                attempt: attempt()?,
                tried: Tried::Running {
                    began: fields.get("began")?,
                    expires_at: fields.get("expires_at")?,
                },
            })),
            "Sent" => done(Done::Sent {
                recipients: fields.get("recipients")?,
                message_id: fields.get("message_id")?,
            }),
            "Applied" => done(Done::Applied {
                objects: fields.get("objects")?,
                chain: fields.get("chain")?,
            }),
            "Acted" => done(Done::Acted {
                what: fields.get("what")?,
            }),
            "MayHaveGone" => done(Done::MayHaveGone {
                reason: fields.get("reason")?,
            }),
            "Checked" => Ok(Said::Responded(Responded::Checked(()))),
            "Dismissed" => Ok(Said::Responded(Responded::Dismissed {
                reason: fields.get("reason")?,
            })),
            "Superseded" => Ok(Said::Responded(Responded::Withdrawn(
                Withdrawal::Superseded,
            ))),
            "DoneElsewhere" => Ok(Said::Responded(Responded::Withdrawn(
                Withdrawal::DoneElsewhere {
                    what: fields.get("what")?,
                },
            ))),
            "NoLongerNeeded" => Ok(Said::Responded(Responded::Withdrawn(
                Withdrawal::NoLongerNeeded {
                    why: fields.get("why")?,
                },
            ))),
            other => Err(prodrome_core::literal::codec::no_arm(
                "an inbox event",
                other,
            )),
        }
    }
}

impl Schema for InboxEvent {
    type Vocabulary = InboxVocabulary;
    type Key = ProposalId;
    type Register = InboxRegister;

    fn registers(_: &InboxVocabulary) -> Vec<InboxRegister> {
        vec![InboxRegister::Content, InboxRegister::State]
    }

    type Registers<'a> = InboxRegisters<'a>;

    fn to_value(&self) -> Value {
        let (body, values) = self.body();
        headed(
            vec![
                ("proposal", self.proposal.to_value()),
                ("at", self.at.to_value()),
                ("actor", self.actor.to_value()),
            ],
            body,
            values,
        )
    }

    fn from_value(_: &InboxVocabulary, value: &Value) -> Result<Self, ProdromeError> {
        let fields = Fields::among(value, SIGNATURES)?;
        Ok(InboxEvent {
            proposal: fields.get("proposal")?,
            at: fields.get("at")?,
            actor: fields.get("actor")?,
            said: InboxEvent::said_of(fields)?,
        })
    }

    fn key(&self) -> &ProposalId {
        &self.proposal
    }

    fn at(&self) -> Datetime {
        self.at
    }

    fn actor(&self) -> &Actor {
        &self.actor
    }

    fn writes(&self) -> impl Iterator<Item = InboxRegister> {
        std::iter::once(match self.said {
            Said::Asked(_) => InboxRegister::Content,
            Said::Responded(_) => InboxRegister::State,
        })
    }
}

/// One term per content candidate, so askings that disagree price as the more urgent (`Least`).
impl Price for InboxEvent {
    fn terms(registers: &InboxRegisters<'_>) -> Result<Vec<Term>, FplError> {
        Ok(registers
            .reading(InboxRegister::Content)
            .into_iter()
            .filter_map(|stamp| match &stamp.event.said {
                Said::Asked(asked) => Some(asked.price.term()),
                Said::Responded(_) => None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        suzatary_base::own::assert_own(SIGNATURES);
    }

    /// Every constructor the whitelist admits is the head and one body, so a print and its
    /// vocabulary are one spelling.
    #[test]
    fn every_signature_is_the_head_and_a_body() {
        use crate::secretary::inbox::{ASKED, NOTICED, OFFERED, TRIAGED};
        use crate::secretary::responded::{
            ACTED, APPLIED, CHECKED, DISMISSED, DONE_ELSEWHERE, LACKS, MAY_HAVE_GONE,
            NO_LONGER_NEEDED, NOT_SENT, REFUSED, SENT, STARTED, SUPERSEDED, UNFINISHED,
        };
        let bodies = [
            ASKED,
            NOTICED,
            TRIAGED,
            OFFERED,
            NOT_SENT,
            LACKS,
            REFUSED,
            UNFINISHED,
            STARTED,
            SENT,
            APPLIED,
            ACTED,
            MAY_HAVE_GONE,
            CHECKED,
            DISMISSED,
            SUPERSEDED,
            NO_LONGER_NEEDED,
            DONE_ELSEWHERE,
        ];
        assert_eq!(SIGNATURES.len(), bodies.len());
        for ((name, fields), (body, own)) in SIGNATURES.iter().zip(bodies) {
            assert_eq!(*name, body);
            let headed: Vec<&str> = ["proposal", "at", "actor"]
                .into_iter()
                .chain(own.iter().copied())
                .collect();
            assert_eq!(fields.to_vec(), headed, "{name}");
        }
    }

    /// An attempt prints as itself and reads back at the widest it holds, and none is below one.
    #[test]
    fn an_attempt_round_trips_at_its_widest() {
        use prodrome_core::event::{canonical, parse_event};
        let event = |attempt| InboxEvent {
            proposal: ProposalId::new("p0").expect("an id"),
            at: Datetime::new(2026, 10, 12, 9, 0, 0, 0).expect("an instant"),
            actor: Actor::new("ada").expect("an actor"),
            said: Said::Responded(Responded::Tried {
                attempt,
                tried: Tried::NotSent {
                    reason: "refused".to_owned(),
                },
            }),
        };
        let widest = event(u32::MAX);
        assert_eq!(
            parse_event(&InboxVocabulary, &canonical(&widest)),
            Ok(widest)
        );
        assert!(parse_event::<InboxEvent>(&InboxVocabulary, &canonical(&event(0))).is_err());
    }
}
