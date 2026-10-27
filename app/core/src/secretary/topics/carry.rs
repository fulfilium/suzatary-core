//! The one-time carry from conversations to the stream: the rounds and notes of each conversation
//! a hand-written [`Map`] names get that conversation's topics. It is planned from the
//! conversations and the topics already read, so a second run plans nothing.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::literal::Datetime;
use serde::Deserialize;

use crate::secretary::store::{ConversationId, Stored};
use crate::secretary::topics::reading::MembershipEvent;
use crate::secretary::topics::reading::Reading;
use crate::secretary::topics::target::Target;
use crate::secretary::topics::topic::{NotATopic, Topic};
use crate::secretary::words::Words;

use super::targets::MAX_TOPICS;

/// The topics of each conversation it names, parsed whole by [`Map::parse`]: a TOML file with
/// one table per conversation id, each `topics = [..]` naming 1 to [`MAX_TOPICS`] topics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map(BTreeMap<ConversationId, Chosen>);

/// One to [`MAX_TOPICS`] distinct topics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chosen(BTreeSet<Topic>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    topics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAMap {
    #[error("the map is not TOML with one table of `topics` per conversation: {0}")]
    Toml(String),
    #[error("the map has {} problem(s):\n{}", .0.len(), lines(.0))]
    Problems(Vec<Problem>),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    #[error("{0:?} is not a conversation id")]
    NotAnId(String),
    #[error("{0} is not a conversation the store holds")]
    Unknown(ConversationId),
    #[error("{0} names {1} topic(s); name 1 to {MAX_TOPICS}")]
    Count(ConversationId, usize),
    #[error("{0}: {1}")]
    Topic(ConversationId, NotATopic),
    #[error("{0} names {1} twice")]
    Twice(ConversationId, Topic),
}

fn lines(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

impl Map {
    /// Refuses the whole map on any problem, and lists every problem it finds.
    ///
    /// # Errors
    /// Text that is not such a TOML file, or any table naming an unknown conversation, an
    /// invalid topic, a topic twice, or no topic or more than [`MAX_TOPICS`].
    pub fn parse(text: &str, conversations: &[Stored]) -> Result<Self, NotAMap> {
        let entries: BTreeMap<String, Entry> =
            toml::from_str(text).map_err(|failed| NotAMap::Toml(failed.message().to_owned()))?;
        let held: BTreeSet<&ConversationId> =
            conversations.iter().map(|stored| &stored.id).collect();
        let mut problems = Vec::new();
        let mut map = BTreeMap::new();
        for (id, entry) in entries {
            let Ok(conversation) = ConversationId::new(id.clone()) else {
                problems.push(Problem::NotAnId(id));
                continue;
            };
            if !held.contains(&conversation) {
                problems.push(Problem::Unknown(conversation));
                continue;
            }
            match Chosen::of(&conversation, entry.topics) {
                Ok(chosen) => {
                    map.insert(conversation, chosen);
                }
                Err(found) => problems.extend(found),
            }
        }
        if problems.is_empty() {
            Ok(Self(map))
        } else {
            Err(NotAMap::Problems(problems))
        }
    }
}

impl Chosen {
    fn of(conversation: &ConversationId, texts: Vec<String>) -> Result<Self, Vec<Problem>> {
        if !(1..=MAX_TOPICS).contains(&texts.len()) {
            return Err(vec![Problem::Count(conversation.clone(), texts.len())]);
        }
        let mut problems = Vec::new();
        let mut topics = BTreeSet::new();
        for text in texts {
            match Topic::new(text) {
                Ok(topic) if topics.contains(&topic) => {
                    problems.push(Problem::Twice(conversation.clone(), topic));
                }
                Ok(topic) => {
                    topics.insert(topic);
                }
                Err(refused) => problems.push(Problem::Topic(conversation.clone(), refused)),
            }
        }
        if problems.is_empty() {
            Ok(Self(topics))
        } else {
            Err(problems)
        }
    }

    pub fn topics(&self) -> impl Iterator<Item = &Topic> {
        self.0.iter()
    }
}

/// One conversation as a person reads it to write the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub conversation: ConversationId,
    pub opened: Datetime,
    pub title: Words<String>,
    pub rounds: usize,
}

#[must_use]
pub fn listing(conversations: &[Stored]) -> Vec<Listed> {
    conversations
        .iter()
        .map(|stored| Listed {
            conversation: stored.id.clone(),
            opened: stored.opened,
            title: stored.title.clone(),
            rounds: targets(stored).0.len(),
        })
        .collect()
}

/// What the carry does to one conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carrying {
    /// Its `rounds` rounds and `notes` notes get `topics`; `writes` is how many memberships this
    /// run writes for them.
    Filed {
        conversation: ConversationId,
        topics: Chosen,
        rounds: usize,
        notes: usize,
        writes: usize,
    },
    /// The map does not name it, so it gets no topic.
    Unnamed { conversation: ConversationId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carry {
    /// One per conversation, in the order given.
    pub conversations: Vec<Carrying>,
    /// Every membership to write, as the assistant, at one instant.
    pub events: Vec<MembershipEvent>,
}

/// The carry of `map` over `conversations` and the topics `reading` already holds: a membership
/// it reads on plans nothing, so running it again plans nothing.
#[must_use]
pub fn plan(conversations: &[Stored], map: &Map, reading: &Reading, at: Datetime) -> Carry {
    let actor = crate::secretary::writes_as();
    let mut events = Vec::new();
    let carried = conversations
        .iter()
        .map(|stored| {
            let conversation = stored.id.clone();
            let Some(chosen) = map.0.get(&conversation) else {
                return Carrying::Unnamed { conversation };
            };
            let (rounds, notes) = targets(stored);
            let planned: Vec<MembershipEvent> = rounds
                .iter()
                .chain(&notes)
                .flat_map(|target| {
                    chosen
                        .topics()
                        .filter_map(|topic| reading.add(target, topic, at, &actor))
                })
                .collect();
            let writes = planned.len();
            events.extend(planned);
            Carrying::Filed {
                conversation,
                topics: chosen.clone(),
                rounds: rounds.len(),
                notes: notes.len(),
                writes,
            }
        })
        .collect();
    Carry {
        conversations: carried,
        events,
    }
}

/// The live rounds and the live notes.
fn targets(stored: &Stored) -> (Vec<Target>, Vec<Target>) {
    let rounds = super::targets::rounds(stored);
    let notes = rounds
        .iter()
        .flat_map(super::targets::Round::notes)
        .collect();
    (
        rounds.iter().map(super::targets::Round::target).collect(),
        notes,
    )
}
