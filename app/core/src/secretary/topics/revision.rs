//! The one planner of the secretary's topic writes, through a [`Standing`]'s tokens.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::literal::Datetime;
use serde_json::{Value, json};

use crate::refusal::{Refusal, shown};
use crate::secretary::taint::Taint;
use crate::secretary::targets::MAX_TOPICS;
use crate::secretary::topics::provenance::Provenance;
use crate::secretary::topics::reading::{MembershipEvent, Reading, in_use};
use crate::secretary::topics::standing::Standing;
use crate::secretary::topics::topic::Topic;
use crate::topic_target::Target;

/// Topics to suggest, suggestions to drop, and suggestions to rename, each `from` to its `to`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Revision {
    pub add: BTreeSet<Topic>,
    pub drop: BTreeSet<Topic>,
    pub rename: BTreeMap<Topic, Topic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub events: Vec<MembershipEvent>,
    pub new: BTreeSet<Topic>,
}

/// The secretary's writes for `revision` on `target`, by a round holding `taint`.
///
/// # Errors
/// A refusal in words for each case the owner's choice, the taint or [`MAX_TOPICS`] forbids.
pub fn plan(
    reading: &Reading,
    target: &Target,
    revision: &Revision,
    taint: Taint,
    at: Datetime,
) -> Result<Planned, Refusal> {
    let off: BTreeSet<Topic> = revision
        .drop
        .iter()
        .chain(revision.rename.keys())
        .map(|topic| reading.spelling(topic))
        .collect();
    let on: BTreeSet<Topic> = revision
        .add
        .iter()
        .chain(revision.rename.values())
        .map(|topic| reading.spelling(topic))
        .collect();
    if let Some(both) = off.intersection(&on).next() {
        return Err(Refusal::bad_request(format!(
            "{} is both put on and taken off; name it once",
            shown(both.as_str())
        )));
    }
    let mut events = Vec::new();
    for topic in &off {
        match reading.standing(target, topic) {
            Standing::Suggested(suggestion) => events.extend(suggestion.drop(at)),
            Standing::Kept => {
                return Err(Refusal::bad_request(format!(
                    "the owner set {topic} on {target}; only the owner takes it off or renames it"
                )));
            }
            Standing::Declined | Standing::Open(_) => {
                return Err(Refusal::bad_request(format!(
                    "{target} holds no topic {topic}; its topics are {}",
                    told(reading, target)
                )));
            }
        }
    }
    let existing: BTreeSet<Topic> = in_use(reading).into_iter().map(|(t, _)| t).collect();
    let new: BTreeSet<Topic> = on.difference(&existing).cloned().collect();
    if !new.is_empty() && taint == Taint::Tainted {
        return Err(Refusal::bad_request(format!(
            "this round has read words that are not the owner's, so it may only use topics \
             that exist; {} would be new. The topics are: {}",
            names(new.iter()),
            listing(reading)
        )));
    }
    for (made, topic) in new.iter().enumerate() {
        let earlier = new.iter().take(made);
        if let Some(twin) = existing.iter().chain(earlier).find(|e| topic.twins(e)) {
            return Err(Refusal::bad_request(format!(
                "{} is a near twin of the topic {twin}; name one of them",
                shown(topic.as_str()),
            )));
        }
    }
    let mut kept = 0;
    for topic in &on {
        match reading.standing(target, topic) {
            Standing::Open(opening) => events.push(opening.suggest(at)),
            Standing::Kept | Standing::Suggested(_) => kept += 1,
            Standing::Declined => {
                return Err(Refusal::bad_request(format!(
                    "the owner took {topic} off {target}; only the owner puts it back"
                )));
            }
        }
    }
    let held = reading.on(target);
    let after = held.len() - off.len() + on.len() - kept;
    if after > MAX_TOPICS {
        return Err(Refusal::bad_request(format!(
            "{target} already has {}, and a round holds at most {MAX_TOPICS} topics",
            names(held.keys())
        )));
    }
    Ok(Planned { events, new })
}

/// The topics on `target` as the secretary is told them.
#[must_use]
pub fn held(reading: &Reading, target: &Target) -> Value {
    let on = reading.on(target);
    let by = |who: Provenance| -> Vec<String> {
        on.iter()
            .filter(|(_, by)| **by == who)
            .map(|(topic, _)| topic.to_string())
            .collect()
    };
    json!({
        "target": target.to_string(),
        "owner": by(Provenance::Owner),
        "suggested": by(Provenance::Inferred),
    })
}

fn told(reading: &Reading, target: &Target) -> String {
    let on = reading.on(target);
    if on.is_empty() {
        return "none".to_owned();
    }
    on.iter()
        .map(|(topic, by)| match by {
            Provenance::Owner => format!("{topic} (the owner's)"),
            Provenance::Inferred => format!("{topic} (yours)"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The topics in use, each with how many rounds it is on, the most first.
#[must_use]
pub fn listing(reading: &Reading) -> String {
    let in_use = in_use(reading);
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
