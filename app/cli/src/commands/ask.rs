//! `suzatary secretary ask <conversation|new> <text>`: the operator's question, sealed as a tab
//! seals one and signed by the root key, and the round's answer as its stream sends it.

use std::ops::ControlFlow;

use std::str::FromStr;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use suzatary_store::replica::held::Node as _;

use anyhow::{Context, bail};
use suzatary_api::wire;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::ConversationState;

use super::call::Operator;
use suzatary_lines::secretary::held_replica::Sealing;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conversation {
    /// A topic begun for it on the box's own store.
    New,
    Named(ConversationId),
}

impl FromStr for Conversation {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text {
            "new" => Ok(Conversation::New),
            named => ConversationId::new(named)
                .map(Conversation::Named)
                .map_err(|failed| failed.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct Asked {
    pub conversation: ConversationId,
    /// The question's line, which the root key signs.
    pub line: prodrome_data::event::Hash,
    /// The secretary's words in the round, as the stored lines hold them.
    pub answer: String,
}

/// How long a round may take before the question is left to the box.
const ROUND: Duration = Duration::from_mins(15);

/// Say `text` into conversation `id` as `operator`, then wait for the round to end; the stream is
/// followed before the question is said, so no frame of its round is missed.
pub fn ask(operator: &Operator, id: &ConversationId, text: &str) -> anyhow::Result<Asked> {
    let id = id.clone();
    let frames = followed(operator, &id);
    // The stream's first frame is the conversation's state before the question.
    frames
        .recv_timeout(Duration::from_secs(30))
        .context("the conversation's stream sent nothing")?;
    let mut replica = operator.lines(&id)?;
    let at = operator.now()?;
    let (owned, name, _) = replica
        .seal(
            Sealing::Say {
                conversation: id.clone(),
                question: suzatary_lines::secretary::held_replica::Question {
                    text: text.to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                    seat: suzatary_signatures::signatures::Seat::Operator,
                },
            },
            at,
            operator.actor(),
            &suzatary_io_local::texts::SystemEntropy,
        )
        .map_err(|e| anyhow::anyhow!("the question was not sealed: {e}"))?;
    let own = owned
        .iter()
        .map(|own| &own.node)
        .find(|own| own.name() == &name)
        .context("the sealed line")?;
    operator.hand_over(own)?;
    let deadline = Instant::now() + ROUND;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let frame = frames.recv_timeout(left).map_err(|failed| match failed {
            mpsc::RecvTimeoutError::Timeout => anyhow::anyhow!(
                "the round in {id} did not end within {} minutes; it goes on on the box",
                ROUND.as_secs() / 60
            ),
            mpsc::RecvTimeoutError::Disconnected => {
                anyhow::anyhow!("the stream of {id} closed before its round ended")
            }
        })?;
        match frame {
            // An idle frame may describe the conversation before this question landed, or after
            // an earlier round closed with it still owed: the lines say whether it was answered.
            wire::Frame::State {
                state: ConversationState::Idle,
            } => {
                if let Some(answer) = answered(operator, &id, &name)? {
                    return Ok(Asked {
                        conversation: id,
                        line: name,
                        answer,
                    });
                }
            }
            wire::Frame::State {
                state:
                    ConversationState::Queued
                    | ConversationState::Thinking
                    | ConversationState::Answering,
            }
            | wire::Frame::Delta { .. }
            | wire::Frame::Turn
            | wire::Frame::Working { .. }
            | wire::Frame::Block { .. } => {}
        }
    }
}

/// The secretary's words in reply to `line` once the claims read it settled: the model's turns of
/// the round that covered it, read off the lines (position is not the reply: an earlier round's can
/// stand after it); None while the line is not held yet or still owed, and nothing for a stopped
/// question.
fn answered(
    operator: &Operator,
    id: &ConversationId,
    line: &prodrome_data::event::Hash,
) -> anyhow::Result<Option<String>> {
    let mut replica = operator.lines(id)?;
    let Some(read) = replica.lines.transcript(id) else {
        bail!("the lines hold no conversation {id}");
    };
    let read = read.map_err(|failed| anyhow::anyhow!("the lines do not read: {failed}"))?;
    let Some(asked) = read.turn_lines().iter().position(|name| *name == line) else {
        return Ok(None);
    };
    if read.owed().iter().any(|(turn, _)| *turn == asked) {
        return Ok(None);
    }
    let turns = &read.stored().turns;
    Ok(Some(
        read.replies_to(asked)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|at| match &turns[at].turn {
                Turn::Model { text, .. } if !text.is_empty() => Some(text.as_str()),
                Turn::Model { .. }
                | Turn::Person { .. }
                | Turn::Tool { .. }
                | Turn::Act { .. }
                | Turn::Erased { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    ))
}

/// The frames of `id`'s stream, read on a thread of their own until the receiver is dropped.
fn followed(operator: &Operator, id: &ConversationId) -> mpsc::Receiver<wire::Frame> {
    let (send, frames) = mpsc::channel();
    let operator = operator.clone();
    let target = format!("/api/secretary/conversations/{id}/stream");
    std::thread::spawn(move || {
        let followed = operator.stream(&target, &mut |line| {
            let Some(data) = line.strip_prefix("data:") else {
                return ControlFlow::Continue(());
            };
            match serde_json::from_str::<wire::Frame>(data.trim()) {
                Ok(frame) => match send.send(frame) {
                    Ok(()) => ControlFlow::Continue(()),
                    Err(_) => ControlFlow::Break(()),
                },
                Err(failed) => {
                    tracing::warn!("ask: a frame of {target} did not read: {failed}");
                    ControlFlow::Continue(())
                }
            }
        });
        if let Err(failed) = followed {
            tracing::warn!("ask: {failed:#}");
        }
    });
    frames
}
