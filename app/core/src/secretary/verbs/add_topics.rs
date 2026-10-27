use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::refusal::{Refusal, shown};
use crate::secretary::stamp::Stamp;
use crate::secretary::store::{Said, Stored};
use crate::secretary::topics::reading::Reading;
use crate::secretary::topics::target::Target;
use crate::secretary::topics::targets::{MAX_TOPICS, notes_in};
use crate::secretary::topics::topic::Topic;
use crate::secretary::turn_shape::{TurnShape, shape};
use crate::secretary::{Call, SecretarySurface, queue, taint};
use crate::verb::Verb;

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
enum On {
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
        you drafted last in this round instead. A round holds at most 3 topics; the owner \
        removes and renames them.";

    fn run(call: &Call<'_>, args: AddTopicsArgs) -> Result<Value, Refusal> {
        let asked = parsed(args.topics)?;
        let stored = crate::secretary::read(call.caps.conversations, call.conversation)?;
        let target = answering(&stored, args.on)?;
        let memberships = call.caps.topics.memberships;
        let _held = memberships.locked()?;
        let reading = call.caps.topics.reading()?;
        let existing: BTreeSet<Topic> = crate::secretary::topics::reading::in_use(&reading)
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        let asked: BTreeSet<Topic> = asked.iter().map(|topic| reading.spelling(topic)).collect();
        let new: Vec<&Topic> = asked.iter().filter(|t| !existing.contains(*t)).collect();
        let listed = || listing(&reading);
        if !new.is_empty() && taint::tainted(&stored) {
            return Err(Refusal::bad_request(format!(
                "this round has read words that are not the owner's, so it may only use topics \
                 that exist; {} would be new. The topics are: {}",
                names(new.iter().copied()),
                listed()
            )));
        }
        for (at, topic) in new.iter().enumerate() {
            let earlier = new[..at].iter().copied();
            if let Some(twin) = existing.iter().chain(earlier).find(|e| topic.twins(e)) {
                return Err(Refusal::bad_request(format!(
                    "{} is a near twin of the topic {twin}; name one of them",
                    shown(topic.as_str()),
                )));
            }
        }
        let on = reading.tags(&target);
        let after: BTreeSet<&Topic> = on.iter().chain(&asked).collect();
        if after.len() > MAX_TOPICS {
            return Err(Refusal::bad_request(format!(
                "{target} already has {}, and a round holds at most {MAX_TOPICS} topics",
                names(on.iter())
            )));
        }
        let at = call.caps.clock.now();
        let actor = crate::secretary::writes_as();
        let mut added = Vec::new();
        for topic in &asked {
            if let Some(event) = reading.add(&target, topic, at, &actor) {
                memberships.append(event)?;
                added.push(topic.to_string());
            }
        }
        Ok(json!({
            "on": target.to_string(),
            "topics": after.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "added": added,
            "new": new.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "existing": listed(),
        }))
    }
}

/// The round being answered: the first turn the owner wrote of those the model is answering, else
/// the first person's or act's turn of them, which a round the box opened (a brief, a sweep) has;
/// or, `on` a note, the note it drafted last among them.
fn answering(stored: &Stored, on: On) -> Result<Target, Refusal> {
    let delta = queue::delta(stored);
    match on {
        On::Round => {
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
                .ok_or_else(|| Refusal::conflict("no question is being answered here"))
        }
        On::Note => notes_in(&stored.id, &delta).pop().ok_or_else(|| {
            Refusal::bad_request("this round has drafted no note; leave `on` out for the round")
        }),
    }
}

/// 1 to [`MAX_TOPICS`] topics, each spelled as [`Topic::new`] reads it; every refusal is named.
fn parsed(texts: Vec<String>) -> Result<BTreeSet<Topic>, Refusal> {
    if !(1..=MAX_TOPICS).contains(&texts.len()) {
        return Err(Refusal::bad_request(format!(
            "name 1 to {MAX_TOPICS} topics, not {}",
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

/// The topics in use, each with how many rounds it is on, the most first.
fn listing(reading: &Reading) -> String {
    let in_use = crate::secretary::topics::reading::in_use(reading);
    if in_use.is_empty() {
        return "none yet".to_owned();
    }
    in_use
        .iter()
        .map(|(topic, rounds)| format!("{topic} ({rounds})"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn names<'t>(topics: impl Iterator<Item = &'t Topic>) -> String {
    topics.map(Topic::as_str).collect::<Vec<_>>().join(", ")
}
