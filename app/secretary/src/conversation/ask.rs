//! How a round asks its model: one fresh call per step, a function of the objects it reads.

use serde_json::Value;
use suzatary_digest::secretary::memories::Memories;

use crate::conversation::round::Sink;
use suzatary_conversations::secretary::model::{
    Conversation, Earlier, Model, ModelError, Reply, ToolCall, Turn,
};
use suzatary_conversations::secretary::queue;
use suzatary_conversations::secretary::store::{Said, Scope, Stored};
use suzatary_conversations::secretary::taint::{self, Taint};
use suzatary_digest::secretary::compaction::Reading;
use suzatary_digest::secretary::digest_view::VIEW_BYTES;
use suzatary_digest::secretary::told::{Asking, context_of};
use suzatary_lines::secretary::transcript::Transcript;
use suzatary_topics::secretary::topics::stores::TopicStores;

/// How a round asks its model.
#[derive(Clone, Copy)]
pub enum Ask<'a> {
    /// What the model remembers ([`context_of`]: the facts, the record less this conversation's
    /// window, the topics in use), then the window of this conversation's own live turns
    /// ([`queue::window`]), which ends in the round being answered.
    Digest {
        model: &'a dyn Model,
        memories: &'a Memories<'a>,
        topics: &'a TopicStores<'a>,
    },
    /// Every stored turn and nothing else, for a job told no memory.
    Transcript {
        /// The model.
        model: &'a dyn Model,
    },
}

/// What one ask was sent, and what came back.
pub struct Asked {
    pub reply: Reply,
    pub sent: Conversation,
}

/// One call, and what it carries.
pub struct Call {
    pub call: Conversation,
    /// The join of the carries of every turn the call holds ([`taint::sent`]), the earlier
    /// rounds' long answers sent as their markers among them.
    pub carries: Taint,
}

impl Ask<'_> {
    /// Send `call`, built by [`Ask::carrying`]: the reply streams to `sink`, and its halt ends
    /// the call. What it carries is the caller's to join before it sends, so a call that fails
    /// still carried it.
    pub fn send(
        &self,
        call: Conversation,
        tools: &Value,
        sink: &Sink<'_>,
    ) -> Result<Asked, ModelError> {
        let reply = self
            .model()
            .streamed(&call, tools, sink.on_delta, sink.halt)?;
        Ok(Asked { reply, sent: call })
    }

    /// What one ask over `read` sends: a function of the objects it reads, so two replicas
    /// holding the same objects send the same call. A turn no round has claimed yet is not
    /// sent: the round that claims it answers it.
    #[must_use]
    pub fn call(&self, system: &str, read: &Transcript) -> Conversation {
        self.carrying(system, read).call
    }

    /// [`Ask::call`] and what it carries, from the one choice of turns.
    #[must_use]
    pub fn carrying(&self, system: &str, read: &Transcript) -> Call {
        match *self {
            Ask::Digest {
                memories, topics, ..
            } => digest(system, *memories, topics, read),
            Ask::Transcript { .. } => {
                let stored = read.stored();
                Call {
                    call: Conversation::bare(
                        system,
                        stored.turns.iter().map(|said| said.turn.clone()).collect(),
                    ),
                    carries: taint::sent(stored, 0..stored.turns.len()),
                }
            }
        }
    }

    fn model(&self) -> &dyn Model {
        match *self {
            Ask::Digest { model, .. } | Ask::Transcript { model } => model,
        }
    }

    /// Which turns this ask sends a model, the one source of it: the round records it on the
    /// conversation when it differs from the last ask's, and [`queue::held`] reads it back.
    #[must_use]
    pub fn scope(&self) -> Scope {
        match self {
            Ask::Digest { .. } => Scope::Window,
            Ask::Transcript { .. } => Scope::Transcript,
        }
    }
}

/// [`Ask::Digest`]'s call: the context, then the window. A store that will not read is said in
/// the context where its part would stand, and the other parts are told as they read.
fn digest(
    system: &str,
    memories: Memories<'_>,
    topics: &TopicStores<'_>,
    read: &Transcript,
) -> Call {
    let stored = read.stored();
    let window = queue::window(stored, &read.queued());
    let carries = taint::sent(stored, window.all().map(|(index, _)| *index));
    let sent: std::collections::BTreeSet<prodrome_data::event::Hash> = window
        .first()
        .and_then(|first| read.line_of(first))
        .map(|line| {
            read.lines()[line..]
                .iter()
                .map(|written| written.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let asking = Some(Asking { sent: &sent });
    let memory = memories.reads().read();
    let record = Reading::of(&memories.digests.reads());
    let topics = topics
        .reading()
        .map_err(suzatary_base::refusal::Refusal::from);
    let turns = |turns: &[(usize, &Said)]| -> Vec<Turn> {
        turns.iter().map(|(_, said)| said.turn.clone()).collect()
    };
    Call {
        call: Conversation {
            system: system.to_owned(),
            context: context_of(
                memory.as_ref(),
                topics.as_ref(),
                record.as_ref(),
                VIEW_BYTES,
                asking,
            )
            .context(),
            earlier: earlier(stored, &window.earlier),
            turns: turns(&window.round),
        },
        carries,
    }
}

/// The earlier rounds as a call sends them: each verb's answer beside the call that has it
/// again, and a long one by its marker. Calls and answers pair by the turns' shapes, so an
/// erased call or answer still takes its place: a model's turn queues its calls in the order
/// made (an erased one by its verbs alone, whose arguments are gone), each answer of the same
/// verb takes the first, and an answer the owner's pointing loaded (`reference`) the call its
/// reference maps to ([`crate::secretary::again`]), in the order the round's opening named them.
fn earlier(stored: &Stored, turns: &[(usize, &Said)]) -> Vec<Earlier> {
    use std::collections::VecDeque;
    use suzatary_conversations::secretary::tool_name::ToolName;
    use suzatary_conversations::secretary::turn_shape::{TurnShape, shape};
    let mut asked: VecDeque<(ToolName, Option<ToolCall>)> = VecDeque::new();
    let mut pointed: VecDeque<suzatary_base::reference::Reference> = VecDeque::new();
    turns
        .iter()
        .map(|&(index, said)| match shape(&said.turn) {
            TurnShape::Model { calls, .. } => {
                asked = match &said.turn {
                    Turn::Model { calls, .. } => calls
                        .iter()
                        .map(|call| (call.verb.clone(), Some(call.clone())))
                        .collect(),
                    _ => calls.into_iter().map(|verb| (verb, None)).collect(),
                };
                Earlier::Said(said.turn.clone())
            }
            TurnShape::Person { references, .. } | TurnShape::Act { references } => {
                pointed = references.into_iter().collect();
                Earlier::Said(said.turn.clone())
            }
            TurnShape::Tool { verb, .. } => {
                let call = if verb == ToolName::Reference {
                    pointed
                        .pop_front()
                        .and_then(|reference| crate::secretary::again(&reference))
                } else {
                    match asked.front() {
                        Some((first, _)) if *first == verb => {
                            asked.pop_front().and_then(|(_, call)| call)
                        }
                        Some(_) | None => None,
                    }
                };
                match &said.turn {
                    _ if queue::long_answer(stored, index) => Earlier::Long {
                        verb,
                        call,
                        bytes: match &said.turn {
                            Turn::Tool { answer, .. } => Some(answer.len()),
                            _ => None,
                        },
                    },
                    Turn::Tool { .. } => Earlier::Answered {
                        call,
                        turn: said.turn.clone(),
                    },
                    _ => Earlier::Said(said.turn.clone()),
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_conversations::secretary::outcome::Outcome;
    use suzatary_conversations::secretary::store::{Entry, Kind};
    use suzatary_conversations::secretary::turn_shape::shape;

    fn at() -> suzatary_base::capability::Datetime {
        suzatary_base::capability::Datetime::new(2026, 11, 25, 9, 0, 0, 0).expect("an instant")
    }

    fn stored(turns: Vec<Turn>) -> Stored {
        let mut entries = vec![Entry::Opened {
            at: at(),
            title: suzatary_memory::secretary::words::Words::kept("t".to_owned()),
            model: "m".to_owned(),
            kind: Kind::Topic,
        }];
        entries.extend(turns.into_iter().map(|turn| Entry::Said {
            at: at(),
            by: matches!(turn, Turn::Person { .. }).then(|| "ada".into()),
            turn,
        }));
        Stored::of(
            &suzatary_base::conversation_id::ConversationId::new("default").expect("a slug"),
            &entries,
        )
        .expect("a conversation")
    }

    fn show(id: &str) -> ToolCall {
        ToolCall {
            verb: "show".into(),
            args: [("id".to_owned(), Value::from(id))].into(),
        }
    }

    fn answer(verb: &str, answer: &str) -> Turn {
        Turn::Tool {
            verb: verb.into(),
            answer: answer.to_owned(),
            outcome: Outcome::Answered,
            files: Vec::new(),
        }
    }

    fn erased(turn: &Turn) -> Turn {
        let shape = shape(turn);
        let words = vec![suzatary_memory::secretary::words::Gone::Missing.into(); shape.slots()];
        Turn::Erased { shape, words }
    }

    fn sent(stored: &Stored) -> Vec<Earlier> {
        let all: Vec<(usize, &Said)> = stored.turns.iter().enumerate().collect();
        earlier(stored, &all)
    }

    /// E4: an answer takes the call it answered by the turns' shapes, so an erased answer still
    /// takes its call's place, and an erased call's answer names no call.
    #[test]
    fn an_answer_pairs_with_its_call_around_an_erasure() {
        let calling = Turn::Model {
            text: String::new(),
            calls: vec![show("a"), show("b")],
        };
        let round = stored(vec![
            calling.clone(),
            erased(&answer("show", "{\"a\": 1}")),
            answer("show", "{\"b\": 2}"),
            erased(&calling),
            answer("show", "{\"c\": 3}"),
        ]);
        let sent = sent(&round);
        assert!(
            matches!(&sent[2], Earlier::Answered { call: Some(call), .. } if *call == show("b"))
        );
        assert!(matches!(&sent[4], Earlier::Answered { call: None, .. }));
    }

    /// E1: a long answer the owner's pointing loaded is marked with the call its reference maps
    /// to, and with the answer's bytes.
    #[test]
    fn a_pointed_long_answer_names_the_call_that_reads_it_again() {
        let pointed: suzatary_base::reference::Reference =
            "todo:life/rent".parse().expect("a reference");
        let long = format!("todo:life/rent\n\n{}", "w".repeat(queue::LONG_ANSWER * 2));
        let round = stored(vec![
            Turn::Person {
                text: "this one".to_owned(),
                references: vec![pointed],
                attachments: Vec::new(),
            },
            answer("reference", &long),
        ]);
        match &sent(&round)[1] {
            Earlier::Long {
                verb,
                call: Some(call),
                bytes,
            } => {
                assert_eq!(verb.as_str(), "reference");
                assert_eq!(call.verb.as_str(), "todo_show");
                assert_eq!(call.args.get("todo_id"), Some(&Value::from("rent")));
                assert_eq!(*bytes, Some(long.len()));
            }
            other => panic!("a marker naming todo_show: {other:?}"),
        }
    }
}
