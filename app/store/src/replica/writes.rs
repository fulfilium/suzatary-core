//! What a replica wrote, each node with the box's answer. Every move is a join (an answer only
//! climbs), so two tabs' moves in any order hold the same writes.
//!
//! THE ROLLBACK LAW (`app/store/tests/writes.rs`): with any set F of the writes refused for good,
//! what stands is what would stand had the replica never written F or anything resting on F, and
//! the refusals name F exactly.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Hash;
use serde::{Deserialize, Serialize};

use crate::replica::held::Node;

/// Why the box refused a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Refusal {
    /// Posted again once its device is trusted.
    Trust,
    /// Posted again, and said once.
    Later,
    /// Held back, with everything resting on it.
    Final,
    /// Sealed under rules the box no longer seals by: held back as a final refusal is.
    Outdated,
}

impl Refusal {
    fn holds_back(self) -> bool {
        match self {
            Refusal::Final | Refusal::Outdated => true,
            Refusal::Trust | Refusal::Later => false,
        }
    }
}

/// How the box answered a node this replica wrote. The order is the join's: an answer only climbs.
/// A refusal stands over the post it answered and a taking over every refusal, so the arms' order
/// must not change.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Answer {
    Unsent,
    Refused {
        kind: Refusal,
        /// The box's sentence.
        reason: String,
    },
    Taken,
}

impl Answer {
    fn is_final(&self) -> bool {
        match self {
            Answer::Refused { kind, .. } => kind.holds_back(),
            Answer::Unsent | Answer::Taken => false,
        }
    }

    fn may_post(&self) -> bool {
        match self {
            Answer::Unsent => true,
            Answer::Refused { kind, .. } => !kind.holds_back(),
            Answer::Taken => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Own<N> {
    pub node: N,
    pub answer: Answer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    Change,
    Signs(Hash),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fit {
    pub most: usize,
    /// Bytes of the posted objects, as the post's body prints them.
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Writes<N>(BTreeMap<Hash, Own<N>>);

impl<N> Default for Writes<N> {
    fn default() -> Self {
        Writes(BTreeMap::new())
    }
}

impl<N: Node> Writes<N> {
    #[must_use]
    pub fn of(owned: &[Own<N>]) -> Self {
        let mut writes = Writes::default();
        writes.apply(owned);
        writes
    }

    pub fn iter(&self) -> impl Iterator<Item = &Own<N>> {
        self.0.values()
    }

    pub fn write(&mut self, nodes: Vec<N>) -> Vec<Own<N>> {
        let owned: Vec<Own<N>> = nodes
            .into_iter()
            .map(|node| Own {
                node,
                answer: Answer::Unsent,
            })
            .collect();
        self.apply(&owned);
        let mut owned = owned;
        owned.extend(self.held_back());
        owned
    }

    /// An answer for a node not written here is passed over.
    pub fn answered(&mut self, answers: impl IntoIterator<Item = (Hash, Answer)>) -> Vec<Own<N>> {
        let owned: Vec<Own<N>> = answers
            .into_iter()
            .filter_map(|(name, answer)| {
                self.0.get(&name).map(|own| Own {
                    node: own.node.clone(),
                    answer,
                })
            })
            .collect();
        self.apply(&owned);
        let mut owned = owned;
        owned.extend(self.held_back());
        owned
    }

    /// Refuse for good each untaken write that rests on one refused for good.
    fn held_back(&mut self) -> Vec<Own<N>> {
        let moved: Vec<Own<N>> = self
            .refusals()
            .into_iter()
            .filter(|(node, _, _)| {
                self.0
                    .get(node.name())
                    .is_some_and(|own| !own.answer.is_final())
            })
            .map(|(node, kind, reason)| Own {
                node: node.clone(),
                answer: Answer::Refused { kind, reason },
            })
            .collect();
        self.apply(&moved);
        moved
    }

    pub fn refused(
        &mut self,
        refused: BTreeMap<Hash, (crate::receipt::Grounds, String)>,
    ) -> Vec<Own<N>> {
        let answers: Vec<(Hash, Answer)> = refused
            .into_iter()
            .filter(|(name, _)| self.0.get(name).is_some_and(|own| !own.answer.is_final()))
            .map(|(name, (grounds, reason))| {
                let kind = grounds.kind();
                (name, Answer::Refused { kind, reason })
            })
            .collect();
        self.answered(answers)
    }

    /// How many of `owned` [`Writes::apply`] would move.
    #[must_use]
    pub fn moves(&self, owned: &[Own<N>]) -> usize {
        owned
            .iter()
            .filter(|own| {
                self.0
                    .get(own.node.name())
                    .is_none_or(|held| own.answer > held.answer)
            })
            .count()
    }

    pub fn apply(&mut self, owned: &[Own<N>]) {
        for own in owned {
            match self.0.get_mut(own.node.name()) {
                Some(held) => {
                    if own.answer > held.answer {
                        held.answer = own.answer.clone();
                    }
                }
                None => {
                    self.0.insert(own.node.name().clone(), own.clone());
                }
            }
        }
    }

    /// The writes not `held`, less each refused for good, of a prodrome in `gone`, or resting on
    /// either.
    pub fn standing(&self, held: impl Fn(&Hash) -> bool, gone: &BTreeSet<Hash>) -> Vec<&N> {
        let mine = self.mine(held);
        let blocked = self.blocked(&mine, gone);
        mine.into_iter()
            .filter(|n| !blocked.contains(n.name()))
            .collect()
    }

    /// The writes not `held` that the box may still take.
    pub fn unsent(&self, held: impl Fn(&Hash) -> bool) -> Vec<&N> {
        let mine = self.mine(held);
        let blocked = self.blocked(&mine, &BTreeSet::new());
        mine.into_iter()
            .filter(|n| {
                !blocked.contains(n.name())
                    && self
                        .0
                        .get(n.name())
                        .is_some_and(|own| own.answer.may_post())
            })
            .collect()
    }

    /// Every write a refusal for good holds back, including each untaken write resting on one,
    /// so no write is held back unsaid.
    #[must_use]
    pub fn refusals(&self) -> Vec<(&N, Refusal, String)> {
        let mut back: BTreeMap<&Hash, (&N, Refusal, String)> = self
            .0
            .values()
            .filter_map(|own| match &own.answer {
                Answer::Refused { kind, reason } if kind.holds_back() => {
                    Some((own.node.name(), (&own.node, *kind, reason.clone())))
                }
                Answer::Unsent | Answer::Refused { .. } | Answer::Taken => None,
            })
            .collect();
        loop {
            let more: Vec<(&Hash, (&N, Refusal, String))> = self
                .0
                .values()
                .filter(|own| own.answer != Answer::Taken && !back.contains_key(own.node.name()))
                .filter_map(|own| {
                    let rests_on = own.node.rests_on();
                    let parent = rests_on.iter().find(|parent| back.contains_key(parent))?;
                    let (_, kind, reason) = &back[parent];
                    Some((
                        own.node.name(),
                        (
                            &own.node,
                            *kind,
                            format!("it rests on {parent}, which the box refused: {reason}"),
                        ),
                    ))
                })
                .collect();
            if more.is_empty() {
                return back.into_values().collect();
            }
            back.extend(more);
        }
    }

    /// Every write refused as [`Refusal::Later`], with its sentence.
    #[must_use]
    pub fn waiting(&self) -> Vec<(&N, String)> {
        self.0
            .values()
            .filter_map(|own| match &own.answer {
                Answer::Refused {
                    kind: Refusal::Later,
                    reason,
                } => Some((&own.node, reason.clone())),
                Answer::Unsent | Answer::Refused { .. } | Answer::Taken => None,
            })
            .collect()
    }

    /// The unsent changes no signature written here covers; a signature refused for good covers
    /// nothing.
    pub fn unsigned(&self, held: impl Fn(&Hash) -> bool, role: impl Fn(&N) -> Role) -> Vec<&N> {
        let signed: BTreeSet<Hash> = self
            .0
            .values()
            .filter(|own| !own.answer.is_final())
            .filter_map(|own| match role(&own.node) {
                Role::Signs(node) => Some(node),
                Role::Change | Role::Other => None,
            })
            .collect();
        self.unsent(held)
            .into_iter()
            .filter(|n| !signed.contains(n.name()) && role(n) == Role::Change)
            .collect()
    }

    /// [`Writes::unsent`] in posts that each fit `fit`, each node beside its signatures. A node
    /// posted before its parent's post waits for the next exchange. A heavier group goes alone.
    pub fn pieces(
        &self,
        held: impl Fn(&Hash) -> bool,
        role: impl Fn(&N) -> Role,
        fit: Fit,
        weight: impl Fn(&N) -> usize,
    ) -> Vec<Vec<&N>> {
        let unsent = self.unsent(held);
        let names: BTreeSet<&Hash> = unsent.iter().map(|n| n.name()).collect();
        let mut signatures: BTreeMap<Hash, Vec<&N>> = BTreeMap::new();
        let mut rest: Vec<&N> = Vec::new();
        for n in unsent {
            match role(n) {
                Role::Signs(node) if names.contains(&node) => {
                    signatures.entry(node).or_default().push(n);
                }
                Role::Signs(_) | Role::Change | Role::Other => rest.push(n),
            }
        }
        let mut pieces: Vec<(Vec<&N>, usize)> = Vec::new();
        for n in rest {
            let mut group = vec![n];
            group.extend(signatures.remove(n.name()).unwrap_or_default());
            let heavy: usize = group.iter().map(|n| weight(n)).sum();
            match pieces.last_mut() {
                Some((piece, bytes))
                    if piece.len() + group.len() <= fit.most && *bytes + heavy <= fit.bytes =>
                {
                    piece.extend(group);
                    *bytes += heavy;
                }
                Some(_) | None => pieces.push((group, heavy)),
            }
        }
        pieces.into_iter().map(|(piece, _)| piece).collect()
    }

    fn mine(&self, held: impl Fn(&Hash) -> bool) -> Vec<&N> {
        self.0
            .values()
            .map(|own| &own.node)
            .filter(|n| !held(n.name()))
            .collect()
    }

    fn blocked<'n>(&self, mine: &[&'n N], gone: &BTreeSet<Hash>) -> BTreeSet<&'n Hash> {
        let mut blocked: BTreeSet<&Hash> = mine
            .iter()
            .filter(|n| {
                n.prodrome().is_some_and(|p| gone.contains(p))
                    || self
                        .0
                        .get(n.name())
                        .is_some_and(|own| own.answer.is_final())
            })
            .map(|n| n.name())
            .collect();
        loop {
            let more: Vec<&Hash> = mine
                .iter()
                .filter(|n| {
                    !blocked.contains(n.name()) && n.rests_on().iter().any(|p| blocked.contains(p))
                })
                .map(|n| n.name())
                .collect();
            if more.is_empty() {
                return blocked;
            }
            blocked.extend(more);
        }
    }
}
