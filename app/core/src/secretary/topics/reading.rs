//! The topic stores as the Prodrome's tag sets (`prodrome_core::tags`, SPEC law 48): a
//! [`Target`] carries [`Topic`]s. This module names the instantiation, reads the two stores with
//! who set each topic, and reads what the secretary draws from them.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::literal::Datetime;
use prodrome_core::registers::{Node, Prodrome, fold};
use prodrome_core::schema::Schema;
use prodrome_core::tags::membership::{Pair, Presence};
use prodrome_core::tags::{Tagging, alias, membership, set};

use crate::secretary::topics::provenance::{Owner, Provenance};
use crate::secretary::topics::standing::{Opening, Standing, Suggestion};
use crate::secretary::topics::target::Target;
use crate::secretary::topics::topic::Topic;

/// The topic stores' tag sets: `Added(target, topic, at, actor)`, `Removed(…)`,
/// `Aliased(topic, at, actor, to)` and `Unaliased(topic, at, actor)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Topics;

impl Tagging for Topics {
    type Target = Target;
    type Tag = Topic;
    const TARGET: &'static str = "target";
    const TAG: &'static str = "topic";
}

/// A topic put on a target or taken off it.
pub type MembershipEvent = membership::MembershipEvent<Topics>;
/// The membership store's schema.
pub type MembershipVocabulary = membership::MembershipVocabulary<Topics>;
/// A topic's alias event.
pub type AliasEvent = alias::AliasEvent<Topics>;
/// The alias store's schema.
pub type AliasVocabulary = alias::AliasVocabulary<Topics>;
/// What a topic's name stands for.
pub(crate) type Alias = alias::Alias<Topics>;
/// A place where concurrent renames disagree.
pub(crate) type Dispute = set::Disagreement<Topics>;

/// The topics on each target, read through their aliases, with who set each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    pub(super) set: set::TagSet<Topics>,
    /// Each membership's frontier: its presence, and the join of its writers of that presence.
    sides: BTreeMap<Pair<Topics>, (Presence, Provenance)>,
}

impl Reading {
    fn of(memberships: &Prodrome<MembershipEvent>, aliases: &Prodrome<AliasEvent>) -> Self {
        let sides = super::frontier::writers(memberships)
            .into_iter()
            .filter_map(|(pair, (presence, actors))| {
                let by = actors.iter().map(Provenance::of).max()?;
                Some((pair, (presence, by)))
            })
            .collect();
        Self {
            set: set::TagSet::of(memberships, aliases),
            sides,
        }
    }

    /// Where `topic` stands on `target`, in the topic's current spelling.
    #[must_use]
    pub fn standing(&self, target: &Target, topic: &Topic) -> Standing<'_> {
        let topic = self.set.spelling(topic);
        let joined = self
            .set
            .names(&topic)
            .into_iter()
            .filter_map(|tag| {
                self.sides.get(&Pair {
                    target: target.clone(),
                    tag,
                })
            })
            .max();
        let target = target.clone();
        match joined {
            Some((Presence::On, Provenance::Owner)) => Standing::Kept,
            Some((Presence::On, Provenance::Inferred)) => Standing::Suggested(Suggestion {
                reading: self,
                target,
                topic,
            }),
            Some((Presence::Off, Provenance::Owner)) => Standing::Declined,
            Some((Presence::Off, Provenance::Inferred)) | None => Standing::Open(Opening {
                target,
                topic,
                of: std::marker::PhantomData,
            }),
        }
    }

    /// The topics on `target`, each in its current spelling with who set it.
    #[must_use]
    pub fn on(&self, target: &Target) -> BTreeMap<Topic, Provenance> {
        let mut on: BTreeMap<Topic, Provenance> = BTreeMap::new();
        for (pair, (presence, by)) in &self.sides {
            if pair.target == *target && *presence == Presence::On {
                let held = on.entry(self.set.spelling(&pair.tag)).or_insert(*by);
                *held = (*held).max(*by);
            }
        }
        on
    }

    #[must_use]
    pub fn spelling(&self, name: &Topic) -> Topic {
        self.set.spelling(name)
    }

    #[must_use]
    pub fn names(&self, name: &Topic) -> BTreeSet<Topic> {
        self.set.names(name)
    }

    #[must_use]
    pub fn tags(&self, target: &Target) -> BTreeSet<Topic> {
        self.set.tags(target)
    }

    #[must_use]
    pub fn memberships(&self) -> BTreeMap<Target, BTreeSet<Topic>> {
        self.set.memberships()
    }

    #[must_use]
    pub fn shows(&self, chosen: &BTreeSet<Topic>, target: &Target) -> bool {
        self.set.shows(chosen, target)
    }

    #[must_use]
    pub(crate) fn disagreements(&self) -> Vec<Dispute> {
        self.set.disagreements()
    }

    /// The secretary's `Added` of `topic` on `target`, where the topic is [`Standing::Open`] there.
    #[must_use]
    pub fn suggestion(
        &self,
        target: &Target,
        topic: &Topic,
        at: Datetime,
    ) -> Option<MembershipEvent> {
        match self.standing(target, topic) {
            Standing::Open(opening) => Some(opening.suggest(at)),
            Standing::Kept | Standing::Suggested(_) | Standing::Declined => None,
        }
    }

    /// The owner's `Added` of `topic` on `target`, or `None` where the owner set it already. On a
    /// suggestion it is the keep: the owner's write makes the topic the owner's.
    #[must_use]
    pub fn add(
        &self,
        target: &Target,
        topic: &Topic,
        at: Datetime,
        by: &Owner,
    ) -> Option<MembershipEvent> {
        match self.standing(target, topic) {
            Standing::Kept => None,
            Standing::Suggested(_) | Standing::Declined | Standing::Open(_) => Some(
                membership::added(target.clone(), self.spelling(topic), at, by.actor().clone()),
            ),
        }
    }

    /// The owner's `Removed` of every name of `topic` on `target`.
    #[must_use]
    pub fn remove(
        &self,
        target: &Target,
        topic: &Topic,
        at: Datetime,
        by: &Owner,
    ) -> Vec<MembershipEvent> {
        self.set.remove(target, topic, at, by.actor())
    }

    /// The owner's alias events that rename `from` to `to` on every target.
    #[must_use]
    pub fn rename(&self, from: &Topic, to: &Topic, at: Datetime, by: &Owner) -> Vec<AliasEvent> {
        self.set.rename(from, to, at, by.actor())
    }
}

/// A topic store that holds more than one prodrome. Each is begun by one genesis, and a reading
/// of two would join writes that never saw each other's registers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a topic store holds {0} prodromes, and it is begun by one genesis")]
pub struct SeveralGeneses(pub usize);

/// The reading of the membership store's `memberships` and the alias store's `aliases`, each
/// the one prodrome its store holds; a store not begun reads as empty.
///
/// # Errors
/// [`SeveralGeneses`], a store holding more than one prodrome.
pub fn of(
    memberships: &[Node<MembershipEvent>],
    aliases: &[Node<AliasEvent>],
) -> Result<Reading, SeveralGeneses> {
    let (memberships, aliases) = (fold(memberships), fold(aliases));
    let (none, nothing) = (Prodrome::new(), Prodrome::new());
    Ok(Reading::of(
        one(memberships.prodromes(), &none)?,
        one(aliases.prodromes(), &nothing)?,
    ))
}

/// The one prodrome of `prodromes`, or `empty` where there is none.
fn one<'a, K, E: Schema>(
    prodromes: &'a BTreeMap<K, Prodrome<E>>,
    empty: &'a Prodrome<E>,
) -> Result<&'a Prodrome<E>, SeveralGeneses> {
    let mut each = prodromes.values();
    match (each.next(), each.next()) {
        (None, _) => Ok(empty),
        (Some(only), None) => Ok(only),
        (Some(_), Some(_)) => Err(SeveralGeneses(prodromes.len())),
    }
}

/// Every topic on a target, with how many rounds it is on, the most first and then by name, so
/// the order moves only when a count does.
#[must_use]
pub fn in_use(reading: &Reading) -> Vec<(Topic, usize)> {
    let mut counts: BTreeMap<Topic, usize> = BTreeMap::new();
    for (target, topics) in reading.memberships() {
        for topic in topics {
            *counts.entry(topic).or_default() +=
                usize::from(matches!(target, Target::Round { .. }));
        }
    }
    let mut in_use: Vec<(Topic, usize)> = counts.into_iter().collect();
    in_use.sort_by(|(one, many), (two, more)| more.cmp(many).then_with(|| one.cmp(two)));
    in_use
}

/// The topics in use as the topic bar draws them: [`in_use`]'s order, each with the names its
/// disputes hold besides its own spelling.
#[must_use]
pub fn bar(reading: &Reading) -> crate::wire::TopicsPayload {
    let mut disputed: BTreeMap<Topic, BTreeSet<Topic>> = BTreeMap::new();
    for dispute in reading.disagreements() {
        let names: BTreeSet<Topic> = match dispute {
            Dispute::Aliases {
                tag,
                chosen,
                others,
            } => std::iter::once(&chosen)
                .chain(&others)
                .map(|alias| match alias {
                    Alias::Itself => tag.clone(),
                    Alias::As(to) => reading.spelling(to),
                })
                .chain([tag.clone()])
                .collect(),
            Dispute::Ring { names, .. } => names,
        };
        for name in &names {
            let topic = reading.spelling(name);
            disputed
                .entry(topic.clone())
                .or_default()
                .extend(names.iter().filter(|other| **other != topic).cloned());
        }
    }
    crate::wire::TopicsPayload {
        topics: in_use(reading)
            .into_iter()
            .map(|(topic, rounds)| crate::wire::TopicInUse {
                disputed: disputed
                    .remove(&topic)
                    .into_iter()
                    .flatten()
                    .map(String::from)
                    .collect(),
                topic: topic.into(),
                rounds,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use prodrome_core::schema::headed::Body as _;

    use super::*;

    #[test]
    fn no_constructor_is_a_term_or_an_envelope() {
        crate::own::assert_own(<membership::Membership<Topics>>::EVENTS);
        crate::own::assert_own(<alias::Alias<Topics>>::EVENTS);
    }
}
