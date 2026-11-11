//! `suzatary secretary ask <conversation|new> <text>`: the operator's question, said through the
//! say route as a device says one, and the round's answer as its stream sends it.

use std::ops::ControlFlow;
use std::str::FromStr;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use suzatary_base::conversation_id::ConversationId;
use suzatary_core::secretary::model::Turn;
use suzatary_core::secretary::stamp::Stamp;
use suzatary_core::secretary::store::ConversationState;
use suzatary_core::wire;

use super::call::Operator;
use super::serve::route::Method;

/// The conversation a question goes into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conversation {
    /// A topic opened for it, as the topic bar's new topic opens one.
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

/// A question asked and answered.
#[derive(Debug)]
pub struct Asked {
    pub conversation: ConversationId,
    /// The name the box keeps the question's record under, which names the root key.
    pub request: prodrome_core::event::Hash,
    /// The secretary's words in the round, as the stored lines hold them.
    pub answer: String,
}

/// How long a round may take before the question is left to the box.
const ROUND: Duration = Duration::from_mins(15);

/// Say `text` into `conversation` as `operator`, then wait for the round to end.
///
/// The stream is followed before the question is said, so no frame of its round is missed.
///
/// # Errors
/// A route's refusal, a stream that closes early, or a round longer than 15 minutes.
pub fn ask(operator: &Operator, conversation: &Conversation, text: &str) -> anyhow::Result<Asked> {
    let id = match conversation {
        Conversation::New => opened(operator)?,
        Conversation::Named(id) => id.clone(),
    };
    let frames = followed(operator, &id);
    // The stream's first frame is the conversation's state before the question.
    frames
        .recv_timeout(Duration::from_secs(30))
        .context("the conversation's stream sent nothing")?;
    let saying = wire::Saying {
        say: text.to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
        after: None,
    };
    let said = operator.request(
        Method::Post,
        &format!("/api/secretary/conversations/{id}/say"),
        &serde_json::to_vec(&saying)?,
    )?;
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
            wire::Frame::State {
                state: ConversationState::Idle,
            } => break,
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
    let answer = answered(operator, &id)?;
    Ok(Asked {
        conversation: id,
        request: said.request,
        answer,
    })
}

/// The secretary's words since the conversation's last line of the owner's, as a replica of the
/// lines reads them: the first exchange of a session streams no delta, so the stream cannot say.
fn answered(operator: &Operator, id: &ConversationId) -> anyhow::Result<String> {
    let mut replica = operator.lines(id)?;
    let Some(read) = replica.lines.conversation(id) else {
        bail!("the lines hold no conversation {id}");
    };
    let stored = read.map_err(|failed| anyhow::anyhow!("the lines do not read: {failed}"))?;
    let since = stored
        .turns
        .iter()
        .rposition(|said| Stamp::owners(said.by.as_ref()))
        .map_or(0, |at| at + 1);
    Ok(stored.turns[since..]
        .iter()
        .filter_map(|said| match &said.turn {
            Turn::Model { text, .. } if !text.is_empty() => Some(text.as_str()),
            Turn::Model { .. }
            | Turn::Person { .. }
            | Turn::Tool { .. }
            | Turn::Act { .. }
            | Turn::Erased { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// A new topic, its title left to the title job.
fn opened(operator: &Operator) -> anyhow::Result<ConversationId> {
    #[derive(serde::Deserialize)]
    struct Opened {
        id: String,
    }
    let answer = operator.request(
        Method::Post,
        "/api/secretary/conversations",
        &serde_json::to_vec(&wire::Titling {
            title: String::new(),
        })?,
    )?;
    let opened: Opened =
        serde_json::from_slice(&answer.body).context("the opened conversation's answer")?;
    Ok(ConversationId::new(opened.id)?)
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
