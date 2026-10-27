//! What the backfill's dry run proposes, as a file the owner reads and the run applies as it
//! stands: each round with its notes and the 1 to [`MAX_TOPICS`] topics they get.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use prodrome_core::literal::Datetime;
use serde::Deserialize;

use crate::secretary::store::Stored;
use crate::secretary::topics::reading::MembershipEvent;
use crate::secretary::topics::reading::Reading;
use crate::secretary::topics::target::Target;
use crate::secretary::topics::targets::{MAX_TOPICS, rounds};
use crate::secretary::topics::topic::Topic;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan(Vec<Planned>);

/// A round, its notes, and the topics they get; built only by [`Planned::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    round: Target,
    notes: Vec<Target>,
    topics: BTreeSet<Topic>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotAPlan {
    #[error("the plan is not TOML with one [[rounds]] table per round: {0}")]
    Toml(String),
    #[error("{0} is not a round")]
    NotARound(Target),
    #[error("{round}: {note} is not a note")]
    NotANote { round: Target, note: Target },
    #[error("{0} names {1} topic(s); a round gets 1 to {MAX_TOPICS}")]
    Count(Target, usize),
    #[error("{0} is planned twice")]
    Twice(Target),
}

/// Why a plan was not applied, every target that stands in its way named.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the plan is refused whole, and nothing was written:\n{}", .0.join("\n"))]
pub struct Stale(pub Vec<String>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    rounds: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    round: Target,
    #[serde(default)]
    notes: Vec<Target>,
    topics: BTreeSet<Topic>,
}

impl Planned {
    /// # Errors
    /// A round that is not a `round:`, a note that is not a `note:`, or no topic or more than
    /// [`MAX_TOPICS`].
    pub fn new(
        round: Target,
        notes: Vec<Target>,
        topics: BTreeSet<Topic>,
    ) -> Result<Self, NotAPlan> {
        if !matches!(round, Target::Round { .. }) {
            return Err(NotAPlan::NotARound(round));
        }
        if let Some(note) = notes.iter().find(|n| !matches!(n, Target::Note { .. })) {
            return Err(NotAPlan::NotANote {
                note: note.clone(),
                round,
            });
        }
        if !(1..=MAX_TOPICS).contains(&topics.len()) {
            return Err(NotAPlan::Count(round, topics.len()));
        }
        Ok(Self {
            round,
            notes,
            topics,
        })
    }

    #[must_use]
    pub fn round(&self) -> &Target {
        &self.round
    }

    #[must_use]
    pub fn topics(&self) -> &BTreeSet<Topic> {
        &self.topics
    }

    fn targets(&self) -> impl Iterator<Item = &Target> {
        std::iter::once(&self.round).chain(&self.notes)
    }
}

impl Plan {
    /// # Errors
    /// The same round twice.
    pub fn new(planned: Vec<Planned>) -> Result<Self, NotAPlan> {
        let mut seen = BTreeSet::new();
        if let Some(twice) = planned.iter().find(|p| !seen.insert(&p.round)) {
            return Err(NotAPlan::Twice(twice.round.clone()));
        }
        Ok(Self(planned))
    }

    #[must_use]
    pub fn rounds(&self) -> &[Planned] {
        &self.0
    }

    /// The file [`Plan::parse`] reads back as this plan. Every value is a target or a topic,
    /// whose one spelling holds no quote and no backslash, so none needs escaping.
    #[must_use]
    pub fn print(&self) -> String {
        let quoted = |items: &mut dyn Iterator<Item = String>| {
            items
                .map(|item| format!("\"{item}\""))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut text = String::new();
        for planned in &self.0 {
            let _ = write!(
                text,
                "[[rounds]]\nround = \"{}\"\nnotes = [{}]\ntopics = [{}]\n\n",
                planned.round,
                quoted(&mut planned.notes.iter().map(ToString::to_string)),
                quoted(&mut planned.topics.iter().map(ToString::to_string)),
            );
        }
        text
    }

    /// # Errors
    /// Text that is not such a file, or any entry [`Planned::new`] or [`Plan::new`] refuses.
    pub fn parse(text: &str) -> Result<Self, NotAPlan> {
        let file: File =
            toml::from_str(text).map_err(|failed| NotAPlan::Toml(failed.message().to_owned()))?;
        let planned = file
            .rounds
            .into_iter()
            .map(|entry| Planned::new(entry.round, entry.notes, entry.topics))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(planned)
    }

    /// The memberships that apply this plan as written, as the assistant: a target already
    /// holding some of its planned topics gets the rest, so applying twice writes nothing.
    ///
    /// # Errors
    /// Every target that is no longer a live round or note of `conversations`, or that has
    /// gained a topic the plan does not give it, named; then nothing is planned.
    pub fn apply(
        &self,
        conversations: &[Stored],
        reading: &Reading,
        at: Datetime,
    ) -> Result<Vec<MembershipEvent>, Stale> {
        let live: BTreeSet<Target> = conversations
            .iter()
            .flat_map(rounds)
            .flat_map(|round| std::iter::once(round.target()).chain(round.notes()))
            .collect();
        let mut stale = Vec::new();
        for planned in &self.0 {
            for target in planned.targets() {
                let on = reading.tags(target);
                if !live.contains(target) {
                    stale.push(format!("{target} is no longer a live round or note"));
                } else if !on.is_subset(&planned.topics) {
                    stale.push(format!(
                        "{target} has gained {} since the dry run",
                        on.difference(&planned.topics)
                            .map(Topic::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
        }
        if !stale.is_empty() {
            return Err(Stale(stale));
        }
        let actor = crate::secretary::writes_as();
        Ok(self
            .0
            .iter()
            .flat_map(|planned| {
                planned.targets().flat_map(|target| {
                    planned
                        .topics
                        .iter()
                        .filter_map(|topic| reading.add(target, topic, at, &actor))
                })
            })
            .collect())
    }
}
