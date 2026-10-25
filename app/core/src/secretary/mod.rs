//! The secretary: a trusted assistant in a conversation the owner is reading, which appends
//! as itself and proposes, but cannot send, anything outward.

pub mod about;
pub mod act_vocabulary;
pub mod acts;
pub mod answer;
pub mod attached;
pub mod attachment_source;
pub mod batch_cards;
pub mod bootstrap;
pub mod briefs;
mod call;
pub mod card;
pub mod carried;
pub mod caused;
pub mod changes;
pub mod coding;
pub mod compact_ask;
pub mod compact_budget;
pub mod compacting;
pub mod compaction;
pub mod compactor;
pub mod connections;
#[cfg(feature = "wasm")]
mod conversation_json;
pub mod conversations;
pub mod daily;
pub mod decide;
pub mod diff;
pub mod digest;
pub mod digest_event;
pub mod digest_label;
pub mod digest_line;
pub mod digest_policy;
pub mod digest_spent;
pub mod digest_view;
pub mod digests;
pub mod erasure;
pub mod fact_life;
pub mod failing;
pub mod file_query;
pub mod file_search;
pub mod filed;
pub mod fixes;
pub mod found;
pub mod google_sign_in;
pub mod heartbeat;
pub mod held_lines;
pub mod inbox;
pub mod inbox_event;
#[cfg(feature = "wasm")]
mod inbox_json;
pub mod inbox_reading;
pub mod inbox_registers;
pub mod key_ring;
pub mod line_event;
pub mod line_shape;
pub mod line_words;
pub mod locked_text;
pub mod mail_health;
pub mod memories;
pub mod memory;
pub mod memory_event;
pub mod noticed;
pub mod opened;
pub mod outcome;
pub mod owners_record;
pub mod phase;
pub mod proposer;
pub mod queue;
pub mod read_mail;
pub mod readings;
pub mod recollection;
pub mod responded;
pub mod review;
pub mod routes;
pub mod rules;
pub mod salted_text;
pub mod search;
pub mod sign_in;
pub mod slots;
pub mod stamp;
pub mod store;
pub mod stream;
pub mod surface;
pub mod sweep;
pub mod taint;
pub mod tap;
pub mod tapped;
pub mod texts;
pub mod title;
pub mod told;
pub mod tool_name;
pub mod topics;
pub mod transcript;
pub mod turn_shape;
pub mod two_phase;
pub mod upload_standing;
pub mod verb_answer;
pub mod verbs;
pub mod words;

use serde_json::{Map, Value};

use crate::capability::{Clock, Datetime, Halt, Model, Secretary, Turn};
use crate::chains::ChainPolicy;
use crate::conversation::ask::Ask;
use crate::conversation::round::{Round, Rules, Sink, Transcript};
use crate::deployment::Deployment;
use crate::reference::{ProposalRef, Reference, TodoRef};
use crate::refusal::{Refusal, Status, shown};
use crate::verb::Request;
pub use call::Call;
use store::{ConversationId, Entry, Stored};
pub use store::{ConversationState, state};
use surface::Subset;
pub use surface::{Effect, Powers, SecretaryCall, SecretarySurface, SecretaryVerb, tools};
use verb_answer::VerbAnswer;

/// Who a chain write from this surface is by: the trusted assistant, never the owner or
/// `triage`, fixed here so no call's arguments can change it.
pub const WRITES_AS: crate::authoring::Writer = crate::authoring::Writer::Assistant;

/// [`WRITES_AS`] as the actor an inbox event names.
pub(crate) fn writes_as() -> prodrome_core::event::Actor {
    prodrome_core::event::Actor::new(WRITES_AS.actor()).expect("assistant is an actor")
}

/// How many rounds one `say` may take; at the limit the transcript says the rounds ran out.
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

/// The push a finished round earns: `Some` only for an answer, naming the conversation and
/// never its words.
#[must_use]
pub fn answer_push(stored: &Stored, brief: bool) -> Option<crate::capability::Push> {
    let Some(Turn::Model { text, calls }) = stored.turns.last().map(|said| &said.turn) else {
        return None;
    };
    (calls.is_empty() && !text.trim().is_empty()).then(|| crate::capability::Push::Answered {
        conversation: stored.id.clone(),
        brief,
    })
}

/// The first line of typst `text` that reads as prose: function-call lines are skipped and
/// leading heading, list and whole-line emphasis marks stripped; not a typst parser.
pub(crate) fn typst_prose_line(text: &str) -> Option<&str> {
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let leading = line.chars().next();
        let mut candidate = line
            .trim_start_matches(['=', '-', '+', '*', '_', '/', ' '])
            .trim();
        if let Some(mark @ ('*' | '_')) = leading
            && let Some(rest) = candidate.strip_suffix(mark)
        {
            candidate = rest.trim();
        }
        if !candidate.is_empty() {
            return Some(candidate);
        }
    }
    None
}

/// A person's turn as [`converse`] takes it: the words, what they point at, and whose own words
/// they are; `None` is a round the box opened.
#[derive(Debug, Clone, Copy)]
pub struct Saying<'a> {
    /// The words.
    pub text: &'a str,
    /// What they point at.
    pub references: &'a [Reference],
    /// Whose own words they are.
    pub by: Option<Speaker<'a>>,
}

/// Whose own words a [`Saying`] is. A person's words make a round clean, so claiming them is the
/// gate's; a direct caller of [`converse`] stands for no gate, and only the e2e fixture's
/// scripted round, a build the box never runs, mints one.
#[derive(Debug, Clone, Copy)]
pub struct Speaker<'a>(&'a prodrome_core::event::Actor);

impl Speaker<'_> {
    /// The fixture's scripted person.
    #[cfg(feature = "fixture")]
    #[must_use]
    pub fn scripted(actor: &prodrome_core::event::Actor) -> Speaker<'_> {
        Speaker(actor)
    }
}

/// One exchange: a person's turn in, the model's answer and every verb it ran out, appended as
/// it happens so a polling page sees turns arrive and a crash leaves an honest transcript.
///
/// # Errors
/// The store's failures and an unreadable conversation file; a model that fails ends the round
/// with the box's words instead, in [`Answered::ending`].
pub fn converse(
    caps: &Secretary<'_>,
    deployment: &Deployment,
    settables: &crate::settings::Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    said: Saying<'_>,
    system: &str,
) -> Result<Option<Answered>, Refusal> {
    let now = caps.clock.now();
    append(
        caps.conversations,
        id,
        &Entry::Said {
            at: now,
            turn: Turn::Person {
                text: said.text.to_owned(),
                references: said.references.to_vec(),
                attachments: Vec::new(),
            },
            by: said.by.map(|Speaker(actor)| actor.as_str().into()),
        },
    )?;
    answer(
        caps,
        || crate::capability::Pick::Turn(Box::new(caps.model)),
        deployment,
        settables,
        chains,
        id,
        system,
        &Subset::ALL,
        &Sink::quiet(&Halt::new()),
    )
}

/// Answer what waits in the conversation with the model `pick` names for this round, resuming
/// its session when it holds one; a stateless turn cannot be interrupted, so a pulled halt
/// drops its reply when it returns. The pick is made only once something waits, so a round with
/// nothing to answer reads no order and no key.
///
/// # Errors
/// The store's failures and an unreadable conversation file; a model that fails ends the round
/// with the box's words instead, in [`Answered::ending`].
#[expect(
    clippy::too_many_arguments,
    reason = "a round's model, its deployment and live names, its chains, its verbs and its sink"
)]
pub fn answer<'p>(
    caps: &Secretary<'_>,
    pick: impl FnOnce() -> crate::capability::Pick<'p>,
    deployment: &Deployment,
    settables: &crate::settings::Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    system: &str,
    verbs: &Subset,
    sink: &Sink<'_>,
) -> Result<Option<Answered>, Refusal> {
    if queue::Pending::of(&read(caps.conversations, id)?).is_none() {
        return Ok(None);
    }
    let pick = pick();
    let load = || load_references(caps, deployment, settables, chains, id);
    let run =
        |request: Request<SecretaryCall>| self::run(deployment, settables, chains, id, request);
    // A lost session that will not reopen is asked statelessly of the same model, never of a
    // second pick.
    let ask = match &pick {
        crate::capability::Pick::Held(model) => Ask::Session {
            model: *model,
            fallback: *model,
            memories: caps.memories,
            topics: caps.topics,
        },
        crate::capability::Pick::Turn(model) => Ask::Window {
            model: model.as_ref(),
            memories: caps.memories,
            topics: caps.topics,
        },
    };
    rounded(
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
        },
    )
}

/// Stop what waits in the conversation before any round has started on it, as `by`: the
/// [`Entry::Stopped`] line a stopped round writes, which closes everything queued, so the next
/// round finds nothing to answer. The caller holds off every round from starting until it returns.
///
/// # Errors
/// [`Refusal::conflict`] when nothing waits, so no stop is left for a later question; the store's
/// failures.
pub fn stop_waiting(
    conversations: crate::secretary::conversations::Conversations<'_>,
    clock: &dyn Clock,
    id: &ConversationId,
    by: &prodrome_core::event::Actor,
) -> Result<(), Refusal> {
    if queue::Pending::of(&read(conversations, id)?).is_none() {
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

/// What the last turn pointed at, loaded into the round through the verbs the model could call
/// itself, once, before the loop and spending no rounds.
fn load_references(
    caps: &Secretary<'_>,
    deployment: &Deployment,
    settables: &crate::settings::Settables,
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

/// The call a reference is, for the three arms a verb already answers; `None` for the rest.
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

/// Resolve one reference.
///
/// # Errors
/// Whatever the verb or the store answers.
fn load(
    caps: &Secretary<'_>,
    deployment: &Deployment,
    settables: &crate::settings::Settables,
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
            // The act's own fields, whichever act it is.
            let act = match &standing.object.asked.proposal {
                store::Proposal::Email(draft) => serde_json::json!({
                    "act": "email",
                    "account": draft.account,
                    "to": draft.to,
                    "cc": draft.cc,
                    "subject": draft.subject,
                    "body": draft.body,
                }),
                store::Proposal::Forward(forward) => serde_json::json!({
                    "act": "forward",
                    "account": forward.account,
                    "to": forward.to,
                    "cc": forward.cc,
                    "subject": forward.subject,
                    "note": forward.note,
                    "message": forward.message.file.id,
                    "as": forward.message.style.as_str(),
                }),
                store::Proposal::Changes(changeset) => serde_json::json!({
                    "act": "changes",
                    "chain": changeset.chain(),
                    "acts": changeset.acts(),
                }),
                // The act's own sentence, rather than its fields decoded a second way.
                store::Proposal::Act(act) => serde_json::json!({
                    "act": "act",
                    "sentence": act.sentence(),
                }),
                // The words while the memory's text store keeps them, and never once deleted.
                store::Proposal::Remember { text, kind } => match caps.memories.words(text)? {
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
                },
                store::Proposal::Decide(draft) => serde_json::json!({
                    "act": "decide",
                    "sentence": crate::secretary::decide::sentence(draft),
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
        Reference::Target(target) => {
            let topics: Vec<String> = caps
                .topics
                .reading()?
                .topics(target)
                .iter()
                .map(ToString::to_string)
                .collect();
            Ok(serde_json::json!({"target": target.to_string(), "topics": topics}))
        }
        Reference::Todo(_) | Reference::Mail { .. } | Reference::Thread { .. } => Err(
            Refusal::internal("a reference with a verb reached the store"),
        ),
    }
}

/// One chain the secretary may read and write: its store and its policy as one unit; the first
/// is what an absent `chain` means.
#[derive(Clone, Copy)]
pub struct Bound<'a> {
    /// The name a `chain` argument spells.
    pub name: &'a str,
    /// The effects over this store; all but `chain` and `proposals` are the box's.
    pub caps: Secretary<'a>,
    /// Whose events bind here.
    pub policy: &'a ChainPolicy,
}

/// The chains a call may read, each read once ([`crate::chain_set::Read`]): the set a proposal
/// and every row are priced through, each chain under its own policy.
#[must_use]
pub fn chains_read(chains: &[Bound<'_>]) -> crate::chain_set::Read {
    crate::chain_set::Read::of_stores(chains.iter().map(|bound| (bound.policy, bound.caps.chain)))
}

/// Call the verb `verb` names with `args`, read whole first.
///
/// # Errors
/// An unknown verb or chain, a missing or ill-typed argument, or a capability's failure.
pub fn call(
    deployment: &Deployment,
    settables: &crate::settings::Settables,
    chains: &[Bound<'_>],
    id: &ConversationId,
    verb: &str,
    args: &Map<String, Value>,
) -> Result<VerbAnswer, Refusal> {
    let request = Subset::ALL.find(&verb.into())?.request(args)?;
    run(deployment, settables, chains, id, request)
}

/// Run one call against the chain it named, resolved here because a model picks it per call.
///
/// # Errors
/// An unknown chain, or whatever the verb refuses.
pub fn run(
    deployment: &Deployment,
    settables: &crate::settings::Settables,
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
            value: crate::held_value::withheld_value(answer.value),
            ..answer
        })
}

/// Which chain a call is about: absent means the first, an unknown name is refused.
fn bound_of<'a>(chains: &'a [Bound<'a>], named: Option<&str>) -> Result<&'a Bound<'a>, Refusal> {
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

/// Answer a proposal no, with a reason, recorded as a [`Turn::Act`] the secretary reads next; a
/// remember card's words go with it, by the memory's sweep, unless a standing fact names them.
///
/// # Errors
/// An empty reason, an index nothing proposed, one performed or running, or the stores'.
pub fn dismiss(
    conversations: crate::secretary::conversations::Conversations<'_>,
    memories: memories::Memories<'_>,
    clock: &dyn Clock,
    by: &prodrome_core::event::Actor,
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
    conversations: crate::secretary::conversations::Conversations<'_>,
    inbox: &dyn crate::store::Prodrome<crate::secretary::inbox_event::InboxEvent>,
    clock: &dyn Clock,
    by: &prodrome_core::event::Actor,
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

/// Say in `card`'s conversation what `by` did to it (a verb and its words) as the
/// [`Turn::Act`] the secretary reads next, when the inbox `answered` that it recorded it; answers
/// whether it did.
fn heard(
    conversations: crate::secretary::conversations::Conversations<'_>,
    clock: &dyn Clock,
    by: &prodrome_core::event::Actor,
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
                text: acts::describe_act(&acts::named(by.as_str()), verb, words),
                references: vec![card.clone().into()],
            },
            by: Some(by.as_str().into()),
        },
    )?;
    Ok(true)
}

/// Ask the last round again: its trace and answer are set aside and the worker answers anew.
///
/// # Errors
/// [`Stored::rewind`]'s refusals and the store's own.
pub fn regenerate(
    conversations: crate::secretary::conversations::Conversations<'_>,
    clock: &dyn Clock,
    by: &prodrome_core::event::Actor,
    id: &ConversationId,
) -> Result<Stored, Refusal> {
    let to = read(conversations, id)?.rewind(store::Rewind::Regenerate)?;
    rewound(conversations, clock, by, id, to)?;
    read(conversations, id)
}

/// Ask the last round again with `said` in place of its question, set aside by `by` and stamped
/// `stamped` (the gate's actor, as [`stamp::pointing`] read what its references point at).
///
/// # Errors
/// [`Stored::rewind`]'s refusals, a last round no question opened, and the store's own.
pub fn edit(
    conversations: crate::secretary::conversations::Conversations<'_>,
    clock: &dyn Clock,
    by: &prodrome_core::event::Actor,
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
    conversations: crate::secretary::conversations::Conversations<'_>,
    clock: &dyn Clock,
    by: &prodrome_core::event::Actor,
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

/// Open a conversation with its first line; an empty `title` leaves it for [`title::owed`].
///
/// # Errors
/// The store's, and a name already taken.
pub fn start(
    conversations: crate::secretary::conversations::Conversations<'_>,
    model: &dyn Model,
    id: &ConversationId,
    title: &str,
    kind: store::Kind,
    at: Datetime,
) -> Result<Stored, Refusal> {
    let opened = Entry::Opened {
        at,
        title: crate::secretary::words::Words::kept(title.to_owned()),
        model: model.name(),
        kind,
    };
    transcript::start(conversations, id, &opened)?;
    read(conversations, id)
}

/// One conversation, folded.
///
/// # Errors
/// [`Refusal::not_found`] for an unknown name, [`Refusal::internal`] for an unreadable file.
pub fn read(
    conversations: crate::secretary::conversations::Conversations<'_>,
    id: &ConversationId,
) -> Result<Stored, Refusal> {
    Ok(transcript::read(conversations, id)?.into_stored())
}

/// [`read`], opening [`store::DEFAULT`] the first time anything asks for it; no other id is
/// created on demand, and a default another caller opened first is read.
///
/// # Errors
/// [`read`]'s own, once the default exists or `id` is any other name, and the store's.
pub fn read_or_open_default(
    conversations: crate::secretary::conversations::Conversations<'_>,
    model: &dyn Model,
    id: &ConversationId,
    at: Datetime,
) -> Result<Stored, Refusal> {
    if id.as_str() != store::DEFAULT.as_str() {
        return read(conversations, id);
    }
    match read(conversations, id) {
        Err(refusal) if refusal.status() == Status::NotFound => {
            match start(
                conversations,
                model,
                id,
                "Secretary",
                store::Kind::Default,
                at,
            ) {
                Err(taken) if taken.status() == Status::Conflict => read(conversations, id),
                opened => opened,
            }
        }
        other => other,
    }
}

/// Every conversation, newest first, each folded to its heading fields.
///
/// # Errors
/// The store's own.
pub fn list(
    conversations: crate::secretary::conversations::Conversations<'_>,
) -> Result<Vec<Stored>, Refusal> {
    transcript::each(conversations)?
        .into_iter()
        .map(|(_, read)| {
            read.map(transcript::Transcript::into_stored)
                .map_err(|failed| Refusal::internal(failed.to_string()))
        })
        .collect()
}

fn append(
    conversations: crate::secretary::conversations::Conversations<'_>,
    id: &ConversationId,
    entry: &Entry,
) -> Result<(), Refusal> {
    transcript::push(conversations, id, entry)
        .map_err(|failed| Refusal::internal(failed.to_string()))
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
