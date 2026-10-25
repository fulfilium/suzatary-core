//! A proposal is priced by the todo it serves (the Prodrome's SPEC law 41, its design §6.3.1, as
//! this application holds it), and the todo's own row is priced through the same set, so the two
//! are one reading. Over generated histories of two chains (one legacy-rooted, one a `Genesis`
//! begins, each under its own policy, their terms referring within a chain, across chains and to
//! a completion) and an inbox, a proposal serving `todo:life/alpha` reads, at every instant, what
//! `alpha`'s row reads; nothing in the inbox moves it, and before any term refers elsewhere,
//! nothing but `alpha`'s own history does.

use chrono::Duration;
use prodrome_core::dag::Dag;
use prodrome_core::event::{
    Actor, Envelope, Hash, mk_cancelled, mk_completed, mk_reopened, mk_sealed, mk_spec_revised,
    seal_hash,
};
use prodrome_core::fpl::{instant_of, mk_after, mk_decay, mk_flat, mk_offset, mk_ref, mk_ref_in};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Untrusted;
use prodrome_core::term::Term;
use proptest::prelude::*;

use suzatary_core::chain_set::{ChainSet, Read};
use suzatary_core::chains::ChainName;
use suzatary_core::record::TodoEvent;
use suzatary_core::secretary::inbox::{self, ProposalId};
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::store::{Draft, Proposal, Reasons};
use suzatary_core::store::Reads as _;
use suzatary_core::store::{InMemory, Prodrome};
use suzatary_core::wire::Fulfillment;

fn at(hour: i64) -> Datetime {
    let base = Datetime::new(2026, 10, 4, 0, 0, 0, 0).expect("an instant");
    prodrome_core::fpl::datetime_of(instant_of(base) + Duration::hours(hour)).expect("in range")
}

/// What a todo's price may refer to.
#[derive(Debug, Clone)]
enum Referent {
    /// `beta`, in the same chain.
    Here,
    /// A todo of the other chain.
    There(&'static str),
    /// A completion of `beta`, in the same chain.
    Completion,
}

/// What a todo is told: its price revised, or its lifecycle moved.
#[derive(Debug, Clone)]
enum Told {
    Flat(f64),
    Decay { start: f64, end: f64, due: i64 },
    Refers(Referent, f64),
    Completed,
    Cancelled,
    Reopened,
}

/// Who writes: the owner, or a writer one of the two chains does not believe.
#[derive(Debug, Clone, Copy)]
enum By {
    Owner,
    Triage,
    Scout,
}

/// One thing that happens to the two chains and the inbox.
#[derive(Debug, Clone)]
enum Step {
    /// To the served todo, `alpha` in `life`.
    Served(Told, By),
    /// To `beta` in `life`.
    Beside(Told, By),
    /// To a todo of the other chain, `suzatary`, `alpha` (the same id) or `beta`.
    Elsewhere(&'static str, Told, By),
    /// Another proposal asked, serving `todo:suzatary/alpha` or nothing.
    Asked(u8, bool),
    /// The served proposal dismissed.
    Answered(u8),
}

fn a_referent() -> impl Strategy<Value = Referent> {
    prop_oneof![
        Just(Referent::Here),
        prop_oneof![Just("alpha"), Just("beta")].prop_map(Referent::There),
        Just(Referent::Completion),
    ]
}

fn a_told() -> impl Strategy<Value = Told> {
    prop_oneof![
        3 => (0.0..=1.0_f64).prop_map(Told::Flat),
        2 => (0.0..=0.9_f64, 0.0..=0.9_f64, 0..40_i64)
            .prop_map(|(start, end, due)| Told::Decay { start, end, due }),
        2 => (a_referent(), -1.0..=1.0_f64).prop_map(|(to, delta)| Told::Refers(to, delta)),
        1 => Just(Told::Completed),
        1 => Just(Told::Cancelled),
        1 => Just(Told::Reopened),
    ]
}

fn a_writer() -> impl Strategy<Value = By> {
    prop_oneof![3 => Just(By::Owner), 1 => Just(By::Triage), 1 => Just(By::Scout)]
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    prop::collection::vec(
        prop_oneof![
            3 => (a_told(), a_writer()).prop_map(|(t, by)| Step::Served(t, by)),
            2 => (a_told(), a_writer()).prop_map(|(t, by)| Step::Beside(t, by)),
            2 => (prop_oneof![Just("alpha"), Just("beta")], a_told(), a_writer())
                .prop_map(|(todo, t, by)| Step::Elsewhere(todo, t, by)),
            1 => (any::<u8>(), any::<bool>()).prop_map(|(n, serving)| Step::Asked(n, serving)),
            1 => any::<u8>().prop_map(Step::Answered),
        ],
        1..16,
    )
}

/// The chain `told` refers across to from `chain`.
fn other(chain: &str) -> &'static str {
    if chain == "life" { "suzatary" } else { "life" }
}

/// `told` as an event of `todo` in `chain` at `hour`, written by `by`.
fn event(chain: &str, todo: &str, told: &Told, hour: i64, by: By) -> TodoEvent {
    let actor = match by {
        By::Owner => "ada",
        By::Triage => "triage",
        By::Scout => "scout",
    };
    let when = at(hour);
    let spec = |term: Term| mk_spec_revised(todo, when, actor, term, "");
    let offset = |delta: f64, term: Term| mk_offset(delta, term).expect("an offset");
    match told {
        Told::Flat(value) => spec(mk_flat(*value).expect("a flat")),
        Told::Decay { start, end, due } => spec(
            mk_decay(
                *start,
                *end,
                instant_of(at(*due)),
                Duration::hours(10),
                None,
            )
            .expect("a decay"),
        ),
        Told::Refers(Referent::Here, delta) => {
            spec(offset(*delta, mk_ref("beta".to_owned()).expect("a ref")))
        }
        Told::Refers(Referent::There(to), delta) => spec(offset(
            *delta,
            mk_ref_in(other(chain).to_owned(), (*to).to_owned()).expect("a ref"),
        )),
        Told::Refers(Referent::Completion, delta) => spec(
            mk_after(
                "beta".to_owned(),
                instant_of(when),
                mk_flat(delta.abs()).expect("a flat"),
                mk_flat(0.9).expect("a flat"),
                None,
            )
            .expect("an after"),
        ),
        Told::Completed => mk_completed(todo, when, actor, ""),
        Told::Cancelled => mk_cancelled(todo, when, actor, ""),
        Told::Reopened => mk_reopened(todo, when, actor, ""),
    }
    .expect("an event")
}

fn asked(subject: &str, serves: Option<&str>) -> inbox::Asked {
    inbox::Asked {
        proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
        proposal: Proposal::Email(Draft {
            account: "ada@example.com".to_owned(),
            to: vec!["mira@example.com".to_owned()],
            cc: Vec::new(),
            subject: subject.to_owned(),
            body: "Thursday works.".to_owned(),
            attachments: Vec::new(),
        }),
        reasons: Reasons::new(
            "she asked",
            vec!["mail:abcdef012345".parse().expect("a ref")],
        )
        .expect("reasons"),
        asked: "proposal:default/0".parse().expect("a ref"),
        price: serves
            .map(|todo| todo.parse().expect("a ref"))
            .map_or_else(inbox::Price::unpriced, inbox::Price::Serves),
    }
}

/// A third chain beside the two, broken in each way a member can be, or none.
#[derive(Debug, Clone, Copy)]
enum Broken {
    None,
    /// Its store does not read.
    Unread,
    /// An object rests on one it does not hold, so its objects do not order.
    Dangling,
    /// It holds two geneses, so no prodrome is its writers'.
    TwoGeneses,
}

/// Every way a member breaks. A function that does not build is not among them: `fold::member`
/// fails only through constructors that are total on what the store admits.
const BREAKS: [Broken; 3] = [Broken::Unread, Broken::Dangling, Broken::TwoGeneses];

/// A legacy chain whose one object rests on a parent it does not hold.
fn dangling() -> Vec<(Hash, Envelope<TodoEvent>)> {
    let priced = |todo: &str| {
        mk_spec_revised(todo, at(0), "ada", mk_flat(0.5).expect("a flat"), "").expect("an event")
    };
    let missing = seal_hash(&mk_sealed(None, priced("elsewhere")));
    let object = mk_sealed(Some(missing), priced("orphan"));
    vec![(seal_hash(&object), object)]
}

/// The third chain's name.
fn broken_chain() -> ChainName {
    ChainName::new("broken").expect("a slug")
}

/// A chain begun twice: no one prodrome, so a member that fails.
fn two_geneses() -> Vec<(Hash, Envelope<TodoEvent>)> {
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

fn actor(name: &str) -> Actor {
    Actor::new(name).expect("an actor")
}

/// The legacy chain `life`: each object sealed on the one before.
#[derive(Default)]
struct Legacy(Vec<(Hash, Envelope<TodoEvent>)>);

impl Legacy {
    fn push(&mut self, event: TodoEvent) {
        let object = mk_sealed(self.0.last().map(|(name, _)| name.clone()), event);
        self.0.push((seal_hash(&object), object));
    }
}

/// The two chains, each under its own policy: `life` does not believe the triage, `suzatary`
/// does not believe the scout.
struct Chains {
    life: Legacy,
    project: InMemory<TodoEvent>,
    names: (ChainName, ChainName),
    policies: (Untrusted, Untrusted),
}

impl Chains {
    fn new() -> Chains {
        Chains {
            life: Legacy::default(),
            project: InMemory::default(),
            names: (
                ChainName::new("life").expect("a slug"),
                ChainName::new("suzatary").expect("a slug"),
            ),
            policies: (
                Untrusted::of([actor("triage")]),
                Untrusted::of([actor("scout")]),
            ),
        }
    }

    /// The two chains read once, beside a third broken as `broken` says, which nothing here
    /// refers into.
    fn read(&self, broken: Broken) -> Read {
        let mut chains = vec![
            (
                self.names.0.clone(),
                self.policies.0.clone(),
                self.life.0.clone(),
            ),
            (
                self.names.1.clone(),
                self.policies.1.clone(),
                self.project.objects().expect("reads"),
            ),
        ];
        let third = match broken {
            Broken::None | Broken::Unread => None,
            Broken::Dangling => Some(dangling()),
            Broken::TwoGeneses => Some(two_geneses()),
        };
        if let Some(objects) = third {
            chains.push((broken_chain(), Untrusted::none(), objects));
        }
        Read::of_objects(chains)
    }

    /// The set at `hour`, with the third chain held as a store that did not read where `broken`
    /// says so.
    fn set(read: &Read, hour: i64, broken: Broken) -> ChainSet<'_> {
        let set = read.set(at(hour));
        match broken {
            Broken::Unread => set.fail(&broken_chain(), "the store does not read"),
            Broken::None | Broken::Dangling | Broken::TwoGeneses => set,
        }
    }

    /// `alpha`'s value in `life`'s rows at `hour`, read through the set: `None` where it does
    /// not link, `∅` where `life` names no `alpha`.
    fn row(&self, hour: i64, broken: Broken) -> Option<Fulfillment> {
        let read = self.read(broken);
        let rows = Chains::set(&read, hour, broken)
            .entries(&self.names.0)
            .expect("rows");
        match rows.iter().find(|row| row.key.as_str() == "alpha") {
            None => Some(Fulfillment::of_value(None)),
            Some(row) => row.value().ok().map(Fulfillment::of_value),
        }
    }
}

struct World {
    chains: Chains,
    inbox: InMemory<InboxEvent>,
    served: ProposalId,
    /// Whether any term written so far refers to anything but itself.
    referred: bool,
}

/// The instants the price is read at: every hour of the history and past it.
const PROBES: std::ops::Range<i64> = -2..44;

impl World {
    fn new() -> World {
        let inbox = InMemory::default();
        let served = inbox::ask(
            &inbox,
            asked("the served one", Some("todo:life/alpha")),
            &actor("assistant"),
            at(0),
        )
        .expect("asked");
        World {
            chains: Chains::new(),
            inbox,
            served,
            referred: false,
        }
    }

    /// The served proposal's value at `hour`, read as the Inbox reads it, beside a broken member
    /// or not.
    fn price(&self, hour: i64, broken: Broken) -> Option<Fulfillment> {
        let fold = inbox::fold(&self.inbox).expect("the inbox reads");
        let read = self.chains.read(broken);
        inbox::value(
            fold.get(&self.served).expect("asked"),
            &Chains::set(&read, hour, broken),
        )
    }

    fn prices(&self, broken: Broken) -> Vec<Option<Fulfillment>> {
        PROBES.map(|hour| self.price(hour, broken)).collect()
    }

    fn take(&mut self, step: &Step, hour: i64) {
        let refers = |told: &Told| matches!(told, Told::Refers(..));
        match step {
            Step::Served(told, by) => {
                self.referred |= refers(told);
                self.chains
                    .life
                    .push(event("life", "alpha", told, hour, *by));
            }
            Step::Beside(told, by) => {
                self.referred |= refers(told);
                self.chains
                    .life
                    .push(event("life", "beta", told, hour, *by));
            }
            Step::Elsewhere(todo, told, by) => {
                self.referred |= refers(told);
                self.chains
                    .project
                    .append(event("suzatary", todo, told, hour, *by))
                    .expect("appends");
            }
            Step::Asked(n, serving) => {
                inbox::ask(
                    &self.inbox,
                    asked(
                        &format!("another {n}"),
                        serving.then_some("todo:suzatary/alpha"),
                    ),
                    &actor("assistant"),
                    at(hour),
                )
                .expect("asked");
            }
            Step::Answered(n) => {
                // Refused where the machine does not climb: the reading is unchanged either way.
                let _ = inbox::dismiss(
                    &self.inbox,
                    &self.served,
                    &actor("ada"),
                    at(hour),
                    &format!("not now {n}"),
                );
            }
        }
    }
}

/// Two readings agree: both do not link, both `∅`, or numbers within 1e-9.
fn agree(a: Option<Fulfillment>, b: Option<Fulfillment>) -> bool {
    match (a.map(Fulfillment::number), b.map(Fulfillment::number)) {
        (Some(Some(x)), Some(Some(y))) => (x - y).abs() < 1e-9,
        (x, y) => x == y,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// THE LAW. At every instant the served proposal's price is `alpha`'s row's, both read
    /// through one set; the inbox never moves it, nor a member that fails, and before any term
    /// refers elsewhere only a step on `alpha` does.
    #[test]
    fn a_proposal_prices_as_the_todo_it_serves(steps in steps()) {
        let mut world = World::new();
        let mut before = world.prices(Broken::None);
        for (hour, step) in (1..).zip(&steps) {
            world.take(step, hour);
            let now = world.prices(Broken::None);
            for broken in BREAKS {
                prop_assert_eq!(&world.prices(broken), &now, "a member {:?} moved a price", broken);
            }
            for (probe, price) in PROBES.zip(&now) {
                let row = world.chains.row(probe, Broken::None);
                prop_assert!(agree(*price, row), "at {probe}: {price:?} vs {row:?}");
                for broken in BREAKS {
                    let beside = world.chains.row(probe, broken);
                    prop_assert!(agree(row, beside), "a member {broken:?} moved a row at {probe}");
                }
            }
            let inboxed = matches!(step, Step::Asked(..) | Step::Answered(_));
            if inboxed || (!world.referred && !matches!(step, Step::Served(..))) {
                prop_assert_eq!(&now, &before, "{:?} moved the price", step);
            }
            before = now;
        }
    }
}

/// A proposal that serves no todo keeps its own spec, whatever the chains hold.
#[test]
fn a_proposal_serving_nothing_keeps_its_own_spec() {
    let world = World::new();
    let own = inbox::Asked {
        price: inbox::Price::Own(mk_flat(0.4).expect("a flat")),
        ..asked("its own", None)
    };
    let id = inbox::ask(&world.inbox, own, &actor("assistant"), at(0)).expect("asked");
    let fold = inbox::fold(&world.inbox).expect("the inbox reads");
    let value = inbox::value(
        fold.get(&id).expect("asked"),
        &world.chains.read(Broken::None).set(at(1)),
    );
    assert_eq!(value, Some(Fulfillment::Number(0.4)));
}

/// Two devices asked one proposal (one act, so one id) serving two different todos: neither
/// asking descends from the other, both stand, and it prices as the more urgent (`Least`).
#[test]
fn two_askings_that_disagree_price_as_the_more_urgent() {
    let mut chains = Chains::new();
    chains
        .life
        .push(event("life", "alpha", &Told::Flat(0.7), 1, By::Owner));
    chains
        .life
        .push(event("life", "beta", &Told::Flat(0.3), 1, By::Owner));
    let (one, two) = (InMemory::<InboxEvent>::default(), InMemory::default());
    let id = inbox::ask(
        &one,
        asked("same act", Some("todo:life/alpha")),
        &actor("assistant"),
        at(1),
    )
    .expect("asked");
    let again = inbox::ask(
        &two,
        asked("same act", Some("todo:life/beta")),
        &actor("assistant"),
        at(1),
    )
    .expect("asked");
    assert_eq!(id, again, "one act, one proposal");
    let union: Vec<_> = one
        .objects()
        .expect("reads")
        .into_iter()
        .chain(two.objects().expect("reads"))
        .collect();
    let nodes = Dag::from_iter(union).nodes().expect("one history");
    let fold = inbox::Fold::of(&nodes);
    let value = inbox::value(
        fold.get(&id).expect("asked"),
        &chains.read(Broken::None).set(at(2)),
    );
    assert_eq!(value.and_then(Fulfillment::number), Some(0.3));
}

/// Each break is a failure the set names, and the two chains it does not touch stay members.
#[test]
fn every_break_is_a_failure_of_its_own_chain() {
    let chains = Chains::new();
    for broken in BREAKS {
        let read = chains.read(broken);
        let set = Chains::set(&read, 1, broken);
        let failed: Vec<String> = set.failures().map(ToString::to_string).collect();
        assert_eq!(failed.len(), 1, "{broken:?}: {failed:?}");
        assert!(failed[0].contains("broken"), "{broken:?}: {failed:?}");
        assert!(set.held(&chains.names.0).is_ok() && set.held(&chains.names.1).is_ok());
    }
}
