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

use crate::chains::ChainName;
use crate::record::{Entry, TodoEvent};

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
    pub fn failed(&self) -> Vec<crate::wire::FailedChain> {
        self.failures()
            .map(|failure| crate::wire::FailedChain {
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
pub fn sooner(a: NextChange, b: NextChange) -> NextChange {
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
    use std::collections::BTreeMap as Specs;

    use prodrome_core::event::{Envelope, Hash, mk_sealed, mk_spec_revised, seal_hash};
    use prodrome_core::fpl::{self, Env, link_in, mk_flat, mk_ref_in};
    use prodrome_core::literal::Datetime;
    use prodrome_core::term::Term;

    use super::*;
    use crate::store::Reads as _;
    use crate::store::{InMemory, Prodrome as _};

    type Objects = Vec<(Hash, Envelope<TodoEvent>)>;

    fn at(hour: u32) -> Datetime {
        Datetime::new(2026, 10, 4, hour, 0, 0, 0).expect("an instant")
    }

    fn spec(todo: &str, term: Term, hour: u32) -> TodoEvent {
        mk_spec_revised(todo, at(hour), "ada", term, "").expect("an event")
    }

    fn priced(todo: &str, value: f64, hour: u32) -> TodoEvent {
        spec(todo, mk_flat(value).expect("a flat"), hour)
    }

    fn name(text: &str) -> ChainName {
        ChainName::new(text).expect("a slug")
    }

    fn legacy(events: Vec<TodoEvent>) -> Objects {
        let mut objects: Objects = Vec::new();
        for event in events {
            let object = mk_sealed(objects.last().map(|(name, _)| name.clone()), event);
            objects.push((seal_hash(&object), object));
        }
        objects
    }

    fn begun(events: Vec<TodoEvent>) -> Objects {
        let store = InMemory::<TodoEvent>::default();
        for event in events {
            store.append(event).expect("appends");
        }
        store.objects().expect("reads")
    }

    fn two_geneses() -> Objects {
        ["one", "two"]
            .into_iter()
            .map(|label| {
                let genesis: Envelope<TodoEvent> = Envelope::Genesis(
                    prodrome_core::genesis::mk_genesis(label, &"0".repeat(32)).expect("a genesis"),
                );
                (seal_hash(&genesis), genesis)
            })
            .collect()
    }

    fn read(chains: Vec<(&str, Objects)>, untrusted: &Untrusted) -> Read {
        Read::of_objects(
            chains
                .into_iter()
                .map(|(chain, objects)| (name(chain), untrusted.clone(), objects)),
        )
    }

    /// What `RefIn(store, todo)` reads through `set` at its instant.
    fn through(set: &ChainSet<'_>, store: &str, todo: &str) -> Option<f64> {
        let term: Term = mk_ref_in(store.to_owned(), todo.to_owned()).expect("a reference");
        let closed = link_in(&term, &Specs::new(), set.stores()).expect("links");
        fpl::fulfillment(&closed, set.instant(), &Env::new())
    }

    fn row(set: &ChainSet<'_>, chain: &str, todo: &str) -> Option<f64> {
        set.entries(&name(chain))
            .expect("rows")
            .iter()
            .find(|row| row.key.as_str() == todo)
            .and_then(|row| row.value().expect("links"))
    }

    /// A legacy chain and a chain a `Genesis` begins are both members, each by its name.
    #[test]
    fn every_chain_is_a_member_by_its_name() {
        let project = begun(vec![priced("alpha", 0.6, 1)]);
        let genesis = Dag::from_iter(project.clone()).geneses().into_iter().next();
        let chains = read(
            vec![
                ("life", legacy(vec![priced("permit", 0.3, 1)])),
                ("suzatary", project),
            ],
            &Untrusted::none(),
        );
        let set = chains.set(at(2));
        assert_eq!(through(&set, "life", "permit"), Some(0.3));
        assert_eq!(through(&set, "suzatary", "alpha"), Some(0.6));
        assert_eq!(through(&set, "life", "alpha"), None, "another chain's todo");
        assert_eq!(
            through(&set, "elsewhere", "permit"),
            None,
            "a chain not held"
        );
        let genesis = genesis.expect("begun");
        assert_eq!(
            through(&set, genesis.as_str(), "alpha"),
            Some(0.6),
            "a member by its genesis too"
        );
    }

    /// Each chain is read under its own policy: an untrusted writer prices nothing in its chain.
    #[test]
    fn each_chain_reads_under_its_own_policy() {
        let mut life = legacy(vec![priced("permit", 0.3, 1)]);
        let claimed = mk_sealed(
            Some(life[0].0.clone()),
            mk_spec_revised("permit", at(2), "triage", mk_flat(0.1).expect("a flat"), "")
                .expect("an event"),
        );
        life.push((seal_hash(&claimed), claimed));
        let read_under = |untrusted: &Untrusted| {
            let chains = read(vec![("life", life.clone())], untrusted);
            through(&chains.set(at(3)), "life", "permit")
        };
        assert_eq!(read_under(&Untrusted::none()), Some(0.1));
        let triage = prodrome_core::event::Actor::new("triage").expect("an actor");
        assert_eq!(read_under(&Untrusted::of([triage])), Some(0.3));
    }

    /// A chain holding two geneses is no member: it is held as that failure, a reference into it
    /// reads `∅`, and no other chain's price moves.
    #[test]
    fn a_broken_member_moves_nothing_that_does_not_refer_into_it() {
        assert_eq!(home(&Dag::from_iter(two_geneses())), Err(2));
        let life = legacy(vec![
            priced("permit", 0.3, 1),
            spec(
                "fee",
                fpl::mk_offset(
                    0.1,
                    mk_ref_in("broken".to_owned(), "x".to_owned()).expect("a ref"),
                )
                .expect("an offset"),
                1,
            ),
        ]);
        let none = Untrusted::none();
        let alone = read(vec![("life", life.clone())], &none);
        let beside = read(vec![("life", life), ("broken", two_geneses())], &none);
        let (alone, beside) = (alone.set(at(2)), beside.set(at(2)));
        assert_eq!(
            row(&beside, "life", "permit"),
            row(&alone, "life", "permit")
        );
        assert_eq!(row(&beside, "life", "fee"), row(&alone, "life", "fee"));
        assert_eq!(through(&beside, "broken", "x"), None);
        assert!(matches!(
            beside.failures().collect::<Vec<_>>().as_slice(),
            [NotAMember::Several(_, 2)]
        ));
    }

    /// A write to one chain re-reads that chain alone: the other is the earlier read's own, its
    /// readings kept, and the set reads at every instant as a fresh read of both stores does.
    #[test]
    fn a_write_to_one_chain_rereads_that_chain_alone() {
        let policy = |chain: &str| crate::chains::ChainPolicy {
            name: name(chain),
            untrusted: Untrusted::none(),
            roster: std::collections::BTreeSet::new(),
            root: crate::cast::keys::root(),
        };
        let (life, project) = (policy("life"), policy("suzatary"));
        let (life_store, project_store) = (
            InMemory::<TodoEvent>::default(),
            InMemory::<TodoEvent>::default(),
        );
        let term = fpl::mk_offset(
            0.1,
            mk_ref_in("suzatary".to_owned(), "alpha".to_owned()).expect("a ref"),
        )
        .expect("an offset");
        life_store.append(spec("permit", term, 1)).expect("appends");
        project_store
            .append(priced("alpha", 0.6, 1))
            .expect("appends");
        let stores = || {
            [
                (&life, &life_store as &dyn crate::store::Prodrome<TodoEvent>),
                (
                    &project,
                    &project_store as &dyn crate::store::Prodrome<TodoEvent>,
                ),
            ]
        };
        let first = Read::of_stores(stores());
        let before = row(&first.set(at(2)), "life", "permit");
        project_store
            .append(priced("alpha", 0.2, 3))
            .expect("appends");
        let second = first.moved(stores());
        assert!(second.shares(&first, &life.name), "life did not move");
        assert!(!second.shares(&first, &project.name), "the project did");
        let fresh = Read::of_stores(stores());
        for hour in [2, 4] {
            for _ in 0..2 {
                assert_eq!(
                    row(&second.set(at(hour)), "life", "permit"),
                    row(&fresh.set(at(hour)), "life", "permit"),
                    "at {hour}h"
                );
            }
        }
        assert_eq!(row(&second.set(at(2)), "life", "permit"), before);
        let after = row(&second.set(at(4)), "life", "permit").expect("a number");
        assert!((after - 0.28).abs() < 1e-9, "{after}");
    }

    /// A chain whose policy alone moved is read again, as a fresh read under that policy reads
    /// it: who is believed is part of what the reading is of.
    #[test]
    fn a_policy_that_moved_is_a_new_reading() {
        let store = InMemory::<TodoEvent>::default();
        store.append(priced("permit", 0.3, 1)).expect("appends");
        store
            .append(
                mk_spec_revised("permit", at(2), "triage", mk_flat(0.1).expect("a flat"), "")
                    .expect("an event"),
            )
            .expect("appends");
        let triage = prodrome_core::event::Actor::new("triage").expect("an actor");
        let policy = |untrusted: Untrusted| crate::chains::ChainPolicy {
            name: name("life"),
            untrusted,
            roster: std::collections::BTreeSet::new(),
            root: crate::cast::keys::root(),
        };
        let (open, closed) = (policy(Untrusted::none()), policy(Untrusted::of([triage])));
        let of = |policy| [(policy, &store as &dyn crate::store::Prodrome<TodoEvent>)];
        let first = Read::of_stores(of(&open));
        let second = first.moved(of(&closed));
        assert!(!second.shares(&first, &open.name), "the policy moved");
        let fresh = Read::of_stores(of(&closed));
        assert_eq!(
            through(&second.set(at(3)), "life", "permit"),
            through(&fresh.set(at(3)), "life", "permit")
        );
        assert_eq!(through(&second.set(at(3)), "life", "permit"), Some(0.3));
        assert_eq!(through(&first.set(at(3)), "life", "permit"), Some(0.1));
    }

    /// A row whose function refers into another chain is priced through the set, as a proposal
    /// serving it is.
    #[test]
    fn a_row_reads_another_chain_through_the_set() {
        let term = fpl::mk_offset(
            0.1,
            mk_ref_in("suzatary".to_owned(), "alpha".to_owned()).expect("a ref"),
        )
        .expect("an offset");
        let chains = read(
            vec![
                ("life", legacy(vec![spec("permit", term, 1)])),
                ("suzatary", begun(vec![priced("alpha", 0.6, 1)])),
            ],
            &Untrusted::none(),
        );
        let set = chains.set(at(2));
        let value = row(&set, "life", "permit").expect("a number");
        assert!((value - 0.64).abs() < 1e-9, "{value}");
        assert_eq!(Some(value), through(&set, "life", "permit"));
    }

    // --- The clock, observed ----------------------------------------------------

    fn hours_after(base: Datetime, hours: f64) -> Datetime {
        // Whole microseconds, the evaluator's grain.
        #[allow(clippy::cast_possible_truncation)]
        let us = (hours * 3_600_000_000.0) as i64;
        fpl::datetime_of(fpl::instant_of(base) + chrono::Duration::microseconds(us))
            .expect("an instant")
    }

    /// Every row of `chain` at `t`, as a whole percent shows it.
    fn shown(read: &Read, chain: &str, t: Datetime) -> Specs<String, Option<u32>> {
        let set = read.set(t);
        set.entries(&name(chain))
            .expect("rows")
            .iter()
            .map(|row| {
                let value = row.value().expect("links");
                (
                    row.key.as_str().to_owned(),
                    Observation::PERCENT.observe(value),
                )
            })
            .collect()
    }

    fn next(read: &Read, chain: &str, t: Datetime) -> NextChange {
        let set = read.set(t);
        let rows = set.entries(&name(chain)).expect("rows");
        set.next(&name(chain), &rows, Observation::PERCENT)
            .expect("held")
    }

    /// One todo's price, by shape: a decay over its own window, the exact fragment, or one the
    /// core answers conservatively, as the live chains hold them (a `Conj` of two, a `Recur`
    /// of its own tendings, a `Within` that widens the span).
    fn priced_as(todo: &str, shape: u8, lead: f64, start: f64, end: f64) -> Term {
        let decay = |lead: f64| {
            fpl::mk_decay(
                start,
                end,
                fpl::instant_of(hours_after(at(0), 2.0 * lead)),
                chrono::Duration::hours(1)
                    + (fpl::instant_of(hours_after(at(0), lead)) - fpl::instant_of(at(0))),
                None,
            )
            .expect("a decay")
        };
        match shape {
            0 => decay(lead),
            1 => fpl::mk_conj(vec![decay(lead), decay(lead / 2.0)], -4.0).expect("a conj"),
            2 => fpl::mk_recur(
                todo.to_owned(),
                fpl::instant_of(at(0)),
                decay(lead),
                decay(lead),
            )
            .expect("a recur"),
            _ => fpl::mk_within(chrono::Duration::hours(6), -4.0, decay(lead)).expect("a within"),
        }
    }

    /// Todos priced over windows of their own, each read through a reference from the other
    /// chain too, so a row priced through the set is scheduled as it is priced.
    fn pricing(todos: &[(u8, f64, f64, f64)]) -> Read {
        let events = todos
            .iter()
            .enumerate()
            .map(|(i, &(shape, lead, start, end))| {
                let todo = format!("t{i}");
                spec(&todo, priced_as(&todo, shape, lead, start, end), 0)
            })
            .collect();
        let follows = mk_ref_in("life".to_owned(), "t0".to_owned()).expect("a reference");
        read(
            vec![
                ("life", legacy(events)),
                ("suzatary", begun(vec![spec("follows", follows, 0)])),
            ],
            &Untrusted::none(),
        )
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

        /// Between the instant a set is read at and its next change, every row of either chain
        /// reads the same whole percent, whatever the clock says; at an exact change a row
        /// reads another, and a conservative one is never later than the first that does.
        #[test]
        fn between_two_observed_changes_every_row_reads_the_same(
            todos in proptest::collection::vec((0u8..4, 1.0f64..48.0, 0.0f64..0.98, 0.0f64..0.9), 1..5),
            from in 0.0f64..120.0,
            within in 0.0f64..1.0,
        ) {
            let read = pricing(&todos);
            let t0 = hours_after(at(0), from);
            for chain in ["life", "suzatary"] {
                let before = shown(&read, chain, t0);
                let answer = next(&read, chain, t0);
                let Some(changes) = answer.at else {
                    proptest::prop_assert!(answer.exact, "never is a proof");
                    // Never again: a thousand days on reads the same.
                    proptest::prop_assert_eq!(&shown(&read, chain, hours_after(t0, 24_000.0 * within)), &before);
                    continue;
                };
                let span = (changes - fpl::instant_of(t0)).num_microseconds().expect("a span");
                proptest::prop_assert!(span > 0, "a change after the instant");
                #[allow(clippy::cast_precision_loss)]
                let t = hours_after(t0, within * (span - 1) as f64 / 3_600_000_000.0);
                proptest::prop_assert_eq!(&shown(&read, chain, t), &before);
                if answer.exact {
                    let then = fpl::datetime_of(changes).expect("an instant");
                    proptest::prop_assert_ne!(&shown(&read, chain, then), &before);
                }
            }
        }

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

    /// An object stamped after the instant is not yet believed, so the reading moves at its
    /// stamp even where no price moves with the clock.
    #[test]
    fn a_stamp_not_yet_believed_is_a_change() {
        let chains = read(
            vec![
                ("life", legacy(vec![priced("permit", 0.3, 1)])),
                (
                    "suzatary",
                    begun(vec![priced("alpha", 0.6, 1), priced("alpha", 0.4, 9)]),
                ),
            ],
            &Untrusted::none(),
        );
        assert_eq!(
            next(&chains, "life", at(2)),
            NextChange {
                at: Some(fpl::instant_of(at(9))),
                exact: true
            }
        );
        assert_eq!(
            next(&chains, "life", at(10)).at,
            None,
            "flat, and every stamp believed"
        );
    }
}
