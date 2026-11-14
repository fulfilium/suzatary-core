//! The todo chains as ONE set of prodromes (the Prodrome's design §6.3.1, SPEC law 41): every
//! chain a member keyed by its writers' genesis, read under that chain's own policy, and named by
//! its chain. A qualified reference `RefIn(<chain>, <todo>)` is a variable of this set, so it
//! reads what that todo reads at home, and every reading that prices (a chain's rows, a proposal)
//! prices through this one set, so a row and the proposal that serves it are one number.
//!
//! A member's reading is a value that may be a failure, as a register holds its candidates: a
//! chain whose objects or functions do not read, or that holds two geneses, is held
//! as that failure, a reference into it reads `∅`, and no other member's price moves.
//!
//! The set is the product of its members' readings, [`fold::member`] each, which is what the
//! Prodrome's `fold::stores` is made of. That function reads one replica under one policy and
//! leaves out the legacy prodrome; the box holds one replica per chain, each under its own
//! policy, and its live chains are legacy-rooted, so each member is keyed here as `nest::Set`
//! keys it, by `Dag::geneses` (a legacy prodrome by its least root).

mod read;

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use prodrome_core::dag::Dag;
use prodrome_core::event::Hash;
use prodrome_core::fold;
use prodrome_core::fpl::{self, Instant, Stores};
use prodrome_core::literal::Datetime;
use prodrome_core::observe::{self, NextChange, Observation};
use prodrome_core::policy::Untrusted;
use prodrome_core::registers::{self, Node};

pub use read::Read;

/// A member's reading at one instant: its genesis and its functions and environment there, none
/// for a chain nothing has begun, or why it is no member.
pub type Reading = Result<Option<(Hash, fpl::Member)>, NotAMember>;

/// One chain's last reading, kept with its instant. The same objects read the same at the same
/// instant, so a chain whose objects did not move is folded once per instant however many
/// readings ask; whoever holds this holds it beside exactly those objects.
#[derive(Debug, Default)]
pub struct Remembered(Mutex<Option<(Datetime, Reading)>>);

impl Remembered {
    /// `held`'s reading at `at`: the kept one when it was read at `at`, else read and kept.
    ///
    /// # Errors
    /// [`reading`]'s.
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

/// `held`'s reading at `at`: [`fold::member`] of the prodrome its writers append to, read under
/// its own policy.
///
/// # Errors
/// Why it is no member: two geneses, or functions that do not read.
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
    // `fold::member` refuses only a function its constructors would not build, which no
    // admitted object writes: what it names is objects that do not read.
    fold::member(prodrome, fpl::instant_of(at), held.untrusted)
        .map(|member| Some((genesis, member)))
        .map_err(|failed| NotAMember::Unread(held.name.clone(), failed.to_string()))
}

use suzatary_base::chains::ChainName;
use suzatary_record::record::{Entry, TodoEvent};

/// The todo chains read as one set at one instant: each chain held as it was read, and each
/// member's reading, a price or why it has none.
#[derive(Debug, Clone)]
pub struct ChainSet<'a> {
    at: Datetime,
    stores: Stores,
    members: BTreeMap<ChainName, Member<'a>>,
}

/// One chain in the set: held and priced, held with prices that failed (its rows still read, at
/// its own prices), or not read at all.
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

/// One chain as the set holds it: its name, its objects and whom it does not believe.
#[derive(Debug, Clone, Copy)]
pub struct Held<'a> {
    /// The name the set declares for the prodrome its writers append to.
    pub name: &'a ChainName,
    /// Its objects.
    pub dag: &'a Dag<TodoEvent>,
    /// The same objects in causal order, as the folds read them.
    pub nodes: &'a [Node<TodoEvent>],
    /// Its policy.
    pub untrusted: &'a Untrusted,
}

/// Why a chain is no member: what a reference into it reads as `∅` for.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NotAMember {
    /// Its objects do not read.
    #[error("the chain {0} does not read: {1}")]
    Unread(ChainName, String),
    /// It holds several geneses, so no prodrome is its writers'.
    #[error("the chain {0} holds {1} geneses; a chain is one prodrome")]
    Several(ChainName, usize),
    /// The set does not hold it.
    #[error("no chain named {0} is held")]
    Absent(ChainName),
}

impl NotAMember {
    /// The chain it is about.
    #[must_use]
    pub fn chain(&self) -> &ChainName {
        match self {
            NotAMember::Unread(chain, _)
            | NotAMember::Several(chain, _)
            | NotAMember::Absent(chain) => chain,
        }
    }
}

/// The prodrome a chain's writers append to: its one genesis (a legacy chain's least root),
/// none for a chain not begun. The writer and the set both ask this, so they agree.
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

/// The prodrome a chain's rows read their unqualified references in: [`home`], and none for a
/// chain of several geneses, which is no member. That failure is not lost here: the set holds
/// it ([`reading`] answers it, [`ChainSet::failures`] names it), and the chain's own rows still
/// price within their chain rather than the whole reading failing with it.
fn home_of(dag: &Dag<TodoEvent>) -> Option<Hash> {
    home(dag).unwrap_or(None)
}

impl<'a> ChainSet<'a> {
    /// The set at `at` holding nothing: every reference reads `∅`.
    #[must_use]
    pub fn new(at: Datetime) -> ChainSet<'a> {
        ChainSet {
            at,
            stores: Stores::new(),
            members: BTreeMap::new(),
        }
    }

    /// The set `chains` hold at `at`.
    #[must_use]
    pub fn read(chains: impl IntoIterator<Item = Held<'a>>, at: Datetime) -> ChainSet<'a> {
        chains.into_iter().fold(ChainSet::new(at), ChainSet::hold)
    }

    /// This set with `held` in place of the chain of its name. Never refused: a chain whose
    /// reading fails is held as that failure, and only what refers into it reads `∅`.
    #[must_use]
    pub fn hold(self, held: Held<'a>) -> ChainSet<'a> {
        let read = reading(held, self.at);
        self.hold_as(held, read)
    }

    /// [`ChainSet::hold`], reading `held` through what `remembered` kept of it.
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

    /// This set with the chain `name` held as the failure `why`.
    #[must_use]
    pub fn fail(mut self, name: &ChainName, why: impl Into<String>) -> ChainSet<'a> {
        self.members.insert(
            name.clone(),
            Member::Unread(NotAMember::Unread(name.clone(), why.into())),
        );
        self
    }

    /// The members' readings, what a qualified reference reads.
    #[must_use]
    pub fn stores(&self) -> &Stores {
        &self.stores
    }

    /// The members' readings, kept past the set: what a reading that links many terms at one
    /// instant links them all against.
    #[must_use]
    pub fn into_stores(self) -> Stores {
        self.stores
    }

    /// The instant it was read at.
    #[must_use]
    pub fn at(&self) -> Datetime {
        self.at
    }

    /// The instant as the evaluator spells it.
    #[must_use]
    pub fn instant(&self) -> Instant {
        fpl::instant_of(self.at)
    }

    /// The chain `name` as it was read.
    ///
    /// # Errors
    /// [`NotAMember::Absent`] for a chain the set does not hold, and the failure of one that did
    /// not read.
    pub fn held(&self, name: &ChainName) -> Result<&Held<'a>, NotAMember> {
        self.members
            .get(name)
            .ok_or_else(|| NotAMember::Absent(name.clone()))?
            .held()
            .map_err(Clone::clone)
    }

    /// Every chain whose reading failed, as a page shows it.
    #[must_use]
    pub fn failed(&self) -> Vec<crate::wire::signatures::FailedChain> {
        self.failures()
            .map(|failure| crate::wire::signatures::FailedChain {
                chain: failure.chain().to_string(),
                why: failure.to_string(),
            })
            .collect()
    }

    /// Every chain whose reading failed, and why: what a reference into it reads `∅` for.
    pub fn failures(&self) -> impl Iterator<Item = &NotAMember> {
        self.members.values().filter_map(Member::failure)
    }

    /// The chain `name`'s rows at the set's instant (the Prodrome's `view::entries` of its
    /// objects under its policy), each priced through the whole set: its function linked against
    /// its own prodrome's functions and every member's, and read under its prodrome's environment,
    /// so a qualified reference in it reads as a proposal serving it reads. A chain held as a
    /// failure that still reads keeps its own chain's prices.
    ///
    /// # Errors
    /// [`ChainSet::held`]'s, and the view's refusal of these objects.
    pub fn entries(&self, name: &ChainName) -> Result<Vec<Entry>, NotAMember> {
        let held = self.held(name)?;
        let mut rows = prodrome_core::view::entries(held.nodes, self.at, held.untrusted)
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

    /// The member a row of a chain whose writers' prodrome is `home` is priced under.
    fn member_of(&self, row: &Entry, home: Option<&Hash>) -> Option<&fpl::Member> {
        row.genesis
            .as_ref()
            .or(home)
            .and_then(|genesis| self.stores.member(genesis))
    }

    /// When `row`, one of the chain `name`'s [`ChainSet::entries`], next shows a different price
    /// through `observation`: the next step of the function it was priced by, under the
    /// environment it was read in, and never for a function that does not link (it reads so
    /// until the history moves). `None` for a row of a member the set could not read, which kept
    /// its own chain's price; it is asked again when that chain reads.
    ///
    /// # Errors
    /// [`ChainSet::held`]'s.
    pub fn next_change(
        &self,
        name: &ChainName,
        row: &Entry,
        observation: Observation,
    ) -> Result<Option<NextChange>, NotAMember> {
        let home = home_of(self.held(name)?.dag);
        Ok(self.next_of(row, home.as_ref(), observation))
    }

    /// [`ChainSet::next_change`] for a row of the chain whose writers' prodrome is `home`.
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

    /// When what the chain `name`'s `rows` show next moves after the set's instant, as
    /// `observation` reads a price: the first row's next observed change, or the first stamp
    /// after the instant of an object any member holds, since a fold believes an event from its
    /// `at` on (SPEC §1). The first of several answers that are each never late is never late,
    /// so a reader redrawn then shows nothing stale. It is exact where the first answer is (a
    /// stamp is: an object comes into force there); a conservative one may come before anything
    /// moves.
    ///
    /// # Errors
    /// [`ChainSet::held`]'s.
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

/// The earlier of two answers, and at one instant exact where either is: a meet-semilattice
/// whose top, never and exact, is its identity.
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
    use prodrome_core::fpl;
    use prodrome_core::literal::Datetime;

    use super::*;

    fn at(hour: u32) -> Datetime {
        Datetime::new(2026, 10, 4, hour, 0, 0, 0).expect("an instant")
    }

    fn an_answer() -> impl proptest::strategy::Strategy<Value = NextChange> {
        use proptest::prelude::*;
        (proptest::option::of(0i64..20), any::<bool>()).prop_map(|(hours, exact)| NextChange {
            at: hours.map(|h| fpl::instant_of(at(0)) + chrono::Duration::hours(h)),
            // Never is only ever a proof.
            exact: exact || hours.is_none(),
        })
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        /// `sooner` is a meet: associative, commutative, idempotent, never-and-exact its
        /// identity, and it answers the earliest instant.
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
