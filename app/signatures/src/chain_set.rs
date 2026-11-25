//! The todo chains as one set of prodromes (design §6.3.1, SPEC law 41), each member keyed by its
//! writers' genesis and read under its own policy. Every price goes through this one set, so a
//! row and the proposal that serves it are one number.
//!
//! A chain that does not read is held as that failure: a reference into it reads `∅`, and no
//! other member's price moves.
//!
//! The Prodrome's `fold::stores` takes one policy and leaves out the legacy prodrome, so each
//! member is keyed here as `nest::Set` keys it, by `Dag::geneses`.

mod read;

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use prodrome_dag::dag::Dag;
use prodrome_dag::registers::{self, Node};
use prodrome_data::event::Hash;
use prodrome_data::instant::Instant;
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Untrusted;
use prodrome_fpl::fpl::{self, Stores};
use prodrome_fpl::observe::{self, NextChange, Observation};
use prodrome_reading::fold;

pub use read::Read;

/// A member's reading at one instant; `None` for a chain nothing has begun.
pub type Reading = Result<Option<(Hash, fpl::Member)>, NotAMember>;

/// One chain's last reading, kept with its instant; hold it beside exactly the objects it read.
#[derive(Debug, Default)]
pub struct Remembered(Mutex<Option<(Datetime, Reading)>>);

impl Remembered {
    pub fn reading(&self, held: Held<'_>, at: Datetime) -> Reading {
        let mut kept = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((when, reading)) = kept.as_ref()
            && *when == at
        {
            return reading.clone();
        }
        let read = reading(held, at);
        *kept = Some((at, read.clone()));
        read
    }
}

/// [`fold::member`] of the prodrome `held`'s writers append to.
pub fn reading(held: Held<'_>, at: Datetime) -> Reading {
    let Some(genesis) =
        home(held.dag).map_err(|several| NotAMember::Several(held.name.clone(), several))?
    else {
        return Ok(None);
    };
    let folded = registers::fold(held.nodes);
    let Some(prodrome) = folded.prodromes().get(&held.dag.key(&genesis)) else {
        return Ok(None);
    };
    // `fold::member` refuses only functions no admitted object writes: the objects do not read.
    fold::member(prodrome, fpl::instant_of(at), held.untrusted)
        .map(|member| Some((genesis, member)))
        .map_err(|failed| NotAMember::Unread(held.name.clone(), failed.to_string()))
}

use suzatary_base::chains::ChainName;
use suzatary_record::record::{Entry, TodoEvent};

/// The todo chains read as one set at one instant.
#[derive(Debug, Clone)]
pub struct ChainSet<'a> {
    at: Datetime,
    stores: Stores,
    members: BTreeMap<ChainName, Member<'a>>,
}

/// A `Failed` chain's rows still read, at its own prices.
#[derive(Debug, Clone)]
enum Member<'a> {
    Priced(Held<'a>),
    Failed(Held<'a>, NotAMember),
    Unread(NotAMember),
}

impl<'a> Member<'a> {
    fn held(&self) -> Result<&Held<'a>, &NotAMember> {
        match self {
            Member::Priced(held) | Member::Failed(held, _) => Ok(held),
            Member::Unread(why) => Err(why),
        }
    }

    fn failure(&self) -> Option<&NotAMember> {
        match self {
            Member::Priced(_) => None,
            Member::Failed(_, why) | Member::Unread(why) => Some(why),
        }
    }
}

/// One chain as the set holds it.
#[derive(Debug, Clone, Copy)]
pub struct Held<'a> {
    pub name: &'a ChainName,
    pub dag: &'a Dag<TodoEvent>,
    /// The same objects in causal order.
    pub nodes: &'a [Node<TodoEvent>],
    pub untrusted: &'a Untrusted,
}

/// Why a chain is no member, so a reference into it reads `∅`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NotAMember {
    #[error("the chain {0} does not read: {1}")]
    Unread(ChainName, String),
    #[error("the chain {0} holds {1} geneses; a chain is one prodrome")]
    Several(ChainName, usize),
    #[error("no chain named {0} is held")]
    Absent(ChainName),
}

impl NotAMember {
    #[must_use]
    pub fn chain(&self) -> &ChainName {
        match self {
            NotAMember::Unread(chain, _)
            | NotAMember::Several(chain, _)
            | NotAMember::Absent(chain) => chain,
        }
    }
}

/// The prodrome a chain's writers append to; the writer and the set both ask this, so they agree.
///
/// # Errors
/// The number of geneses, where there are several.
pub fn home(dag: &Dag<TodoEvent>) -> Result<Option<Hash>, usize> {
    let geneses = dag.geneses();
    match geneses.len() {
        0 | 1 => Ok(geneses.into_iter().next()),
        several => Err(several),
    }
}

/// [`home`], or none for several geneses; [`ChainSet::failures`] still names that failure.
fn home_of(dag: &Dag<TodoEvent>) -> Option<Hash> {
    home(dag).unwrap_or(None)
}

impl<'a> ChainSet<'a> {
    #[must_use]
    pub fn new(at: Datetime) -> ChainSet<'a> {
        ChainSet {
            at,
            stores: Stores::new(),
            members: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn read(chains: impl IntoIterator<Item = Held<'a>>, at: Datetime) -> ChainSet<'a> {
        chains.into_iter().fold(ChainSet::new(at), ChainSet::hold)
    }

    /// Never refused: a chain whose reading fails is held as that failure.
    #[must_use]
    pub fn hold(self, held: Held<'a>) -> ChainSet<'a> {
        let read = reading(held, self.at);
        self.hold_as(held, read)
    }

    #[must_use]
    pub fn hold_remembered(self, held: Held<'a>, remembered: &Remembered) -> ChainSet<'a> {
        let read = remembered.reading(held, self.at);
        self.hold_as(held, read)
    }

    fn hold_as(mut self, held: Held<'a>, read: Reading) -> ChainSet<'a> {
        let member = match read {
            Ok(Some((genesis, member))) => {
                self.stores = std::mem::take(&mut self.stores)
                    .with(genesis.clone(), member)
                    .named(held.name.as_str(), genesis);
                Member::Priced(held)
            }
            Ok(None) => Member::Priced(held),
            Err(failure) => Member::Failed(held, failure),
        };
        self.members.insert(held.name.clone(), member);
        self
    }

    #[must_use]
    pub fn fail(mut self, name: &ChainName, why: impl Into<String>) -> ChainSet<'a> {
        self.members.insert(
            name.clone(),
            Member::Unread(NotAMember::Unread(name.clone(), why.into())),
        );
        self
    }

    #[must_use]
    pub fn stores(&self) -> &Stores {
        &self.stores
    }

    #[must_use]
    pub fn into_stores(self) -> Stores {
        self.stores
    }

    #[must_use]
    pub fn at(&self) -> Datetime {
        self.at
    }

    #[must_use]
    pub fn instant(&self) -> Instant {
        fpl::instant_of(self.at)
    }

    pub fn held(&self, name: &ChainName) -> Result<&Held<'a>, NotAMember> {
        self.members
            .get(name)
            .ok_or_else(|| NotAMember::Absent(name.clone()))?
            .held()
            .map_err(Clone::clone)
    }

    #[must_use]
    pub fn failed(&self) -> Vec<crate::wire::signatures::FailedChain> {
        self.failures()
            .map(|failure| crate::wire::signatures::FailedChain {
                chain: failure.chain().to_string(),
                why: failure.to_string(),
            })
            .collect()
    }

    pub fn failures(&self) -> impl Iterator<Item = &NotAMember> {
        self.members.values().filter_map(Member::failure)
    }

    /// The chain's `view::entries`, each priced through the whole set; a failed chain keeps its
    /// own prices.
    pub fn entries(&self, name: &ChainName) -> Result<Vec<Entry>, NotAMember> {
        let held = self.held(name)?;
        let mut rows = prodrome_reading::view::entries(held.nodes, self.at, held.untrusted)
            .map_err(|failed| NotAMember::Unread(name.clone(), failed.to_string()))?;
        let home = home_of(held.dag);
        let now = self.instant();
        for row in &mut rows {
            let Some(member) = self.member_of(row, home.as_ref()) else {
                continue;
            };
            let linked = fpl::link_in(&row.price.spec, &member.specs, &self.stores);
            row.price.value = linked
                .as_ref()
                .map(|closed| fpl::fulfillment(closed, now, &member.env))
                .map_err(Clone::clone);
            row.price.linked = linked;
        }
        Ok(rows)
    }

    fn member_of(&self, row: &Entry, home: Option<&Hash>) -> Option<&fpl::Member> {
        row.genesis
            .as_ref()
            .or(home)
            .and_then(|genesis| self.stores.member(genesis))
    }

    /// When `row` next shows a different price; never for an unlinked function, and `None` for
    /// a row of an unread member.
    pub fn next_change(
        &self,
        name: &ChainName,
        row: &Entry,
        observation: Observation,
    ) -> Result<Option<NextChange>, NotAMember> {
        let home = home_of(self.held(name)?.dag);
        Ok(self.next_of(row, home.as_ref(), observation))
    }

    fn next_of(
        &self,
        row: &Entry,
        home: Option<&Hash>,
        observation: Observation,
    ) -> Option<NextChange> {
        self.member_of(row, home).map(|member| {
            row.linked().map_or(
                NextChange {
                    at: None,
                    exact: true,
                },
                |closed| observe::next_change(closed, self.instant(), &member.env, observation),
            )
        })
    }

    /// The first of every row's next change and every later object stamp, since a fold believes
    /// an event from its `at` on (SPEC §1); never late, possibly early.
    pub fn next(
        &self,
        name: &ChainName,
        rows: &[Entry],
        observation: Observation,
    ) -> Result<NextChange, NotAMember> {
        let now = self.instant();
        let stamped = NextChange {
            at: self
                .members
                .values()
                .filter_map(|member| member.held().ok())
                .flat_map(|held| held.nodes)
                .filter_map(|node| node.event().map(|event| fpl::instant_of(event.at())))
                .filter(|at| *at > now)
                .min(),
            exact: true,
        };
        let home = home_of(self.held(name)?.dag);
        Ok(rows.iter().fold(stamped, |first, row| {
            self.next_of(row, home.as_ref(), observation)
                .map_or(first, |next| sooner(first, next))
        }))
    }
}

/// The earlier of two answers, exact at a tie where either is; never-and-exact is the identity.
#[must_use]
pub(crate) fn sooner(a: NextChange, b: NextChange) -> NextChange {
    match (a.at, b.at) {
        (Some(x), Some(y)) if x < y => a,
        (Some(x), Some(y)) if y < x => b,
        (Some(x), Some(_)) => NextChange {
            at: Some(x),
            exact: a.exact || b.exact,
        },
        (Some(_), None) => a,
        (None, Some(_)) => b,
        (None, None) => NextChange {
            at: None,
            exact: a.exact && b.exact,
        },
    }
}

#[cfg(test)]
mod tests {
    use prodrome_data::literal::Datetime;
    use prodrome_fpl::fpl;

    use super::*;

    fn at(hour: u32) -> Datetime {
        Datetime::new(2026, 10, 4, hour, 0, 0, 0).expect("an instant")
    }

    fn an_answer() -> impl proptest::strategy::Strategy<Value = NextChange> {
        use proptest::prelude::*;
        (proptest::option::of(0i64..20), any::<bool>()).prop_map(|(hours, exact)| NextChange {
            at: hours.map(|h| fpl::instant_of(at(0)) + chrono::Duration::hours(h)),
            exact: exact || hours.is_none(),
        })
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        /// `sooner` is a meet with never-and-exact as identity, answering the earliest instant.
        #[test]
        fn sooner_is_a_meet(a in an_answer(), b in an_answer(), c in an_answer()) {
            let never = NextChange { at: None, exact: true };
            proptest::prop_assert_eq!(sooner(a, sooner(b, c)), sooner(sooner(a, b), c));
            proptest::prop_assert_eq!(sooner(a, b), sooner(b, a));
            proptest::prop_assert_eq!(sooner(a, a), a);
            proptest::prop_assert_eq!(sooner(a, never), a);
            let first = [a.at, b.at].into_iter().flatten().min();
            proptest::prop_assert_eq!(sooner(a, b).at, first);
        }
    }
}
