//! The secretary: a trusted assistant in a conversation the owner is reading, which appends
//! as itself and proposes, but cannot send, anything outward.

use prodrome_data::event::Actor;
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
use suzatary_base::capability::{Clock, Manual, Manuals};
use suzatary_base::chains::ChainPolicy;
use suzatary_base::conversation_id::ConversationId;
use suzatary_base::reference::{ProposalRef, Reference, TodoRef};
use suzatary_base::refusal::{Refusal, shown};
use suzatary_connect::deployment::Deployment;
use suzatary_connect::settings::Settables;
use suzatary_conversations::secretary::model::{Halt, Turn};
use suzatary_conversations::secretary::store::{Entry, Stored};
use suzatary_conversations::secretary::taint::Taint;
use suzatary_conversations::secretary::tool_name;
use suzatary_digest::secretary::memories;
use suzatary_inbox::secretary::proposal;
use suzatary_inbox::secretary::{inbox, own_round};
use suzatary_lines::secretary::conversations::{append, read};
use suzatary_lines::secretary::transcript;
use suzatary_lines::secretary::transcript::Retry;
use suzatary_topics::secretary::{build, readings};
use verb_answer::VerbAnswer;

/// `SECRETARY.md`; without it the model is told to propose nothing.
#[must_use]
pub fn system_prompt(manuals: &dyn Manuals) -> String {
    manuals.document(Manual::Secretary).unwrap_or_else(|| {
        "SECRETARY.md is not present in this checkout. Answer only what you are \
         asked, propose nothing, and say that the secretary's brief is missing."
            .to_owned()
    })
}

/// Who a chain write from this surface is by, fixed here so no call's arguments can change it.
pub const WRITES_AS: Writer = Writer::Assistant;

pub const MAX_ROUNDS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    pub stored: Stored,
    /// How many times the model was asked; one when it answered without a verb.
    pub rounds: usize,
    pub ending: crate::conversation::round::Ending,
}

#[derive(Debug, Clone, Copy)]
pub struct Saying<'a> {
    pub text: &'a str,
    pub references: &'a [Reference],
    pub by: Asker<'a>,
}

#[derive(Debug, Clone, Copy)]
pub enum Asker<'a> {
    Person(Speaker<'a>),
    Box(own_round::OwnRound),
}

/// Whose own words a [`Saying`] is; the gate's to claim.
#[derive(Debug, Clone, Copy)]
pub struct Speaker<'a>(&'a Actor);

impl Speaker<'_> {
    #[cfg(feature = "fixture")]
    #[must_use]
    pub fn scripted(actor: &Actor) -> Speaker<'_> {
        Speaker(actor)
    }
}

/// One exchange, appended as it happens; a failing model ends the round in [`Answered::ending`].
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
        || -> Pick<'_> { Box::new(caps.model) },
        deployment,
        settables,
        chains,
        id,
        system,
        &Subset::ALL,
        &Sink::quiet(&Halt::new()),
        Retry::Allowed,
        Holder::Alone,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    /// The server's worker, which reads its conversation again after each round, so a `Turn`
    /// frame and a push come between two: one round.
    Task,
    /// A holder no task reads again (`ask`, `brief`, the review): rounds while one is owed.
    Alone,
}

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
    retry: Retry,
    holder: Holder,
) -> Result<Option<Answered>, Refusal> {
    if !transcript::read(caps.conversations, id)?.owes_a_round() {
        return Ok(None);
    }
    let pick = pick();
    let load = || load_references(caps, deployment, settables, chains, id);
    let confirms = confirming(chains, caps.clock);
    let run = |request: Request<SecretaryCall>, sent| {
        self::run(deployment, settables, chains, id, request, sent)
    };
    let ask = Ask::Digest {
        model: pick.as_ref(),
        memories: &caps.memories,
        topics: &caps.topics,
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
            journal: caps.journal,
            system,
            verbs,
            run: &run,
            bound: MAX_ROUNDS,
            sink,
            load: &load,
            retry,
            confirms: &confirms,
        },
    )
}

/// Whether every todo some references name is confirmed, as `chains` read when asked.
pub(crate) fn confirming<'c>(
    chains: &'c [Bound<'_>],
    clock: &'c dyn Clock,
) -> impl Fn(&[suzatary_base::reference::Reference]) -> bool + 'c {
    move |references| {
        !references.iter().any(|reference| {
            matches!(
                reference,
                suzatary_base::reference::Reference::Todo(_)
                    | suzatary_base::reference::Reference::Global(_)
            )
        }) || {
            let read = chains_read(chains);
            let set = read.set(clock.now());
            let speaker = suzatary_inbox::secretary::stamp::Speaker::device("");
            matches!(
                suzatary_inbox::secretary::stamp::pointing(speaker, references, &set),
                suzatary_inbox::secretary::stamp::Stamp::Actor(_)
            )
        }
    }
}

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

/// A [`Round`] under `rules`, then another while one is owed: a round's run holds the
/// conversation, so a question said during its last ask is joined by no one and the holder claims
/// again; a stop ends the rounds, since its halt stays pulled.
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

/// The call that reads what `reference` names again, as a round's load reads it: a marker's
/// "calling … answers again" for an answer the owner's pointing loaded; `None` for one no verb
/// reads.
#[must_use]
pub(crate) fn again(
    reference: &Reference,
) -> Option<suzatary_conversations::secretary::model::ToolCall> {
    call_for(reference).map(
        |(verb, args)| suzatary_conversations::secretary::model::ToolCall {
            verb: verb.into(),
            args: args.into_iter().collect(),
        },
    )
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
        Reference::Briefing { date } => {
            args.insert("day".to_owned(), Value::from(date.to_string()));
            Some(("briefing", args))
        }
        Reference::Global(_)
        | Reference::Proposal(_)
        | Reference::Batch { .. }
        | Reference::Target(_) => None,
    }
}

/// The secretary's verbs read a todo by its chain.
fn by_genesis(global: &suzatary_base::reference::GlobalTodo) -> Refusal {
    Refusal::not_found(format!(
        "{global} names a todo by its store's genesis; ask about it by its chain"
    ))
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
        Reference::Global(global) => Err(by_genesis(global)),
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
        Reference::Todo(_)
        | Reference::Mail { .. }
        | Reference::Thread { .. }
        | Reference::Briefing { .. } => Err(Refusal::internal(
            "a reference with a verb reached the store",
        )),
    }
}

/// One chain the secretary may read and write; the first is what an absent `chain` means.
#[derive(Clone, Copy)]
pub struct Bound<'a> {
    pub name: &'a str,
    /// The effects over this store; all but `chain` and `proposals` are the box's.
    pub caps: Secretary<'a>,
    pub policy: &'a ChainPolicy,
}

impl<'a> Bound<'a> {
    #[must_use]
    pub fn chain(&self) -> build::Chain<'a> {
        (self.policy, self.caps.chain)
    }
}

/// The chain the box files its own todos on (`[upkeep]`'s, `None` when the deployment names
/// none); every automated filer takes its chain from here.
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

pub fn call(
    deployment: &Deployment,
    settables: &Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    verb: &str,
    args: &Map<String, Value>,
) -> Result<VerbAnswer, Refusal> {
    let request = Subset::ALL.find(&verb.into())?.request(args)?;
    // Outside a round nothing was sent: the gate reads what an ask of the conversation now
    // would carry, and the worst where there is no conversation to read.
    let bound = bound_of(chains, request.chain.as_deref())?;
    let sent = suzatary_lines::secretary::conversations::read(bound.caps.conversations, id)
        .map_or(Taint::Tainted, |stored| {
            suzatary_conversations::secretary::taint::held(&stored)
        });
    run(deployment, settables, chains, id, request, sent)
}

pub fn run(
    deployment: &Deployment,
    settables: &Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    request: Request<SecretaryCall>,
    sent: suzatary_conversations::secretary::taint::Taint,
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
            sent,
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

pub(crate) fn bound_named<'a>(chains: &'a [Bound<'a>], name: &str) -> Option<&'a Bound<'a>> {
    chains.iter().find(|bound| bound.name == name)
}

/// Answer a proposal no, with a reason the secretary reads next.
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Noon;

    impl Clock for Noon {
        fn now(&self) -> suzatary_base::capability::Datetime {
            suzatary_base::capability::Datetime::new(2026, 9, 1, 12, 0, 0, 0).expect("an instant")
        }
    }

    /// The gate confirms a line only where it can show who wrote each todo it names, so a todo
    /// named by its genesis is never confirmed, and a line naming no todo always is.
    #[test]
    fn a_todo_by_genesis_is_never_confirmed() {
        let confirms = confirming(&[], &Noon);
        let global: Reference = "todo:+3f2a9c1b2d4e/x".parse().expect("a reference");
        let mail: Reference = "mail:a1b2c3d4e5f6".parse().expect("a reference");
        assert!(!confirms(&[global, mail.clone()]));
        assert!(confirms(&[mail]));
    }

    #[test]
    fn the_only_actor_this_surface_can_write_as_is_the_assistant() {
        assert_eq!(WRITES_AS.actor(), "assistant");
        assert_ne!(WRITES_AS, crate::agent::WRITES_AS);
    }
}
