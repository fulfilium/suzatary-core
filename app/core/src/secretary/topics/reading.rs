//! The reading is a function of the two stores' object sets alone, so every device holding the
//! same objects reads the same topics, offline included. Where concurrent renames disagree, it
//! still chooses one topic by a rule every device applies alike, and lists the [`Dispute`].

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Everything;
use prodrome_core::registers::{Node, Stamp, fold};

use crate::secretary::topics::alias_event::{Alias, AliasEvent};
use crate::secretary::topics::dispute::Dispute;
use crate::secretary::topics::membership::Presence;
use crate::secretary::topics::membership_event::MembershipEvent;
use crate::secretary::topics::target::Target;
use crate::secretary::topics::topic::Topic;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reading {
    on: BTreeMap<Target, BTreeSet<Topic>>,
    aliases: BTreeMap<Topic, Candidates>,
    spelled: BTreeMap<Topic, Topic>,
    rings: BTreeMap<Topic, BTreeSet<Topic>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidates {
    chosen: Alias,
    others: Vec<Alias>,
}

impl Reading {
    #[must_use]
    pub fn of(memberships: &[Node<MembershipEvent>], aliases: &[Node<AliasEvent>]) -> Self {
        let mut on: BTreeMap<Target, BTreeSet<Topic>> = BTreeMap::new();
        for (membership, stream) in fold(memberships).entities() {
            let registers: crate::sole::One<'_, MembershipEvent> =
                prodrome_core::fold::read(stream, None, &Everything);
            if registers.presence() == Presence::On {
                on.entry(membership.target.clone())
                    .or_default()
                    .insert(membership.topic.clone());
            }
        }
        let folded = fold(aliases);
        let mut written: BTreeMap<&Topic, Vec<&Stamp<AliasEvent>>> = BTreeMap::new();
        for (topic, stream) in folded.entities() {
            let registers: crate::sole::One<'_, AliasEvent> =
                prodrome_core::fold::read(stream, None, &Everything);
            written
                .entry(topic)
                .or_default()
                .extend(registers.reading());
        }
        let aliases: BTreeMap<Topic, Candidates> = written
            .into_iter()
            .filter_map(|(topic, mut stamps)| {
                stamps.sort_by(|one, two| one.name.cmp(&two.name));
                let mut values: Vec<Alias> = Vec::with_capacity(stamps.len());
                for stamp in stamps {
                    if !values.contains(stamp.event.said()) {
                        values.push(stamp.event.said().clone());
                    }
                }
                let mut values = values.into_iter();
                let chosen = values.next()?;
                Some((
                    topic.clone(),
                    Candidates {
                        chosen,
                        others: values.collect(),
                    },
                ))
            })
            .collect();
        let known: BTreeSet<&Topic> = on
            .values()
            .flatten()
            .chain(aliases.keys())
            .chain(
                aliases
                    .values()
                    .filter_map(|candidates| match &candidates.chosen {
                        Alias::As(to) => Some(to),
                        Alias::Itself => None,
                    }),
            )
            .collect();
        let mut spelled = BTreeMap::new();
        let mut rings = BTreeMap::new();
        for name in known {
            let (topic, ring) = follow(&aliases, name);
            if let Some(ring) = ring {
                rings.insert(topic.clone(), ring);
            }
            spelled.insert(name.clone(), topic);
        }
        Self {
            on,
            aliases,
            spelled,
            rings,
        }
    }

    /// The end of `name`'s alias chain, or the least name of a ring.
    #[must_use]
    pub fn spelling(&self, name: &Topic) -> Topic {
        self.spelled.get(name).unwrap_or(name).clone()
    }

    #[must_use]
    pub fn names(&self, name: &Topic) -> BTreeSet<Topic> {
        let topic = self.spelling(name);
        self.spelled
            .iter()
            .filter(|(_, spelled)| **spelled == topic)
            .map(|(name, _)| name.clone())
            .chain([topic.clone()])
            .collect()
    }

    #[must_use]
    pub fn topics(&self, target: &Target) -> BTreeSet<Topic> {
        self.on
            .get(target)
            .into_iter()
            .flatten()
            .map(|name| self.spelling(name))
            .collect()
    }

    #[must_use]
    pub fn memberships(&self) -> BTreeMap<Target, BTreeSet<Topic>> {
        self.on
            .keys()
            .map(|target| (target.clone(), self.topics(target)))
            .filter(|(_, topics)| !topics.is_empty())
            .collect()
    }

    /// Whether `target` carries every topic of `chosen`, each read in its current spelling: the
    /// stream's filter, so no topic chosen shows every target.
    #[must_use]
    pub fn shows(&self, chosen: &BTreeSet<Topic>, target: &Target) -> bool {
        let on = self.topics(target);
        chosen
            .iter()
            .all(|topic| on.contains(&self.spelling(topic)))
    }

    /// Every topic on a target, with how many rounds it is on, the most first and then by name,
    /// so the order moves only when a count does.
    #[must_use]
    pub fn in_use(&self) -> Vec<(Topic, usize)> {
        let mut counts: BTreeMap<Topic, usize> = BTreeMap::new();
        for (target, topics) in self.memberships() {
            for topic in topics {
                *counts.entry(topic).or_default() +=
                    usize::from(matches!(target, Target::Round { .. }));
            }
        }
        let mut in_use: Vec<(Topic, usize)> = counts.into_iter().collect();
        in_use.sort_by(|(one, many), (two, more)| more.cmp(many).then_with(|| one.cmp(two)));
        in_use
    }

    /// The topics in use as the topic bar draws them: [`Reading::in_use`]'s order, each with the
    /// names its disputes hold besides its own spelling.
    #[must_use]
    pub fn bar(&self) -> crate::wire::TopicsPayload {
        let mut disputed: BTreeMap<Topic, BTreeSet<Topic>> = BTreeMap::new();
        for dispute in self.disputes() {
            let names: BTreeSet<Topic> = match dispute {
                Dispute::Aliases {
                    topic,
                    chosen,
                    others,
                } => std::iter::once(&chosen)
                    .chain(&others)
                    .map(|alias| match alias {
                        Alias::Itself => topic.clone(),
                        Alias::As(to) => self.spelling(to),
                    })
                    .chain([topic.clone()])
                    .collect(),
                Dispute::Ring { names, .. } => names,
            };
            for name in &names {
                let topic = self.spelling(name);
                disputed
                    .entry(topic.clone())
                    .or_default()
                    .extend(names.iter().filter(|other| **other != topic).cloned());
            }
        }
        crate::wire::TopicsPayload {
            topics: self
                .in_use()
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

    #[must_use]
    pub fn disputes(&self) -> Vec<Dispute> {
        let aliases = self
            .aliases
            .iter()
            .filter(|(_, candidates)| !candidates.others.is_empty())
            .map(|(topic, candidates)| Dispute::Aliases {
                topic: topic.clone(),
                chosen: candidates.chosen.clone(),
                others: candidates.others.clone(),
            });
        let rings = self.rings.iter().map(|(spelled, names)| Dispute::Ring {
            names: names.clone(),
            spelled: spelled.clone(),
        });
        aliases.chain(rings).collect()
    }

    #[must_use]
    pub fn add(
        &self,
        target: &Target,
        topic: &Topic,
        at: Datetime,
        actor: &Actor,
    ) -> Option<MembershipEvent> {
        let spelled = self.spelling(topic);
        (!self.topics(target).contains(&spelled))
            .then(|| MembershipEvent::added(target.clone(), spelled, at, actor.clone()))
    }

    /// Takes off every name of the topic this reading sees on `target`, and no add it has not
    /// seen.
    #[must_use]
    pub fn remove(
        &self,
        target: &Target,
        topic: &Topic,
        at: Datetime,
        actor: &Actor,
    ) -> Vec<MembershipEvent> {
        let spelled = self.spelling(topic);
        self.on
            .get(target)
            .into_iter()
            .flatten()
            .filter(|name| self.spelling(name) == spelled)
            .map(|name| MembershipEvent::removed(target.clone(), name.clone(), at, actor.clone()))
            .collect()
    }

    /// Afterwards every name of `from`'s topic and of `to`'s topic stands for `to`: a rename to a
    /// name of another topic merges the two, and renaming back undoes a rename. Each write
    /// supersedes what this reading holds for its name, so it settles disputes on those names.
    #[must_use]
    pub fn rename(&self, from: &Topic, to: &Topic, at: Datetime, actor: &Actor) -> Vec<AliasEvent> {
        let itself = (!self.settled(to, &Alias::Itself))
            .then(|| AliasEvent::unaliased(to.clone(), at, actor.clone()));
        let pointed = Alias::As(to.clone());
        let mut names = self.names(from);
        names.extend(self.names(to));
        let aliased = names
            .into_iter()
            .filter(|name| !self.settled(name, &pointed))
            // `aliased` refuses `to` itself, the one name that is no alias afterwards.
            .filter_map(|name| AliasEvent::aliased(name, to.clone(), at, actor.clone()).ok());
        itself.into_iter().chain(aliased).collect()
    }

    /// An unwritten register reads `Itself`.
    fn settled(&self, name: &Topic, alias: &Alias) -> bool {
        match self.aliases.get(name) {
            None => *alias == Alias::Itself,
            Some(candidates) => candidates.chosen == *alias && candidates.others.is_empty(),
        }
    }
}

fn follow(aliases: &BTreeMap<Topic, Candidates>, name: &Topic) -> (Topic, Option<BTreeSet<Topic>>) {
    let mut path: Vec<&Topic> = Vec::new();
    let mut at = name;
    loop {
        if let Some(start) = path.iter().position(|seen| *seen == at) {
            let ring = &path[start..];
            let least = ring.iter().fold(at, |least, name| least.min(*name));
            return (
                least.clone(),
                Some(ring.iter().map(|name| (*name).clone()).collect()),
            );
        }
        path.push(at);
        match aliases.get(at).map(|candidates| &candidates.chosen) {
            Some(Alias::As(to)) => at = to,
            Some(Alias::Itself) | None => return (at.clone(), None),
        }
    }
}
