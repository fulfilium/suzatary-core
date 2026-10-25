//! The one-time pass over the rounds no topic is on: a model is shown each such round in the
//! owner's words alone, in batches, and gives it 0 to [`MAX_TOPICS`] topics from the topics in use
//! and the new ones it names. A new topic stands only when it is on [`RECURS`] rounds or more, so a
//! subject that came up once makes no topic. Its notes get the round's topics. What it proposes is
//! a [`Plan`], which the run applies as the dry run wrote it, asking no model.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use serde::Deserialize;

use crate::capability::{ModelError, Turn};
use crate::for_model::ForModel;
use crate::secretary::store::Stored;
use crate::secretary::taint::{self, Taint};
use crate::secretary::topics::plan::{NotAPlan, Plan, Planned};
use crate::secretary::topics::reading::Reading;
use crate::secretary::topics::target::Target;
use crate::secretary::topics::targets::{MAX_TOPICS, rounds};
use crate::secretary::topics::topic::Topic;

/// The fewest rounds a new topic must be on to be made.
pub const RECURS: usize = 3;

/// The most bytes of the owner's words one batch shows the model.
pub const BATCH_BYTES: usize = 24_000;

/// What the model is told, as its whole system prompt.
pub const PROMPT: &str = "You sort the owner's past questions to their secretary by subject, so \
    they can find them again. Each round below is led by its name and holds only the owner's own \
    words. Give each round 0 to 3 topics. USE A TOPIC FROM THE LIST wherever one fits. Make a new \
    topic only for a subject that comes up in several rounds, never one for a single round, and \
    name every new one under \"new\"; keep new topics few. A topic is 1 to 3 lowercase words joined \
    by hyphens, at most 24 characters, such as garden or tax-return. Leave out a round no topic \
    fits. Answer with JSON alone, in this shape: {\"new\": [\"topic\"], \"rounds\": \
    {\"round:<conversation>/<turn>\": [\"topic\"]}}";

/// One round no topic is on, in the owner's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Untopiced {
    pub target: Target,
    pub notes: Vec<Target>,
    pub words: String,
}

/// What the pass found and plans.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Backfill {
    /// Each round of an answered batch, with the topics it gets, possibly none.
    pub rounds: Vec<(Target, BTreeSet<Topic>)>,
    /// Each batch the model did not answer as asked: its rounds, left out of the plan, and why.
    pub failed: Vec<(Vec<Target>, String)>,
    /// How many rounds no topic is on hold none of the owner's words, and were not sent.
    pub unsent: usize,
    /// Each new topic made, with how many rounds it is on.
    pub new: Vec<(Topic, usize)>,
    /// Each new topic the model named that is on too few rounds of the answered batches, or is a
    /// near twin of a topic in use or of a new one made before it, with how many rounds it was
    /// given; no round gets it.
    pub dropped: Vec<(Topic, usize)>,
    /// What the model answered that was not used, in the box's words.
    pub problems: Vec<String>,
    /// The rounds that get a topic, as the run applies them.
    pub plan: Plan,
}

/// The rounds of `conversations` that no topic is on in `reading`, and how many of them hold none
/// of the owner's words.
#[must_use]
pub fn untopiced(conversations: &[Stored], reading: &Reading) -> (Vec<Untopiced>, usize) {
    let mut found = Vec::new();
    let mut unsent = 0;
    for stored in conversations {
        for round in rounds(stored) {
            let target = round.target();
            if !reading.topics(&target).is_empty() {
                continue;
            }
            let words: Vec<&str> = round
                .turns()
                .filter(|(_, said)| taint::turn(said) == Taint::Clean)
                .filter_map(|(_, said)| match &said.turn {
                    Turn::Person { text, .. } if !text.trim().is_empty() => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            if words.is_empty() {
                unsent += 1;
                continue;
            }
            found.push(Untopiced {
                notes: round
                    .notes()
                    .into_iter()
                    .filter(|note| reading.topics(note).is_empty())
                    .collect(),
                target,
                words: words.join("\n"),
            });
        }
    }
    (found, unsent)
}

/// The rounds in batches of at most [`BATCH_BYTES`] of words each, a longer round alone.
#[must_use]
pub fn batches(rounds: &[Untopiced]) -> Vec<&[Untopiced]> {
    let mut batches = Vec::new();
    let (mut start, mut bytes) = (0, 0);
    for (at, round) in rounds.iter().enumerate() {
        if at > start && bytes + round.words.len() > BATCH_BYTES {
            batches.push(&rounds[start..at]);
            (start, bytes) = (at, 0);
        }
        bytes += round.words.len();
    }
    if start < rounds.len() {
        batches.push(&rounds[start..]);
    }
    batches
}

/// What one batch shows the model: the topics it may use, then each round by name.
#[must_use]
pub fn ask(topics: &BTreeSet<Topic>, batch: &[Untopiced]) -> ForModel {
    let listed = if topics.is_empty() {
        "none yet".to_owned()
    } else {
        topics
            .iter()
            .map(Topic::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = format!("THE TOPICS: {listed}\n");
    for round in batch {
        let _ = write!(text, "\n{}\n{}\n", round.target, round.words);
    }
    ForModel::of(&text)
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    new: Vec<String>,
    #[serde(default)]
    rounds: BTreeMap<String, Vec<String>>,
}

/// The pass over `conversations` and the topics `reading` holds, each batch asked of `model`
/// under [`PROMPT`]. A batch the model does not answer, or answers with anything but the JSON
/// asked for, is left out of the plan and named in [`Backfill::failed`].
///
/// # Errors
/// A plan [`Plan::new`] refuses, which the rounds of distinct turns never make.
pub fn plan(
    conversations: &[Stored],
    reading: &Reading,
    model: &mut dyn FnMut(&ForModel, ForModel) -> Result<String, ModelError>,
) -> Result<Backfill, NotAPlan> {
    let (untopiced, unsent) = untopiced(conversations, reading);
    let existing: BTreeSet<Topic> = reading.in_use().into_iter().map(|(t, _)| t).collect();
    let mut sorting = Sorting::default();
    let mut failed = Vec::new();
    let mut answered: Vec<&Untopiced> = Vec::new();
    let system = ForModel::of(PROMPT);
    for batch in batches(&untopiced) {
        let offered: BTreeSet<Topic> = existing.union(&sorting.named).cloned().collect();
        let answer = model(&system, ask(&offered, batch))
            .map_err(|failed| failed.to_string())
            .and_then(|said| read(&said));
        match answer {
            Ok(answer) => {
                sorting.take(reading, batch, &offered, answer);
                answered.extend(batch);
            }
            Err(why) => failed.push((batch.iter().map(|r| r.target.clone()).collect(), why)),
        }
    }
    let Sorting {
        named,
        given,
        problems,
    } = sorting;
    let mut count: BTreeMap<&Topic, usize> = BTreeMap::new();
    for topics in given.values() {
        for topic in topics {
            *count.entry(topic).or_default() += 1;
        }
    }
    let mut candidates: Vec<(&Topic, usize)> = named
        .iter()
        .filter(|topic| !existing.contains(*topic))
        .map(|topic| (topic, count.get(topic).copied().unwrap_or(0)))
        .collect();
    candidates.sort_by(|(one, many), (two, more)| more.cmp(many).then_with(|| one.cmp(two)));
    let mut new: Vec<(Topic, usize)> = Vec::new();
    let mut dropped: Vec<(Topic, usize)> = Vec::new();
    for (topic, rounds) in candidates {
        let twin = existing
            .iter()
            .chain(new.iter().map(|(made, _)| made))
            .any(|other| topic.twins(other));
        if rounds >= RECURS && !twin {
            new.push((topic.clone(), rounds));
        } else {
            dropped.push((topic.clone(), rounds));
        }
    }
    let gone: BTreeSet<&Topic> = dropped.iter().map(|(topic, _)| topic).collect();
    let mut planned = Vec::new();
    let rounds = answered
        .into_iter()
        .map(|round| {
            let topics: BTreeSet<Topic> = given
                .get(&round.target)
                .into_iter()
                .flatten()
                .filter(|topic| !gone.contains(topic))
                .cloned()
                .collect();
            if !topics.is_empty() {
                planned.push((round, topics.clone()));
            }
            (round.target.clone(), topics)
        })
        .collect();
    let plan = planned
        .into_iter()
        .map(|(round, topics)| Planned::new(round.target.clone(), round.notes.clone(), topics))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Backfill {
        rounds,
        failed,
        unsent,
        new,
        dropped,
        problems,
        plan: Plan::new(plan)?,
    })
}

/// The JSON object in what the model said, which it may have fenced or framed.
fn read(said: &str) -> Result<Answer, String> {
    let object = said
        .find('{')
        .zip(said.rfind('}'))
        .filter(|(start, end)| start < end)
        .map(|(start, end)| &said[start..=end])
        .ok_or_else(|| "it answered no JSON object".to_owned())?;
    serde_json::from_str(object).map_err(|failed| format!("its JSON did not read: {failed}"))
}

/// What the batches answered so far.
#[derive(Default)]
struct Sorting {
    /// Every new topic an answer named and gave a round.
    named: BTreeSet<Topic>,
    given: BTreeMap<Target, BTreeSet<Topic>>,
    problems: Vec<String>,
}

impl Sorting {
    /// One batch's answer, taken where it holds: a round of this batch, at most [`MAX_TOPICS`]
    /// topics each, every topic one offered or one the answer names as new.
    fn take(
        &mut self,
        reading: &Reading,
        batch: &[Untopiced],
        offered: &BTreeSet<Topic>,
        answer: Answer,
    ) {
        let mut new = BTreeSet::new();
        for text in answer.new {
            match Topic::new(text) {
                Ok(topic) => {
                    new.insert(reading.spelling(&topic));
                }
                Err(refused) => self
                    .problems
                    .push(format!("a new topic is refused: {refused}")),
            }
        }
        let asked: BTreeSet<&Target> = batch.iter().map(|round| &round.target).collect();
        for (name, texts) in answer.rounds {
            let Some(target) = name.parse::<Target>().ok().filter(|t| asked.contains(t)) else {
                self.problems
                    .push(format!("{name:?} is not a round this batch asked about"));
                continue;
            };
            if texts.len() > MAX_TOPICS {
                self.problems.push(format!(
                    "{target} was given {} topics, more than {MAX_TOPICS}, so none",
                    texts.len()
                ));
                continue;
            }
            let mut topics = BTreeSet::new();
            for text in texts {
                match Topic::new(text).map(|topic| reading.spelling(&topic)) {
                    Ok(topic) if offered.contains(&topic) || new.contains(&topic) => {
                        topics.insert(topic);
                    }
                    Ok(topic) => self.problems.push(format!(
                        "{target}: {topic} is neither in the list nor named as new"
                    )),
                    Err(refused) => self.problems.push(format!("{target}: {refused}")),
                }
            }
            self.named
                .extend(topics.iter().filter(|t| new.contains(*t)).cloned());
            self.given.insert(target, topics);
        }
    }
}
