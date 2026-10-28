//! How a round asks its model.

use serde_json::Value;

use crate::capability::{
    Conversation, Delta, Model, ModelError, Reply, SessionId, SessionModel, Turn,
};
use crate::conversation::round::Sink;
use crate::secretary::compaction::Reading;
use crate::secretary::digest_view::VIEW_BYTES;
use crate::secretary::store::{ConversationId, Kept, Scope, Stored};
use crate::secretary::told::{Asking, context_of};
use crate::secretary::topics::stores::TopicStores;
use crate::secretary::{memory, queue};

/// How a round asks its model: the one difference between a model that remembers and one that
/// does not.
#[derive(Clone, Copy)]
pub enum Ask<'a> {
    /// A model with no session, sent this conversation's window of live turns under what it
    /// remembers ([`crate::secretary::told::context_of`]) each time.
    Window {
        /// The model.
        model: &'a dyn Model,
        /// The memory it is told.
        memories: crate::secretary::memories::Memories<'a>,
        /// The topics it is told are in use.
        topics: TopicStores<'a>,
    },
    /// A session resumed with the turns it has not read, opened when there is none; an open that
    /// fails asks `fallback` once, as [`Ask::Window`] would.
    Session {
        /// The model that holds the session.
        model: &'a dyn SessionModel,
        /// Asked when a session will not open.
        fallback: &'a dyn Model,
        /// The memory it is told.
        memories: crate::secretary::memories::Memories<'a>,
        /// The topics it is told are in use when it opens.
        topics: TopicStores<'a>,
    },
    /// Every stored turn and nothing else, for a job told no memory.
    Transcript {
        /// The model.
        model: &'a dyn Model,
    },
}

/// What one ask sent the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    /// The window under what the model remembers, to a model with no session or one opened now.
    Whole(Conversation),
    /// Only the turns a resumed session has not read.
    Delta(Delta),
}

/// Who answered an ask, and what it was sent: each the fact the round records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answered {
    /// The kept session, resumed with the turns it had not read.
    Resumed(Delta),
    /// A session opened now over the whole window: what was sent, and the session as it was
    /// sent, whose line is owed.
    Opened(Conversation, Kept),
    /// A model holding no session, sent the whole window.
    Stateless {
        /// What it was sent.
        sent: Conversation,
        /// Its name.
        by: String,
    },
}

impl Answered {
    /// What the model was sent.
    #[must_use]
    pub fn sent(self) -> Sent {
        match self {
            Answered::Resumed(delta) => Sent::Delta(delta),
            Answered::Opened(whole, _) | Answered::Stateless { sent: whole, .. } => {
                Sent::Whole(whole)
            }
        }
    }
}

/// What one ask sent and got back.
pub struct Asked {
    /// What the model answered.
    pub reply: Reply,
    /// Who answered, and what it was sent.
    pub answered: Answered,
    /// The memory's version this ask told the model; `None` when it told none or could not read it.
    pub seen: Option<u64>,
}

impl Ask<'_> {
    /// Ask once, over what `stored` holds and `waiting` names.
    ///
    /// # Errors
    /// The model's, except a lost session, which opens a fresh one.
    pub fn ask(
        &self,
        id: &ConversationId,
        system: &str,
        tools: &Value,
        stored: &Stored,
        waiting: &queue::Pending<'_>,
        sink: &Sink<'_>,
    ) -> Result<Asked, ModelError> {
        match *self {
            Ask::Window {
                model,
                memories,
                topics,
            } => {
                let (whole, seen, _) = opening(system, memories, topics, stored);
                let reply = model.turn(&whole, tools)?;
                Ok(Asked {
                    reply,
                    answered: Answered::Stateless {
                        sent: whole,
                        by: model.name(),
                    },
                    seen,
                })
            }
            Ask::Session {
                model,
                fallback,
                memories,
                topics,
            } => sessioned(
                model, fallback, memories, topics, id, system, tools, stored, waiting, sink,
            ),
            Ask::Transcript { model } => {
                let whole = Conversation {
                    system: system.to_owned(),
                    turns: queue::sent(stored, Scope::Transcript)
                        .into_iter()
                        .map(|(_, said)| said.turn.clone())
                        .collect(),
                };
                let reply = model.turn(&whole, tools)?;
                Ok(Asked {
                    reply,
                    answered: Answered::Stateless {
                        sent: whole,
                        by: model.name(),
                    },
                    seen: None,
                })
            }
        }
    }
}

impl Ask<'_> {
    /// Which turns this ask sends a model, the one source of it: the round records it on the
    /// conversation when it differs from the last ask's, and [`queue::held`] reads it back.
    #[must_use]
    pub fn scope(&self) -> Scope {
        match self {
            Ask::Window { .. } | Ask::Session { .. } => Scope::Window,
            Ask::Transcript { .. } => Scope::Transcript,
        }
    }
}

/// What a model with no session, or a session opening, is sent: what it remembers, then this
/// conversation's own last live turns ([`queue::for_model`]); the memory's version it told, and
/// the first turn it sent.
fn opening(
    system: &str,
    memories: crate::secretary::memories::Memories<'_>,
    topics: TopicStores<'_>,
    stored: &Stored,
) -> (Conversation, Option<u64>, usize) {
    let window = queue::for_model(stored);
    let asking = window.first().map(|(from, _)| Asking {
        conversation: &stored.id,
        from: *from,
    });
    let heading = heading(memories, Telling::Opening(topics, asking));
    let from = window.first().map_or(stored.turns.len(), |(from, _)| *from);
    (
        Conversation {
            system: system.to_owned(),
            turns: queue::headed(heading.turn, &window),
        },
        heading.version,
        from,
    )
}

/// Resume the stored session, or open a fresh one when there is none, it was opened on another
/// [`surface_of`], or it is lost; an open that fails gets one stateless turn.
#[allow(clippy::too_many_arguments)]
fn sessioned(
    model: &dyn SessionModel,
    fallback: &dyn Model,
    memories: crate::secretary::memories::Memories<'_>,
    topics: TopicStores<'_>,
    id: &ConversationId,
    system: &str,
    tools: &Value,
    stored: &Stored,
    waiting: &queue::Pending<'_>,
    sink: &Sink<'_>,
) -> Result<Asked, ModelError> {
    let surface = surface_of(system, tools);
    let open = |tag: &str| {
        let fresh = SessionId::minted(&format!("{}/{}{tag}", id.as_str(), stored.turns.len()));
        let (whole, seen, from) = opening(system, memories, topics, stored);
        model
            .open(&fresh, &whole, tools, sink.halt)
            .map(|mut sessioned| {
                sessioned.session.surface = Some(surface.clone());
                Asked {
                    reply: sessioned.reply,
                    // What this open sent is a fact now; a turn appended before the session's
                    // line is written must not move it.
                    answered: Answered::Opened(
                        whole,
                        Kept {
                            session: sessioned.session,
                            from,
                        },
                    ),
                    seen,
                }
            })
    };
    // A session that will not open, whatever the reason but a halt, gets one stateless turn
    // instead, so a round whose opens keep failing still answers every step.
    let opened = |tag: &str| match open(tag) {
        Err(ModelError::Halted) => Err(ModelError::Halted),
        Err(_will_not_open) => {
            let (whole, seen, _) = opening(system, memories, topics, stored);
            let reply = fallback.turn(&whole, tools)?;
            Ok(Asked {
                reply,
                answered: Answered::Stateless {
                    sent: whole,
                    by: fallback.name(),
                },
                seen,
            })
        }
        asked => asked,
    };
    let session = match &stored.session {
        Some(kept) if kept.session.surface.as_deref() == Some(surface.as_str()) => &kept.session,
        Some(_) => return opened("/surface"),
        None => return opened(""),
    };
    // A session resumed is told only what moved in the memory since it last was.
    let told = heading(memories, Telling::Resumed(stored.memory_seen));
    let delta = waiting.delta(told.turn);
    match model.resume(session, &delta, tools, sink.on_delta, sink.halt) {
        Ok(sessioned) => Ok(Asked {
            reply: sessioned.reply,
            answered: Answered::Resumed(delta),
            seen: told.version,
        }),
        Err(ModelError::Lost(_)) => opened("/lost"),
        Err(other) => Err(other),
    }
}

/// The digest of what a session is told only when it opens: the manual and the verb list.
#[must_use]
pub fn surface_of(system: &str, tools: &Value) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(system.as_bytes());
    hasher.update([0]);
    hasher.update(tools.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

/// What an ask tells the model it remembers: the turn heading its batch, if there is anything to
/// tell, and the memory's version it brings the conversation to.
struct Heading {
    turn: Option<Turn>,
    version: Option<u64>,
}

/// Which telling an ask makes.
#[derive(Clone, Copy)]
enum Telling<'a> {
    /// To a model with no session, or a session opening: everything, the topics in use, and the
    /// record less the turns it is sent as turns.
    Opening(TopicStores<'a>, Option<Asking<'a>>),
    /// To a session resumed, last told the memory's version `.0`: only what moved in the facts,
    /// since the record it was told when it opened is in its context.
    Resumed(Option<u64>),
}

/// What `telling` tells: the facts whole, then the record of every conversation, to a model
/// opening; the facts added and forgotten since a resumed session was last told. A memory that
/// will not read is said, never skipped.
fn heading(memories: crate::secretary::memories::Memories<'_>, telling: Telling<'_>) -> Heading {
    let memory = match memories.reads().read() {
        Ok(memory) => memory,
        Err(refusal) => {
            return Heading {
                turn: Some(Turn::Tool {
                    verb: crate::secretary::tool_name::ToolName::Memory,
                    answer: format!("your memory could not be read: {}", refusal.message()),
                    outcome: crate::secretary::outcome::Outcome::Refused,
                    files: Vec::new(),
                }),
                version: None,
            };
        }
    };
    let told = match telling {
        Telling::Opening(topics, asking) => {
            let record = Reading::of(&memories.digests.reads());
            let topics = topics.reading().map_err(crate::refusal::Refusal::from);
            let told = context_of(
                &memory,
                topics.as_ref(),
                record.as_ref(),
                VIEW_BYTES,
                asking,
            );
            (!told.is_empty()).then(|| told.text())
        }
        Telling::Resumed(seen) => {
            let change = memory::change(&memory, seen);
            (!change.is_empty()).then(|| change.told())
        }
    };
    Heading {
        turn: told.map(queue::heading),
        version: Some(memory.version),
    }
}
