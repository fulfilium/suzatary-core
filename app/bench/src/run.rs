//! `suzatary-dev bench run`: replay a case through a candidate, without scoring.
//!
//! [`replay`] hands a candidate [`Model`] the manual and the case's input and dispatches every
//! call through the real agent surface until it stops asking; a real [`Model`] already builds its
//! prompt inside `turn`, so this loop builds none of its own.
//!
//! The candidate never sees [`crate::cases::Truths`]: `opening` is built from the case's input
//! alone.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use serde::{Deserialize, Serialize};
use suzatary_core::agent;
use suzatary_core::capability::{Agent, Conversation, Model, Reply, TokenUsage, Turn};
use suzatary_core::deployment::Deployment;
use suzatary_core::secretary::filed::TriageCards;

use crate::cases::CaseId;

/// The round bound: a candidate that keeps asking stops, and the last round records what it said.
pub const MAX_ROUNDS: u32 = 8;

/// One verb call the candidate made, dispatched, and what came back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallRecord {
    /// The verb's name as the candidate spelled it, recorded even when unknown.
    pub verb: String,
    /// Its arguments, as the candidate supplied them.
    pub args: BTreeMap<String, serde_json::Value>,
    /// What the real dispatch answered.
    pub outcome: CallOutcome,
}

/// The two kinds of "no" `suzatary_core::agent::call` draws, kept apart for the scorer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum CallOutcome {
    /// The verb ran; an `ok: false` body is carried in `value` unchanged.
    Answered {
        /// Whatever the verb answered.
        value: serde_json::Value,
    },
    /// The call was refused before a handler ran.
    Refused {
        /// The refusal's words, which teach the right shape.
        message: String,
    },
}

/// One round: what the candidate said, and every call it asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundRecord {
    /// What the candidate said to the owner this round. May be empty.
    pub say: String,
    /// The calls it asked for, in order, each with its outcome.
    pub calls: Vec<CallRecord>,
}

/// What a replay produced for one case, never the incumbent's answer nor a message body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    /// Which model answered, so a run directory mixing candidates says who wrote which file.
    pub model: String,
    /// Every round, in order.
    pub rounds: Vec<RoundRecord>,
    /// The objects produced, as the difference of every chain's and the inbox's objects before
    /// and after, since the verbs' answers differ in shape; empty means the candidate wrote
    /// nothing.
    pub objects: Vec<String>,
    /// Token usage summed across rounds, `None` when no round reported any rather than a zero.
    pub usage: Option<TokenUsage>,
}

/// One case's outcome: whether it was answered at all.
///
/// A closed sum, so a transport failure never reads as a candidate declining, and resume can ask
/// it again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "lowercase")]
pub enum Outcome {
    /// The candidate answered — every round it took, even an empty one.
    Answered(Answer),
    /// [`Model::turn`] failed before this case reached an [`Answer`].
    Failed {
        /// What [`suzatary_core::capability::ModelError`] said.
        error: String,
    },
}

/// One case's record as a run directory holds it, keyed by [`CaseId`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunEntry {
    /// Which case this answers.
    pub case: CaseId,
    /// When this replay ran, as an RFC 3339 stamp.
    pub recorded_at: String,
    /// What happened: an answer, or a failure that never reached one.
    pub outcome: Outcome,
}

/// Replays one case against `model`, dispatching every call through the real agent surface.
///
/// A call naming a chain missing from `chains` is refused; a model failure ends the case as
/// [`Outcome::Failed`].
#[must_use]
pub fn replay(
    model: &dyn Model,
    chains: &BTreeMap<String, Agent<'_>>,
    cards: &Filing<'_>,
    deployment: &Deployment,
    system: &str,
    opening: String,
) -> Outcome {
    let tools = agent::tools();
    let before = objects_of(chains, cards);

    let mut turns = vec![Turn::Person {
        text: opening,
        references: Vec::new(),
        attachments: Vec::new(),
    }];
    let mut rounds = Vec::new();
    let mut usages: Vec<Option<TokenUsage>> = Vec::new();
    let mut round_no = 0u32;
    loop {
        round_no += 1;
        let asking = Conversation {
            system: system.to_owned(),
            turns: turns.clone(),
        };
        let reply: Reply = match model.turn(&asking, &tools) {
            Ok(reply) => reply,
            Err(failed) => {
                return Outcome::Failed {
                    error: failed.to_string(),
                };
            }
        };
        usages.push(reply.usage);
        turns.push(Turn::Model {
            text: reply.say.clone(),
            calls: reply.calls.clone(),
        });
        let mut call_records = Vec::with_capacity(reply.calls.len());
        for call in &reply.calls {
            let args: serde_json::Map<String, serde_json::Value> =
                call.args.clone().into_iter().collect();
            let outcome = dispatch(chains, &cards.cards, deployment, call.verb.as_str(), &args);
            turns.push(Turn::Tool {
                verb: call.verb.clone(),
                answer: shown(&outcome),
                // The bench runs the agent's verbs, none of which remembers a fact.
                outcome: match outcome {
                    CallOutcome::Refused { .. } => {
                        suzatary_core::secretary::outcome::Outcome::Refused
                    }
                    CallOutcome::Answered { .. } => {
                        suzatary_core::secretary::outcome::Outcome::Answered
                    }
                },
                files: Vec::new(),
            });
            call_records.push(CallRecord {
                verb: call.verb.to_string(),
                args: call.args.clone(),
                outcome,
            });
        }
        let calls_asked = !reply.calls.is_empty();
        rounds.push(RoundRecord {
            say: reply.say,
            calls: call_records,
        });
        if !calls_asked || round_no >= MAX_ROUNDS {
            break;
        }
    }

    let after = objects_of(chains, cards);
    let objects = after
        .difference(&before)
        .map(Hash::as_str)
        .map(str::to_owned)
        .collect();
    Outcome::Answered(Answer {
        model: model.name(),
        rounds,
        objects,
        usage: sum_usage(usages.iter()),
    })
}

/// Sum what was reported across rounds, keeping "no round mentioned caching" apart from zero.
fn sum_usage<'a>(usages: impl Iterator<Item = &'a Option<TokenUsage>>) -> Option<TokenUsage> {
    let mut total = TokenUsage::default();
    let mut any = false;
    let mut any_cached = false;
    for usage in usages.filter_map(Option::as_ref) {
        any = true;
        total.prompt_tokens += usage.prompt_tokens;
        total.completion_tokens += usage.completion_tokens;
        if let Some(cached) = usage.cached_prompt_tokens {
            any_cached = true;
            total.cached_prompt_tokens = Some(total.cached_prompt_tokens.unwrap_or(0) + cached);
        }
    }
    any.then(|| TokenUsage {
        cached_prompt_tokens: any_cached.then_some(total.cached_prompt_tokens.unwrap_or(0)),
        ..total
    })
}

/// One call, dispatched to the real handler over the chain the candidate named.
fn dispatch(
    chains: &BTreeMap<String, Agent<'_>>,
    cards: &TriageCards<'_>,
    deployment: &Deployment,
    verb: &str,
    args: &serde_json::Map<String, serde_json::Value>,
) -> CallOutcome {
    match agent::call(chains, cards, deployment, verb, args) {
        Ok(value) => CallOutcome::Answered { value },
        Err(refused) => CallOutcome::Refused {
            message: refused.message().to_owned(),
        },
    }
}

/// A call's outcome as the transcript shows it, so a string does not reach the next round quoted.
fn shown(outcome: &CallOutcome) -> String {
    match outcome {
        CallOutcome::Answered {
            value: serde_json::Value::String(text),
        } => text.clone(),
        CallOutcome::Answered { value } => {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        }
        CallOutcome::Refused { message } => message.clone(),
    }
}

/// Every object every bundle holds; an unreadable chain answers none rather than panicking.
fn objects_of(chains: &BTreeMap<String, Agent<'_>>, filing: &Filing<'_>) -> BTreeSet<Hash> {
    chains
        .values()
        .flat_map(|caps| caps.chain.objects().unwrap_or_default())
        .map(|(hash, _)| hash)
        .chain(
            filing
                .inbox
                .objects()
                .unwrap_or_default()
                .into_iter()
                .map(|(hash, _)| hash),
        )
        .collect()
}

/// Where a candidate's writes go: the triage agent's door into a scratch inbox, and that inbox,
/// which the replay reads to say what the candidate filed.
#[derive(Clone, Copy)]
pub struct Filing<'a> {
    /// The door the verbs file through.
    pub cards: TriageCards<'a>,
    /// The inbox behind it.
    pub inbox: &'a dyn suzatary_core::store::Prodrome<suzatary_core::secretary::inbox_event::InboxEvent>,
}

#[cfg(test)]
#[allow(clippy::zero_sized_map_values)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use prodrome_core::event::Actor;
    use prodrome_core::policy::Untrusted;
    use suzatary_core::capability::{ModelError, Reply};
    use suzatary_core::chains::{ChainName, ChainPolicy, Chains};
    use suzatary_core::deployment::Deployment;

    use super::*;

    /// A deployment naming one chain, enough for [`agent::chain_of`] to resolve a `chain`
    /// argument.
    fn deployment(chain_name: &str) -> Deployment {
        let policy = ChainPolicy {
            name: ChainName::new(chain_name).expect("a valid chain name"),
            untrusted: Untrusted::none(),
            roster: BTreeSet::<Actor>::new(),
            root: suzatary_core::cast::keys::root(),
        };
        Deployment {
            viewers: BTreeSet::new(),
            actors: BTreeMap::new(),
            chains: Chains::of(vec![policy]).expect("one chain is a valid list"),
            categories: BTreeMap::new(),
            app_url: String::new(),
            coding: None,
            zone: suzatary_core::zone::Zone::UTC,
        }
    }

    /// A scratch inbox and conversations a replay files into.
    #[derive(Default)]
    struct Scratch {
        conversations: suzatary_core::secretary::conversations::InMemory,
        inbox: suzatary_core::store::InMemory<suzatary_core::secretary::inbox_event::InboxEvent>,
    }

    impl Scratch {
        fn filing(&self) -> Filing<'_> {
            Filing {
                cards: TriageCards::new(self.conversations.conversations(), &self.inbox),
                inbox: &self.inbox,
            }
        }
    }

    /// A `Model::turn` error ends the case as [`Outcome::Failed`], never as an [`Answer`].
    #[test]
    fn a_turn_error_is_failed_not_a_prose_answer() {
        struct Unreachable;
        impl Model for Unreachable {
            fn name(&self) -> String {
                "unreachable/test".to_owned()
            }
            fn turn(
                &self,
                _conversation: &Conversation,
                _tools: &serde_json::Value,
            ) -> Result<Reply, ModelError> {
                Err(ModelError::Failed("404 not found".to_owned()))
            }
        }
        let scratch = Scratch::default();
        let outcome = replay(
            &Unreachable,
            &BTreeMap::<String, Agent<'_>>::new(),
            &scratch.filing(),
            &deployment("life"),
            "system prompt",
            "a message needs triage".to_owned(),
        );
        match outcome {
            Outcome::Failed { error } => assert!(error.contains("404"), "{error}"),
            Outcome::Answered(answer) => {
                panic!("a turn error must not become an answer: {answer:?}")
            }
        }
    }

    /// The round bound stops an unbounded script.
    #[test]
    fn the_round_bound_stops_a_candidate_that_never_finishes() {
        struct Insistent;
        impl Model for Insistent {
            fn name(&self) -> String {
                "insistent/test".to_owned()
            }
            fn turn(
                &self,
                _conversation: &Conversation,
                _tools: &serde_json::Value,
            ) -> Result<Reply, ModelError> {
                Ok(Reply::parse(
                    r#"{"say": "again", "calls": [{"verb": "context", "args": {}}]}"#,
                ))
            }
        }
        let scratch = Scratch::default();
        let outcome = replay(
            &Insistent,
            &BTreeMap::<String, Agent<'_>>::new(),
            &scratch.filing(),
            &deployment("life"),
            "system prompt",
            "a message needs triage".to_owned(),
        );
        let Outcome::Answered(answer) = outcome else {
            panic!("the round bound ends in an answer, not a failure: {outcome:?}");
        };
        assert_eq!(
            u32::try_from(answer.rounds.len()).expect("a small round count"),
            MAX_ROUNDS
        );
    }
}
