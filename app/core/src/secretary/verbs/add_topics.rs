use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::secretary::stamp::Stamp;
use crate::secretary::store::{Said, Stored};
use crate::secretary::targets::{MAX_TOPICS, notes_in};
use crate::secretary::topics::revision::{Revision, held, listing, plan};
use crate::secretary::topics::topic::Topic;
use crate::secretary::turn_shape::{TurnShape, shape};
use crate::secretary::{Call, SecretarySurface, queue, taint};
use crate::verb::Verb;
use suzatary_base::refusal::Refusal;
use suzatary_base::topic_target::Target;

/// Put topics on the round being answered, or on the note it drafted last.
pub struct AddTopics;

#[derive(Deserialize, JsonSchema)]
pub struct AddTopicsArgs {
    topics: Vec<String>,
    #[serde(default)]
    on: On,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(super) enum On {
    #[default]
    Round,
    Note,
}

impl Verb for AddTopics {
    type Surface = SecretarySurface;
    type Args = AddTopicsArgs;
    type Answer = Value;
    const NAME: &'static str = "add_topics";
    const DESCRIPTION: &'static str = "Give the round you are answering 1 to 3 topics, so the owner can find it \
        again by subject. A topic is 1 to 3 lowercase words joined by hyphens, at most 24 \
        characters (`garden`, `tax-return`). USE A TOPIC THAT EXISTS: the session opening lists \
        them, and this answer lists them again. Make a new one only for a subject that will come \
        back, never one per conversation or per email; a round that has read anything but the \
        owner's own words may only use topics that exist. `on: \"note\"` puts them on the note \
        you drafted last in this round instead. A round holds at most 3 topics. Yours are \
        suggestions until the owner keeps them; revise them with revise_topics.";

    fn run(call: &Call<'_>, args: AddTopicsArgs) -> Result<Value, Refusal> {
        let add = parsed(args.topics, 1)?;
        revise(
            call,
            Aim::Answering(args.on),
            &Revision {
                add,
                ..Revision::default()
            },
        )
    }
}

/// Where a revision lands: the round being answered or its note, or a target the model names.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(super) enum Aim {
    Answering(On),
    /// `round:<conversation>/<turn>` or `note:<conversation>/<turn>`.
    At(String),
}

/// Plans `revision` on its target under the store's lock, writes it, and answers the target's
/// topics after.
pub(super) fn revise(call: &Call<'_>, aim: Aim, revision: &Revision) -> Result<Value, Refusal> {
    let stored = crate::secretary::conversations::read(call.caps.conversations, call.conversation)?;
    let target = match aim {
        Aim::Answering(on) => answering(&stored, on)?,
        Aim::At(text) => text
            .parse::<Target>()
            .map_err(|e| Refusal::bad_request(e.to_string()))?,
    };
    let memberships = call.caps.topics.memberships;
    let _held = memberships.locked()?;
    let reading = call.caps.topics.reading()?;
    let planned = plan(
        &reading,
        &target,
        revision,
        taint::held(&stored),
        call.caps.clock.now(),
    )?;
    for event in planned.events {
        memberships.append(event)?;
    }
    let after = call.caps.topics.reading()?;
    let mut answer = held(&after, &target);
    answer["new"] = json!(
        planned
            .new
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    answer["existing"] = json!(listing(&after));
    Ok(answer)
}

/// The round being answered: the first turn the owner wrote of those the model is answering, else
/// the first person's or act's turn of them, which a round the box opened (a brief, a sweep) has;
/// or, `on` a note, the note it drafted last among them.
fn answering(stored: &Stored, on: On) -> Result<Target, Refusal> {
    match on {
        On::Round => round_answered(stored)
            .ok_or_else(|| Refusal::conflict("no question is being answered here")),
        On::Note => notes_in(&stored.id, &queue::delta(stored))
            .pop()
            .ok_or_else(|| {
                Refusal::bad_request("this round has drafted no note; leave `on` out for the round")
            }),
    }
}

/// The round being answered in `stored`, named by its own conversation: the first turn the owner
/// wrote of those the model is answering, else the first person's or act's turn of them. `None`
/// when no question is being answered.
pub(super) fn round_answered(stored: &Stored) -> Option<Target> {
    let delta = queue::delta(stored);
    let opens = |said: &Said| {
        matches!(
            shape(&said.turn),
            TurnShape::Person { .. } | TurnShape::Act { .. }
        )
    };
    delta
        .iter()
        .find(|(_, said)| opens(said) && Stamp::owners(said.by.as_ref()))
        .or_else(|| delta.iter().find(|(_, said)| opens(said)))
        .map(|(turn, _)| Target::Round {
            conversation: stored.id.clone(),
            turn: *turn,
        })
}

/// `least` to [`MAX_TOPICS`] topics, each spelled as [`Topic::new`] reads it; every refusal is
/// named.
pub(super) fn parsed(texts: Vec<String>, least: usize) -> Result<BTreeSet<Topic>, Refusal> {
    if !(least..=MAX_TOPICS).contains(&texts.len()) {
        return Err(Refusal::bad_request(format!(
            "name {least} to {MAX_TOPICS} topics, not {}",
            texts.len()
        )));
    }
    let mut topics = BTreeSet::new();
    let mut refused = Vec::new();
    for text in texts {
        match Topic::new(text) {
            Ok(topic) => {
                topics.insert(topic);
            }
            Err(refusal) => refused.push(refusal.to_string()),
        }
    }
    if refused.is_empty() {
        Ok(topics)
    } else {
        Err(Refusal::bad_request(refused.join("; ")))
    }
}
