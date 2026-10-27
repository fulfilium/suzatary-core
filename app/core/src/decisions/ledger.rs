use std::collections::{BTreeMap, BTreeSet};

use prodrome_core::event::Actor;
use prodrome_core::fold::Order;
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Everything;
use prodrome_core::registers::{Node, Stamp, fold};
use prodrome_core::schema::Schema as _;

use super::{DecisionEvent, DecisionId, DecisionRegisters, Said, Statement};

/// Every decision the history holds, as one value: its statement, its life and its links.
///
/// SUPERSESSION IS AN ORDER on decisions: `a ≤ b` when `b`, or a decision `b` supersedes, names
/// `a` among what it supersedes, so a superseding decision reads above what it supersedes. It is
/// read off the links, which only grow, and nothing else orders two decisions: a clock or a name
/// never says which is the newer word. Concurrent links can close a cycle, which no write here
/// makes (the store's writer refuses one); the reading names it rather than pick an order.
///
/// A decision IN FORCE is one neither withdrawn nor superseded. Both only ever become true, so
/// "out of force" is monotone in the history and needs no coordination to read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ledger {
    decisions: BTreeMap<DecisionId, Decision>,
}

/// What a ledger holds, as its order sees it: each decision's links and whether it is withdrawn.
/// Statements are not in it, since a revision replaces one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Held(BTreeMap<DecisionId, (BTreeSet<DecisionId>, bool)>);

/// One decision as the history reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// Its statement, or, after two concurrent revisions, every candidate: never empty.
    statements: Vec<Statement>,
    /// When it was first decided: the earliest stamp among its statements, data and never an
    /// order.
    pub decided: Datetime,
    /// Every decision it supersedes.
    pub supersedes: BTreeSet<DecisionId>,
    /// Its withdrawal, if it was withdrawn.
    pub withdrawn: Option<Withdrawal>,
}

/// A withdrawal, as its event wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Withdrawal {
    /// When.
    pub at: Datetime,
    /// Who.
    pub by: Actor,
    /// Why.
    pub reason: String,
}

impl Ledger {
    /// The history's reading. A key whose history holds no statement (a withdrawal alone, which
    /// no writer here makes) is not a decision and reads as absent.
    #[must_use]
    pub fn of(nodes: &[Node<DecisionEvent>]) -> Self {
        let folded = fold(nodes);
        let mut decisions = BTreeMap::new();
        for (id, stream) in folded.entities() {
            let registers: DecisionRegisters<'_> =
                prodrome_core::fold::read(stream, None, &Everything);
            if let Some(decision) = Decision::of(stream, &registers) {
                decisions.insert(id.clone(), decision);
            }
        }
        Self { decisions }
    }

    /// What it holds, as its order sees it.
    #[must_use]
    pub fn held(&self) -> Held {
        Held(
            self.decisions
                .iter()
                .map(|(id, decision)| {
                    (
                        id.clone(),
                        (decision.supersedes.clone(), decision.withdrawn.is_some()),
                    )
                })
                .collect(),
        )
    }

    /// One decision.
    #[must_use]
    pub fn get(&self, id: &DecisionId) -> Option<&Decision> {
        self.decisions.get(id)
    }

    /// Every decision, by id.
    pub fn iter(&self) -> impl Iterator<Item = (&DecisionId, &Decision)> {
        self.decisions.iter()
    }

    /// How many decisions there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.decisions.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.decisions.is_empty()
    }

    /// The decisions that supersede `id` directly.
    #[must_use]
    pub fn superseded_by(&self, id: &DecisionId) -> BTreeSet<&DecisionId> {
        self.decisions
            .iter()
            .filter(|(_, decision)| decision.supersedes.contains(id))
            .map(|(by, _)| by)
            .collect()
    }

    /// Everything strictly below `id` in the order: what it supersedes, and what those do, to
    /// the bottom, whether or not this ledger holds them.
    #[must_use]
    pub fn beneath(&self, id: &DecisionId) -> BTreeSet<DecisionId> {
        let mut below = BTreeSet::new();
        let mut frontier: Vec<&DecisionId> = self.links(id).collect();
        while let Some(next) = frontier.pop() {
            if below.insert(next.clone()) {
                frontier.extend(self.links(next));
            }
        }
        below
    }

    /// The supersession order: `a ≤ b` when they are one decision or `b` stands above `a`.
    #[must_use]
    pub fn below(&self, a: &DecisionId, b: &DecisionId) -> bool {
        a == b || self.beneath(b).contains(a)
    }

    /// Whether `id` is decided, not withdrawn and superseded by nothing.
    #[must_use]
    pub fn in_force(&self, id: &DecisionId) -> bool {
        self.get(id)
            .is_some_and(|decision| decision.withdrawn.is_none())
            && self.superseded_by(id).is_empty()
    }

    /// The decisions on a cycle of links, which concurrent writers can close and none here
    /// does: each stands above itself, so the order is not antisymmetric among them.
    #[must_use]
    pub fn cycles(&self) -> BTreeSet<&DecisionId> {
        self.decisions
            .keys()
            .filter(|id| self.beneath(id).contains(*id))
            .collect()
    }

    /// The decisions as a reader is shown them: in LINEAGES, each the decisions joined by links,
    /// every decision before everything it supersedes (a linear extension of the order), the
    /// lineages newest first. Among incomparable decisions the later decided comes first, then
    /// the lesser id, which is presentation and decides nothing. A cycle's decisions follow the
    /// rest of their lineage, by id.
    #[must_use]
    pub fn lineages(&self) -> Vec<Vec<&DecisionId>> {
        let mut lineages: Vec<Vec<&DecisionId>> = self
            .components()
            .into_iter()
            .map(|component| self.extension(&component))
            .collect();
        lineages.sort_by(|one, two| {
            self.newest(two)
                .cmp(&self.newest(one))
                .then_with(|| one.first().cmp(&two.first()))
        });
        lineages
    }

    /// What `id` supersedes, as this ledger holds it.
    fn links<'s>(&'s self, id: &DecisionId) -> impl Iterator<Item = &'s DecisionId> + 's {
        self.get(id)
            .into_iter()
            .flat_map(|decision| decision.supersedes.iter())
    }

    /// The decisions joined by links, a link to a decision not held joining nothing.
    fn components(&self) -> Vec<BTreeSet<&DecisionId>> {
        let mut seen: BTreeSet<&DecisionId> = BTreeSet::new();
        let mut components = Vec::new();
        for start in self.decisions.keys() {
            if seen.contains(start) {
                continue;
            }
            let mut component = BTreeSet::new();
            let mut frontier = vec![start];
            while let Some(id) = frontier.pop() {
                if !component.insert(id) {
                    continue;
                }
                let down = self
                    .links(id)
                    .filter(|link| self.decisions.contains_key(*link));
                let up = self.superseded_by(id).into_iter();
                frontier.extend(down.chain(up));
            }
            seen.extend(component.iter().copied());
            components.push(component);
        }
        components
    }

    /// One component, superseding before superseded: each step takes, among the decisions
    /// nothing remaining supersedes, the one shown first.
    fn extension<'s>(&'s self, component: &BTreeSet<&'s DecisionId>) -> Vec<&'s DecisionId> {
        let mut remaining: BTreeSet<&DecisionId> = component.clone();
        let mut order = Vec::with_capacity(component.len());
        loop {
            let ready = remaining
                .iter()
                .copied()
                .filter(|id| {
                    !remaining
                        .iter()
                        .any(|above| above != id && self.links(above).any(|link| link == *id))
                })
                .min_by(|one, two| {
                    self.decided(two)
                        .cmp(&self.decided(one))
                        .then_with(|| one.cmp(two))
                });
            let Some(next) = ready else { break };
            remaining.remove(next);
            order.push(next);
        }
        order.extend(remaining);
        order
    }

    fn decided(&self, id: &DecisionId) -> Option<Datetime> {
        self.get(id).map(|decision| decision.decided)
    }

    fn newest(&self, lineage: &[&DecisionId]) -> Option<Datetime> {
        lineage.iter().filter_map(|id| self.decided(id)).max()
    }
}

/// A larger history holds a larger ledger: every decision, link and withdrawal of the smaller is
/// in it.
impl Order for Held {
    fn le(&self, other: &Self) -> bool {
        self.0.iter().all(|(id, (links, withdrawn))| {
            other
                .0
                .get(id)
                .is_some_and(|(held, gone)| links.is_subset(held) && (!withdrawn || *gone))
        })
    }
}

impl Decision {
    fn of(stream: &[Stamp<DecisionEvent>], registers: &DecisionRegisters<'_>) -> Option<Self> {
        let statements: Vec<Statement> = registers
            .content
            .reading::<super::ContentRegister>()
            .into_iter()
            .filter_map(|stamp| stamp.event.body().statement().cloned())
            .collect();
        let decided = stream
            .iter()
            .filter(|stamp| stamp.event.body().statement().is_some())
            .map(|stamp| stamp.event.at())
            .min()?;
        let withdrawn = registers
            .life
            .reading::<super::LifeRegister>()
            .into_iter()
            .find_map(|stamp| match stamp.event.body() {
                Said::Withdrawn { reason } => Some(Withdrawal {
                    at: stamp.event.at(),
                    by: stamp.event.actor().clone(),
                    reason: reason.clone(),
                }),
                Said::Decided { .. } | Said::Revised { .. } => None,
            });
        (!statements.is_empty()).then(|| Self {
            statements,
            decided,
            supersedes: registers
                .supersedes
                .iter()
                .map(|id| (*id).clone())
                .collect(),
            withdrawn,
        })
    }

    /// Every candidate statement: one, or a conflict's.
    #[must_use]
    pub fn statements(&self) -> &[Statement] {
        &self.statements
    }

    /// The statement a reading that can show only one shows: the first candidate, whose write
    /// has the least name, and the reader says when there were more.
    #[must_use]
    pub fn statement(&self) -> &Statement {
        &self.statements[0]
    }

    /// Whether two concurrent revisions both stand.
    #[must_use]
    pub fn is_conflict(&self) -> bool {
        self.statements.len() > 1
    }
}
