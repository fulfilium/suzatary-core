//! The topic stores as the Prodrome's tag sets (`prodrome_core::tags`, SPEC law 48): a
//! [`Target`] carries [`Topic`]s. This module names the instantiation, reads the two stores, and
//! reads what the secretary draws from them.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::registers::{Node, Prodrome, fold};
use prodrome_core::schema::Schema;
use prodrome_core::tags::{Tagging, alias, membership, set};

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
pub type Alias = alias::Alias<Topics>;
/// A place where concurrent renames disagree.
pub type Dispute = set::Disagreement<Topics>;
/// The topics on each target, read through their aliases.
pub type Reading = set::TagSet<Topics>;

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
