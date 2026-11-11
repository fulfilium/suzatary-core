//! The model's laws ([`crate::secretary::model::Model`], the brain `choose_brain` picks), read through
//! a rig that scripts the far end.

use std::fmt;

use crate::conformance::{Broken, hold};
use crate::secretary::model::{Conversation, Model, Reply, Turn};

/// A law of a model, in the order the suite asks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Law {
    /// It has a name, and the same one every time: a conversation stores it.
    Named,
    /// What it answers is what its far end said, read by `Reply::parse`: the interpreter adds
    /// no word and no call, and drops none.
    Faithful,
    /// Its far end hears the whole conversation: the rules, then every turn's words in order.
    Whole,
    /// A far end that cannot be reached is a failure, never an answer.
    Fails,
}

impl fmt::Display for Law {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Law::Named => "a model's name is fixed",
            Law::Faithful => "a model answers what its far end said",
            Law::Whole => "a model's far end hears the whole conversation",
            Law::Fails => "an unreachable model fails",
        })
    }
}

/// An implementation with its far end scripted.
pub trait Rig {
    /// The model under test.
    type Model: Model;

    /// The model, its far end answering `text` to every turn.
    fn answering(&self, text: &str) -> Self::Model;

    /// The model, its far end unreachable.
    fn unreachable(&self) -> Self::Model;

    /// Everything `model`'s far end has heard so far, as text.
    fn heard(&self, model: &Self::Model) -> String;
}

/// One conversation, and what the far end answers it.
#[derive(Debug, Clone)]
pub struct Case {
    /// The conversation asked.
    pub conversation: Conversation,
    /// The far end's text: an envelope, or prose.
    pub answer: String,
}

/// Every law over every case, through `rig`, with `tools` the verb list each turn names.
///
/// # Errors
/// The first [`Broken`] law.
pub fn check<R: Rig>(
    rig: &R,
    tools: &serde_json::Value,
    cases: &[Case],
) -> Result<(), Broken<Law>> {
    for Case {
        conversation,
        answer,
    } in cases
    {
        let model = rig.answering(answer);
        let name = model.name();
        hold(Law::Named, !name.is_empty() && name == model.name(), || {
            format!("named {name:?}, then {:?}", model.name())
        })?;

        let reply = model
            .turn(conversation, tools)
            .map_err(|failed| Broken::of(Law::Faithful, failed.to_string()))?;
        let said = Reply::parse(answer);
        hold(
            Law::Faithful,
            reply.say == said.say && reply.calls == said.calls,
            || format!("the far end said {answer:?}, and the model answered {reply:?}"),
        )?;

        let heard = rig.heard(&model);
        let words = std::iter::once(conversation.system.as_str())
            .chain(conversation.turns.iter().map(words))
            .filter(|words| !words.is_empty());
        let mut from = 0;
        for said in words {
            let Some(at) = heard.get(from..).and_then(|rest| rest.find(said)) else {
                return Err(Broken::of(
                    Law::Whole,
                    format!("the far end never heard {said:?} (after byte {from})"),
                ));
            };
            from += at + said.len();
        }

        let failing = rig.unreachable();
        let failed = failing.turn(conversation, tools);
        hold(Law::Fails, failed.is_err(), || {
            format!("an unreachable far end answered {failed:?}")
        })?;
    }
    Ok(())
}

/// A turn's own words: what was typed, said, answered or done.
fn words(turn: &Turn) -> &str {
    match turn {
        Turn::Person { text, .. } | Turn::Model { text, .. } | Turn::Act { text, .. } => text,
        Turn::Tool { answer, .. } => answer,
        // A turn whose words are gone says none.
        Turn::Erased { .. } => "",
    }
}
