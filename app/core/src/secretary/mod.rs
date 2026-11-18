//! The secretary: a trusted assistant in a conversation the owner is reading, which appends
//! as itself and proposes, but cannot send, anything outward.

use prodrome_core::event::Actor;
use suzatary_base::writer::Writer;
use suzatary_conversations::secretary::model::Pick;
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_signatures::chain_set::Read;
pub mod answering;
pub mod briefings;
pub mod briefs;
mod call;
pub mod daily;
pub(crate) mod file_query;
pub mod file_search;
pub mod gate;
#[cfg(any(test, feature = "nothing"))]
pub mod live_shape;
pub mod mail_health;
pub mod opening;
pub mod read_holders;
mod record;
pub mod review;
pub mod surface;
pub mod sweep;
pub mod token_expiry;
pub mod verb_answer;
pub mod verbs;

use serde_json::{Map, Value};

use crate::conversation::ask::Ask;
use crate::conversation::round::{Ending, Round, Rules, Sink, Transcript, Why};
use crate::verb::Request;
pub use call::Call;
pub use record::Secretary;
pub(crate) use surface::SecretarySurface;
use surface::Subset;
pub use surface::{SecretaryCall, SecretaryVerb, tools};
use suzatary_base::capability::Clock;
use suzatary_base::chains::ChainPolicy;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::{ProposalRef, Reference, TodoRef};
use suzatary_base::refusal::{Refusal, shown};
use suzatary_connect::deployment::Deployment;
use suzatary_connect::settings::Settables;
use suzatary_conversations::secretary::model::{Halt, Turn};
use suzatary_conversations::secretary::store::{Entry, Stored};
use suzatary_conversations::secretary::{store, tool_name};
use suzatary_digest::secretary::memories;
use suzatary_inbox::secretary::proposal;
use suzatary_inbox::secretary::{inbox, own_round, stamp};
use suzatary_lines::secretary::conversations::{append, read};
use suzatary_lines::secretary::transcript;
use suzatary_topics::secretary::{build, readings};
use verb_answer::VerbAnswer;

/// Who a chain write from this surface is by, fixed here so no call's arguments can change it.
pub const WRITES_AS: Writer = Writer::Assistant;

/// How many rounds one `say` may take.
pub const MAX_ROUNDS: usize = 8;

/// What one `say` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    /// The conversation as it now stands.
    pub stored: Stored,
    /// How many times the model was asked; one when it answered without a verb.
    pub rounds: usize,
    /// Whether the model answered, or the box ended the round and said why.
    pub ending: crate::conversation::round::Ending,
}

/// A question as [`converse`] takes it.
#[derive(Debug, Clone, Copy)]
pub struct Saying<'a> {
    pub text: &'a str,
    pub references: &'a [Reference],
    pub by: Asker<'a>,
}

/// Who asks a [`Saying`].
#[derive(Debug, Clone, Copy)]
pub enum Asker<'a> {
    /// A person, in their own words.
    Person(Speaker<'a>),
    /// The box, opening one of its own rounds.
    Box(own_round::OwnRound),
}

/// Whose own words a [`Saying`] is; the gate's to claim.
#[derive(Debug, Clone, Copy)]
pub struct Speaker<'a>(&'a Actor);

impl Speaker<'_> {
    /// The fixture's scripted person.
    #[cfg(feature = "fixture")]
    #[must_use]
    pub fn scripted(actor: &Actor) -> Speaker<'_> {
        Speaker(actor)
    }
}

/// One exchange, appended as it happens.
///
/// # Errors
/// The store's and an unreadable file's; a model that fails ends the round in
/// [`Answered::ending`].
pub fn converse(
    caps: &Secretary<'_>,
    deployment: &Deployment,
    settables: &Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    said: Saying<'_>,
    system: &str,
) -> Result<Option<Answered>, Refusal> {
    let now = caps.clock.now();
    let text = said.text.to_owned();
    let references = said.references.to_vec();
    append(
        caps.conversations,
        id,
        &match said.by {
            Asker::Person(Speaker(actor)) => Entry::Said {
                at: now,
                turn: Turn::Person {
                    text,
                    references,
                    attachments: Vec::new(),
                },
                by: Some(actor.as_str().into()),
            },
            Asker::Box(round) => Entry::asked_by_the_box(now, round, text, references),
        },
    )?;
    answer(
        caps,
        || Pick::Turn(Box::new(caps.model)),
        deployment,
        settables,
        chains,
        id,
        system,
        &Subset::ALL,
        &Sink::quiet(&Halt::new()),
        suzatary_lines::secretary::transcript::Retry::Allowed,
        Holder::Alone,
    )
}

/// Who runs a conversation's rounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// The server's worker, whose task reads its conversation again after each round, so a
    /// `Turn` frame and a push come between two: one round.
    Task,
    /// A holder with no task to read it again (`ask`, `brief`, the review): a round, then another
    /// while one is owed.
    Alone,
}

/// Answer what waits with the model `pick` names, as `holder` runs rounds.
///
/// # Errors
/// As [`converse`].
#[expect(
    clippy::too_many_arguments,
    reason = "a round's model, its deployment and live names, its chains, its verbs, its sink and its holder"
)]
pub fn answer<'p>(
    caps: &Secretary<'_>,
    pick: impl FnOnce() -> Pick<'p>,
    deployment: &Deployment,
    settables: &Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    system: &str,
    verbs: &Subset,
    sink: &Sink<'_>,
    retry: suzatary_lines::secretary::transcript::Retry,
    holder: Holder,
) -> Result<Option<Answered>, Refusal> {
    if !transcript::read(caps.conversations, id)?.owes_a_round() {
        return Ok(None);
    }
    let pick = pick();
    let load = || load_references(caps, deployment, settables, chains, id);
    let run =
        |request: Request<SecretaryCall>| self::run(deployment, settables, chains, id, request);
    // A lost session is asked statelessly of the same model, never of a second pick.
    let ask = match &pick {
        Pick::Held(model) => Ask::Session {
            model: *model,
            fallback: *model,
            memories: caps.memories,
            topics: caps.topics,
        },
        Pick::Turn(model) => Ask::Window {
            model: model.as_ref(),
            memories: caps.memories,
            topics: caps.topics,
        },
    };
    let rounds = match holder {
        Holder::Task => rounded,
        Holder::Alone => held,
    };
    rounds(
        caps,
        id,
        &Rules {
            ask,
            system,
            verbs,
            run: &run,
            bound: MAX_ROUNDS,
            sink,
            load: &load,
            retry,
        },
    )
}

/// Stop what waits before any round has started on it, as `by`.
///
/// # Errors
/// [`Refusal::conflict`] when nothing waits; the store's failures.
pub fn stop_waiting(
    conversations: Conversations<'_>,
    clock: &dyn Clock,
    id: &ConversationId,
    by: &Actor,
) -> Result<(), Refusal> {
    if !transcript::read(conversations, id)?.owes_a_round() {
        return Err(Refusal::conflict(format!(
            "nothing is being answered in {id}, so there is nothing to stop"
        )));
    }
    transcript::push(
        conversations,
        id,
        &Entry::Stopped {
            at: clock.now(),
            by: by.as_str().to_owned(),
        },
    )?;
    Ok(())
}

/// One [`Round`] under `rules`, and the conversation as it leaves it.
pub(crate) fn rounded(
    caps: &Secretary<'_>,
    id: &ConversationId,
    rules: &Rules<'_>,
) -> Result<Option<Answered>, Refusal> {
    let transcript = Transcript {
        conversations: caps.conversations,
        clock: caps.clock,
        id,
    };
    Round::answer(rules, &transcript)?
        .map(|closed| {
            Ok(Answered {
                stored: read(caps.conversations, id)?,
                rounds: closed.round.steps,
                ending: closed.ending,
            })
        })
        .transpose()
}

/// A [`Round`] under `rules`, then another while one is owed, and the conversation as they leave
/// it: its first round's ending, and every round's asks: [`Holder::Alone`]'s, and the sweep's.
/// While a round runs, its
/// run holds the conversation, so another holder's claim step finds nothing to do, and a
/// question said during its last ask is joined by no one; the holder claims again after its own
/// round, so that question is answered now and not at the next write. A stop ends the holder's
/// rounds, since the halt it pulled stays pulled.
pub(crate) fn held(
    caps: &Secretary<'_>,
    id: &ConversationId,
    rules: &Rules<'_>,
) -> Result<Option<Answered>, Refusal> {
    let transcript = Transcript {
        conversations: caps.conversations,
        clock: caps.clock,
        id,
    };
    let mut first: Option<Ending> = None;
    let mut asks = 0;
    while let Some(closed) = Round::answer(rules, &transcript)? {
        asks += closed.round.steps;
        let stopped = matches!(
            closed.ending,
            Ending::Ended {
                why: Why::Stopped(_)
            }
        );
        first.get_or_insert(closed.ending);
        if stopped {
            break;
        }
    }
    first
        .map(|ending| {
            Ok(Answered {
                stored: read(caps.conversations, id)?,
                rounds: asks,
                ending,
            })
        })
        .transpose()
}

/// What the last turn pointed at, loaded before the loop.
fn load_references(
    caps: &Secretary<'_>,
    deployment: &Deployment,
    settables: &Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
) -> Result<(), Refusal> {
    let stored = read(caps.conversations, id)?;
    let references = match stored.live().last().map(|(_, said)| &said.turn) {
        Some(Turn::Person { references, .. } | Turn::Act { references, .. }) => references.clone(),
        Some(Turn::Model { .. } | Turn::Tool { .. } | Turn::Erased { .. }) | None => return Ok(()),
    };
    for reference in &references {
        let (answer, outcome, files) = VerbAnswer::said(
            load(caps, deployment, settables, chains, id, reference).map(Into::into),
        );
        append(
            caps.conversations,
            id,
            &Entry::Said {
                at: caps.clock.now(),
                turn: Turn::Tool {
                    verb: tool_name::ToolName::Reference,
                    // The text form first, so the model can name the thing back.
                    answer: format!("{reference}\n\n{answer}"),
                    outcome,
                    files,
                },
                by: None,
            },
        )?;
    }
    Ok(())
}

/// The call a reference is; `None` for one no verb answers.
fn call_for(reference: &Reference) -> Option<(&'static str, Map<String, Value>)> {
    let mut args = Map::new();
    match reference {
        Reference::Todo(TodoRef { chain, id }) => {
            args.insert("todo_id".to_owned(), Value::from(id.as_str()));
            args.insert("chain".to_owned(), Value::from(chain.as_str()));
            Some(("todo_show", args))
        }
        Reference::Mail { hash } => {
            args.insert("id".to_owned(), Value::from(hash.as_str()));
            Some(("show", args))
        }
        Reference::Thread { id } => {
            args.insert(
                "query".to_owned(),
                Value::from(format!("thread:{}", id.as_str())),
            );
            Some(("search", args))
        }
        Reference::Briefing { .. }
        | Reference::Proposal(_)
        | Reference::Batch { .. }
        | Reference::Target(_) => None,
    }
}

fn load(
    caps: &Secretary<'_>,
    deployment: &Deployment,
    settables: &Settables,
    chains: &[Bound<'_>],
    here: &ConversationId,
    reference: &Reference,
) -> Result<Value, Refusal> {
    if let Some((verb, args)) = call_for(reference) {
        return call(deployment, settables, chains, here, verb, &args).map(|answer| answer.value);
    }
    match reference {
        Reference::Briefing { date } => {
            let mut args = Map::new();
            args.insert("day".to_owned(), Value::from(date.to_string()));
            call(deployment, settables, chains, here, "briefing", &args).map(|answer| answer.value)
        }
        Reference::Proposal(ProposalRef {
            conversation,
            index,
        }) => {
            let standing = read(caps.conversations, conversation)?
                .standing(*index, &inbox::fold(caps.inbox)?)?;
            let act = match &standing.object.asked.proposal {
                proposal::Proposal::Email(draft) => serde_json::json!({
                    "act": "email",
                    "account": draft.account,
                    "to": draft.to,
                    "cc": draft.cc,
                    "subject": draft.subject,
                    "body": draft.body,
                }),
                proposal::Proposal::Forward(forward) => {
                    serde_json::json!({
                        "act": "forward",
                        "account": forward.account,
                        "to": forward.to,
                        "cc": forward.cc,
                        "subject": forward.subject,
                        "note": forward.note,
                        "message": forward.message.file.id,
                        "as": forward.message.style.as_str(),
                    })
                }
                proposal::Proposal::Changes(changeset) => {
                    serde_json::json!({
                        "act": "changes",
                        "chain": changeset.chain(),
                        "acts": changeset.acts(),
                    })
                }
                // The act's own sentence, rather than its fields decoded a second way.
                proposal::Proposal::Act(act) => serde_json::json!({
                    "act": "act",
                    "sentence": act.sentence(),
                }),
                // The words while the memory's text store keeps them, and never once deleted.
                proposal::Proposal::Remember {
                    text,
                    kind,
                    replaces,
                } => {
                    let mut card = match caps.memories.reads().words(text)? {
                        Some(fact) => serde_json::json!({
                            "act": "remember",
                            "fact": fact.as_str(),
                            "kind": kind.as_str(),
                        }),
                        None => serde_json::json!({
                            "act": "remember",
                            "deleted": "its words are deleted: dismissed, or the fact forgotten",
                            "kind": kind.as_str(),
                        }),
                    };
                    if let suzatary_memory::secretary::memory::Replaces::Facts(from) = replaces {
                        card["act"] = "consolidate".into();
                        card["replaces"] = from.ids().iter().map(|id| id.0).collect();
                    }
                    card
                }
                proposal::Proposal::Decide(draft) => serde_json::json!({
                    "act": "decide",
                    "sentence": suzatary_inbox::secretary::decide::sentence(draft),
                }),
            };
            Ok(serde_json::json!({
                "conversation": conversation.as_str(),
                "index": index,
                "proposal": act,
                "why": standing.object.asked.reasons.why(),
                "sources": standing.object.asked.reasons.sources().iter().map(ToString::to_string).collect::<Vec<_>>(),
                "state": readings::state(&standing.object.reading).0,
                "tries": standing.object.tries(),
            }))
        }
        // A batch is the replica's, which the secretary's record does not hold.
        Reference::Batch { chain, head } => Err(Refusal::not_found(format!(
            "batch:{chain}/{} is not readable from the conversation; ask about the \
             decision that names it instead",
            head.as_str()
        ))),
        // A round or a note is shown by its topics; its words are the conversation's.
        Reference::Target(target) => Ok(suzatary_topics::secretary::topics::revision::held(
            &caps.topics.reading()?,
            target,
        )),
        Reference::Todo(_) | Reference::Mail { .. } | Reference::Thread { .. } => Err(
            Refusal::internal("a reference with a verb reached the store"),
        ),
    }
}

/// One chain the secretary may read and write; the first is what an absent `chain` means.
#[derive(Clone, Copy)]
pub struct Bound<'a> {
    /// The name a `chain` argument spells.
    pub name: &'a str,
    /// The effects over this store; all but `chain` and `proposals` are the box's.
    pub caps: Secretary<'a>,
    /// Whose events bind here.
    pub policy: &'a ChainPolicy,
}

impl<'a> Bound<'a> {
    /// This chain as a reading of every chain's todos takes it: its policy and its store.
    #[must_use]
    pub fn chain(&self) -> build::Chain<'a> {
        (self.policy, self.caps.chain)
    }
}

/// The chain among `chains` that the box files its own todos on: `[upkeep]`'s, or `None` when
/// the deployment names none. Every automated filer takes its chain from here.
///
/// # Errors
/// [`Refusal::internal`] when `chains` does not bind the chain `[upkeep]` names.
pub fn upkeep<'c, 'a>(
    chains: &'c [Bound<'a>],
    deployment: &Deployment,
) -> Result<Option<&'c Bound<'a>>, Refusal> {
    deployment
        .upkeep
        .as_ref()
        .map(|named| {
            chains
                .iter()
                .find(|bound| bound.name == named.as_str())
                .ok_or_else(|| Refusal::internal(format!("[upkeep] chain {named} is not bound")))
        })
        .transpose()
}

/// The chains a call may read, each read once ([`Read`]).
#[must_use]
pub(crate) fn chains_read(chains: &[Bound<'_>]) -> Read {
    Read::of_stores(chains.iter().map(Bound::chain))
}

/// Call the verb `verb` names with `args`, read whole first.
///
/// # Errors
/// An unknown verb or chain, a missing or ill-typed argument, or a capability's failure.
pub fn call(
    deployment: &Deployment,
    settables: &Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    verb: &str,
    args: &Map<String, Value>,
) -> Result<VerbAnswer, Refusal> {
    let request = Subset::ALL.find(&verb.into())?.request(args)?;
    run(deployment, settables, chains, id, request)
}

/// Run one call against the chain it named.
///
/// # Errors
/// An unknown chain, or whatever the verb refuses.
pub fn run(
    deployment: &Deployment,
    settables: &Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    request: Request<SecretaryCall>,
) -> Result<VerbAnswer, Refusal> {
    let bound = bound_of(chains, request.chain.as_deref())?;
    // Whatever a verb read, no identity number reaches the model: mail's are placeholders the
    // owner can reveal already, and anything else (Junk, a page, a calendar) is withheld here.
    request
        .call
        .run(&Call {
            caps: &bound.caps,
            deployment,
            settables,
            chain: bound.policy,
            chains,
            conversation: id,
        })
        .map(|answer| VerbAnswer {
            value: suzatary_held::held_value::withheld_value(answer.value),
            ..answer
        })
}

/// Which chain a call is about: absent means the first, an unknown name is refused.
pub fn bound_of<'a>(
    chains: &'a [Bound<'a>],
    named: Option<&str>,
) -> Result<&'a Bound<'a>, Refusal> {
    match named {
        None => chains
            .first()
            .ok_or_else(|| Refusal::internal("a deployment names at least one chain")),
        Some(name) => chains
            .iter()
            .find(|bound| bound.name == name)
            .ok_or_else(|| {
                Refusal::not_found(format!(
                    "no chain named {} — this deployment serves {}",
                    shown(name),
                    chains
                        .iter()
                        .map(|bound| bound.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }),
    }
}

/// The bound chain named, or none.
pub(crate) fn bound_named<'a>(chains: &'a [Bound<'a>], name: &str) -> Option<&'a Bound<'a>> {
    chains.iter().find(|bound| bound.name == name)
}

/// Answer a proposal no, with a reason the secretary reads next.
///
/// # Errors
/// An empty reason, an index nothing proposed, one performed or running, or the stores'.
pub fn dismiss(
    conversations: Conversations<'_>,
    memories: memories::Memories<'_>,
    clock: &dyn Clock,
    by: &Actor,
    card: &ProposalRef,
    reason: &str,
) -> Result<Stored, Refusal> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(Refusal::bad_request(
            "say why: the reason is the only part of a dismissal the next answer can use",
        ));
    }
    let inbox = memories.cards;
    let standing =
        read(conversations, &card.conversation)?.standing(card.index, &inbox::fold(inbox)?)?;
    let answered = inbox::dismiss(inbox, &standing.object.id, by, clock.now(), reason)?;
    if heard(
        conversations,
        clock,
        by,
        card,
        ("dismissed", reason),
        &answered,
    )? {
        // After the dismissal and its line, both written: a refused sweep is the next one's.
        memories.sweep_after_write();
    }
    read(conversations, &card.conversation)
}

/// Record that the owner has checked Sent for proposal `index`; the send is never retried.
///
/// # Errors
/// [`inbox::acknowledge`]'s refusals and the stores' own.
pub fn acknowledge(
    conversations: Conversations<'_>,
    inbox: &dyn suzatary_store::store::Prodrome<suzatary_inbox::secretary::inbox_event::InboxEvent>,
    clock: &dyn Clock,
    by: &Actor,
    card: &ProposalRef,
) -> Result<Stored, Refusal> {
    let standing =
        read(conversations, &card.conversation)?.standing(card.index, &inbox::fold(inbox)?)?;
    let answered = inbox::acknowledge(inbox, &standing.object.id, by, clock.now())?;
    let said = (
        "checked Sent for",
        "it may have gone, and it is never sent again",
    );
    heard(conversations, clock, by, card, said, &answered)?;
    read(conversations, &card.conversation)
}

/// Say in `card`'s conversation what `by` did to it; answers whether it did.
fn heard(
    conversations: Conversations<'_>,
    clock: &dyn Clock,
    by: &Actor,
    card: &ProposalRef,
    (verb, words): (&str, &str),
    answered: &inbox::Response,
) -> Result<bool, Refusal> {
    if !matches!(answered, inbox::Response::Ran(_)) {
        return Ok(false);
    }
    append(
        conversations,
        &card.conversation,
        &Entry::Said {
            at: clock.now(),
            turn: Turn::Act {
                text: suzatary_lines::secretary::act_lines::describe_act(
                    &suzatary_base::spelling::capitalised(by.as_str()),
                    verb,
                    words,
                ),
                references: vec![card.clone().into()],
            },
            by: Some(by.as_str().into()),
        },
    )?;
    Ok(true)
}

/// Ask the last round again.
///
/// # Errors
/// [`Stored::rewind`]'s refusals and the store's own.
pub fn regenerate(
    conversations: Conversations<'_>,
    clock: &dyn Clock,
    by: &Actor,
    id: &ConversationId,
) -> Result<Stored, Refusal> {
    let to = read(conversations, id)?.rewind(store::Rewind::Regenerate)?;
    rewound(conversations, clock, by, id, to)?;
    read(conversations, id)
}

/// Ask the last round again with `said` in place of its question.
///
/// # Errors
/// [`Stored::rewind`]'s refusals, a last round no question opened, and the store's own.
pub fn edit(
    conversations: Conversations<'_>,
    clock: &dyn Clock,
    by: &Actor,
    stamped: stamp::Stamp,
    id: &ConversationId,
    said: Turn,
) -> Result<Stored, Refusal> {
    let to = read(conversations, id)?.rewind(store::Rewind::Edit)?;
    rewound(conversations, clock, by, id, to)?;
    append(
        conversations,
        id,
        &Entry::Said {
            at: clock.now(),
            turn: said,
            by: Some(stamped),
        },
    )?;
    read(conversations, id)
}

fn rewound(
    conversations: Conversations<'_>,
    clock: &dyn Clock,
    by: &Actor,
    id: &ConversationId,
    to: usize,
) -> Result<(), Refusal> {
    append(
        conversations,
        id,
        &Entry::Rewound {
            to,
            at: clock.now(),
            by: by.as_str().to_owned(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The actor is this module's constant, and it is the trusted one.
    #[test]
    fn the_only_actor_this_surface_can_write_as_is_the_assistant() {
        assert_eq!(WRITES_AS.actor(), "assistant");
        assert_ne!(WRITES_AS, crate::agent::WRITES_AS);
    }
}
