//! The memory schema's laws (`docs/design-register-types.md` §7 in the Prodrome): a fact's life
//! is a partial order (law 2); the reading is a function of the object set, whatever order the
//! objects arrive in (law 1); the store's real append refuses a remembering of a forgotten fact,
//! so a forgotten id never reads again, and the reading of a union is the join of the readings
//! (law 4); every event prints and parses back. Consolidations ride in every script, so a
//! consolidation arriving before its sources, or beside a forgetting of one of them on another
//! replica, reads the same whatever the order, and never stands on a forgotten fact.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use prodrome_core::dag::Dag;
use prodrome_core::event::{Actor, Envelope, Hash, canonical, canonical_envelope, parse_event};
use prodrome_core::fold::{Discrete, Order};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Everything;
use prodrome_core::registers::Node;
use prodrome_core::store::EventStore;
use proptest::prelude::*;

use suzatary_core::secretary::fact_life::Life;
use suzatary_core::secretary::memory::{FactId, Kind, Origin};
use suzatary_core::secretary::memory_event::Said;
use suzatary_core::secretary::memory_event::{self, MemoryEvent, MemoryVocabulary};
use suzatary_core::secretary::provenance::Wrote;
use suzatary_core::secretary::salted_text::TextName;
use suzatary_core::secretary::store::ConversationId;
use suzatary_core::secretary::two_phase::TwoPhase;
use suzatary_core::store::linearised;

type Disk = EventStore<MemoryEvent, Everything>;

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A store at the memory schema, removed when dropped.
struct Scratch(Disk);

impl Scratch {
    fn new() -> Self {
        let scratch = Self::empty();
        scratch.0.init("memory").expect("begins");
        scratch
    }

    /// A store with nothing in it, not even a genesis: a replica about to adopt one.
    fn empty() -> Self {
        let root: PathBuf = std::env::temp_dir().join(format!(
            "suzatary-memory-schema-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        Self(Disk::new(root, Everything))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.root());
    }
}

fn at(minute: u64) -> Datetime {
    let minute = u32::try_from(minute % 600).expect("a minute");
    Datetime::new(2026, 10, 2, 8 + minute / 60, minute % 60, 0, 0).expect("an instant")
}

fn actor(who: &str) -> Actor {
    Actor::new(who).expect("an actor")
}

fn name(seed: u64) -> TextName {
    TextName::new(format!("{seed:064x}")).expect("a name")
}

const KINDS: [Kind; 4] = Kind::ALL;

fn remembered(fact: u64, seed: u64) -> MemoryEvent {
    memory_event::remembered(
        FactId(fact),
        at(fact),
        actor("assistant"),
        name(seed),
        KINDS[usize::try_from(seed % 4).expect("small")],
        if seed.is_multiple_of(2) {
            Origin::Turn {
                conversation: ConversationId::new("default").expect("a slug"),
                turn: usize::try_from(seed % 7).expect("small"),
            }
        } else {
            Origin::Tapped {
                conversation: ConversationId::new("2026-10-02-1").expect("a slug"),
                proposal: usize::try_from(seed % 5).expect("small"),
            }
        },
    )
    .expect("a remembering")
}

fn consolidated(fact: u64, from: &[u64], seed: u64) -> MemoryEvent {
    memory_event::consolidated(
        FactId(fact),
        at(fact),
        actor("assistant"),
        from.iter().copied().map(FactId).collect(),
        name(seed),
        KINDS[usize::try_from(seed % 4).expect("small")],
        Origin::Turn {
            conversation: ConversationId::new("default").expect("a slug"),
            turn: usize::try_from(seed % 3).expect("small"),
        },
    )
    .expect("after its sources")
}

fn forgotten(fact: u64, version: u64) -> MemoryEvent {
    memory_event::forgotten(FactId(fact), at(version), actor("ada"), version).expect("after it")
}

/// What the box writes: a remembering of a new fact, a forgetting of a standing one, or a
/// consolidation of two standing ones.
#[derive(Debug, Clone)]
enum Step {
    Remember(u64),
    Forget(usize),
    Consolidate(usize, usize, u64),
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    proptest::collection::vec(
        prop_oneof![
            (0u64..1000).prop_map(Step::Remember),
            (0usize..40).prop_map(Step::Forget),
            (0usize..40, 0usize..40, 0u64..1000)
                .prop_map(|(a, b, seed)| Step::Consolidate(a, b, seed)),
        ],
        1..40,
    )
}

/// The events `script` makes, numbered as `memory::remembering` and `memory::forgetting` number
/// them: each one more than every id and version before it.
fn history(script: &[Step]) -> Vec<MemoryEvent> {
    let mut version = 0;
    let mut standing: Vec<u64> = Vec::new();
    let mut events = Vec::new();
    for step in script {
        match step {
            Step::Remember(seed) => {
                version += 1;
                standing.push(version);
                events.push(remembered(version, *seed));
            }
            Step::Forget(pick) if !standing.is_empty() => {
                let fact = standing.remove(pick % standing.len());
                version += 1;
                events.push(forgotten(fact, version));
            }
            Step::Consolidate(a, b, seed) if !standing.is_empty() => {
                let mut from = vec![standing[a % standing.len()], standing[b % standing.len()]];
                from.sort_unstable();
                from.dedup();
                standing.retain(|fact| !from.contains(fact));
                version += 1;
                standing.push(version);
                events.push(consolidated(version, &from, *seed));
            }
            Step::Forget(_) | Step::Consolidate(..) => {}
        }
    }
    events
}

fn written(events: &[MemoryEvent]) -> Scratch {
    let scratch = Scratch::new();
    for event in events {
        scratch.0.append(event.clone()).expect("climbs");
    }
    scratch
}

fn objects(store: &Disk) -> Vec<(Hash, Envelope<MemoryEvent>)> {
    linearised(store).expect("linearised")
}

fn read(objects: Vec<(Hash, Envelope<MemoryEvent>)>) -> TwoPhase {
    let nodes: Vec<Node<MemoryEvent>> = Dag::from_iter(objects).nodes().expect("a prodrome");
    TwoPhase::of(&nodes)
}

/// `objects` in an order a replica could receive them, every parent before its child, drawn
/// from `seed`.
fn arrival(
    objects: &[(Hash, Envelope<MemoryEvent>)],
    mut seed: u64,
) -> Vec<(Hash, Envelope<MemoryEvent>)> {
    let mut waiting: Vec<(Hash, Envelope<MemoryEvent>)> = objects.to_vec();
    let mut arrived: Vec<(Hash, Envelope<MemoryEvent>)> = Vec::new();
    while !waiting.is_empty() {
        let ready: Vec<usize> = (0..waiting.len())
            .filter(|&at| {
                prodrome_core::event::parents_of(&waiting[at].1)
                    .iter()
                    .all(|parent| arrived.iter().any(|(held, _)| held == parent))
            })
            .collect();
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let pick = ready[usize::try_from(seed >> 33).expect("fits") % ready.len()];
        arrived.push(waiting.remove(pick));
    }
    arrived
}

fn adopt(to: &Disk, from: &Disk) {
    let prints: BTreeMap<Hash, String> = objects(from)
        .iter()
        .map(|(name, object)| (name.clone(), canonical_envelope(object)))
        .collect();
    for tip in from.tips().expect("tips") {
        to.adopt_objects(&prints, &tip).expect("adopted");
    }
}

fn lives(events: &[MemoryEvent]) -> Vec<Life<'_>> {
    events
        .iter()
        .map(|event| match event.body() {
            suzatary_core::secretary::memory_event::Said::Remembered { .. }
            | suzatary_core::secretary::memory_event::Said::Consolidated { .. } => {
                Life::Remembered(Discrete(event))
            }
            suzatary_core::secretary::memory_event::Said::Forgotten { .. } => Life::Forgotten,
        })
        .collect()
}

proptest! {
    /// A fact's life is a partial order, with forgotten the top.
    #[test]
    fn a_life_is_a_partial_order(script in steps()) {
        let events = history(&script);
        let values = lives(&events);
        crate::laws::partial_order(&values)?;
        for a in &values {
            prop_assert!(a.le(&Life::Forgotten));
        }
    }

    /// Every event prints and parses back to itself.
    #[test]
    fn every_event_prints_and_parses_back(script in steps()) {
        for event in history(&script) {
            let print = canonical(&event);
            prop_assert_eq!(parse_event::<MemoryEvent>(&MemoryVocabulary::default(), &print).expect("parses"), event);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// The reading is a function of the object set: every order a replica could receive the
    /// objects in reads the same, and every prefix of that arrival holds at most the whole, a
    /// forgotten id never standing again once its forgetting has arrived; what the prefixes of
    /// two arrivals hold is a partial order.
    #[test]
    fn the_reading_is_the_object_sets_whatever_order_they_arrive_in(
        script in steps(),
        seed in any::<u64>(),
        other in any::<u64>(),
    ) {
        let events = history(&script);
        let store = written(&events);
        let all = objects(&store.0);
        let whole = read(all.clone());
        let arrived = arrival(&all, seed);
        prop_assert_eq!(&read(arrived.clone()), &whole);
        let mut forgotten_so_far = std::collections::BTreeSet::new();
        let mut held = Vec::new();
        for end in 1..=arrived.len() {
            let prefix = read(arrived[..end].to_vec());
            prop_assert!(prefix.held().le(&whole.held()));
            held.push(prefix.held());
            forgotten_so_far.extend(prefix.forgotten().keys().copied());
            prop_assert!(
                prefix.live().all(|(fact, _)| !forgotten_so_far.contains(&fact)),
                "a forgotten fact stands again"
            );
            for &down in &forgotten_so_far {
                let taken = prefix.forgotten_with(down);
                prop_assert!(
                    prefix.live().all(|(fact, _)| !taken.contains_key(&fact)),
                    "a fact standing on forgotten fact {} stands", down
                );
            }
        }
        // A second arrival's prefixes, so pairs no one arrival orders are compared too.
        let second = arrival(&all, other);
        held.extend((1..=second.len()).map(|end| read(second[..end].to_vec()).held()));
        crate::laws::partial_order(&held)?;
        prop_assert!(store.0.verify().is_empty(), "{:?}", store.0.verify());
    }

    /// Once forgotten, always forgotten: the store refuses any remembering of a forgotten id,
    /// whatever words or kind it names, and writes nothing.
    #[test]
    fn the_store_refuses_to_remember_a_forgotten_fact(script in steps(), seed in any::<u64>()) {
        let events = history(&script);
        let store = written(&events);
        let before = objects(&store.0).len();
        let reading = read(objects(&store.0));
        for &fact in reading.forgotten().keys() {
            let mut again = remembered(fact.0, seed);
            prop_assert!(store.0.append(again.clone()).is_err());
            again = remembered(fact.0, seed.wrapping_add(1));
            prop_assert!(store.0.append(again).is_err());
        }
        // Beside a standing remembering, another is refused, or is the very event held, which
        // the store writes nothing for.
        for (fact, _) in reading.live() {
            let _ = store.0.append(remembered(fact.0, seed));
        }
        prop_assert_eq!(objects(&store.0).len(), before);
        prop_assert_eq!(read(objects(&store.0)), reading);
    }

    /// Two replicas share a prefix; each remembers facts the other does not see (ids the other
    /// does not use), consolidates shared facts, and forgets one of them; adopting each other's
    /// objects, both hold the join of what each held, and the join is the least upper bound.
    ///
    /// The union reads law 11's two clauses: a fact stands exactly when its sources are held,
    /// nothing it rests on is forgotten and nothing replaced it; a consolidation that reads
    /// forgotten through a source replaces nothing, so its other sources stand again. When both
    /// replicas number their consolidations alike, the least-named one stands, the other's
    /// sources are replaced by nothing, and its words are named by nothing live.
    #[test]
    fn the_union_reads_the_join(
        script in steps(),
        mine in proptest::collection::vec(0u64..1000, 0..5),
        theirs in proptest::collection::vec(0u64..1000, 0..5),
        drop_mine in 0usize..40,
        drop_theirs in 0usize..40,
        merge_mine in proptest::option::weighted(0.9, (0usize..40, 0usize..40)),
        merge_theirs in proptest::option::weighted(0.9, (0usize..40, 0usize..40)),
        same_number in any::<bool>(),
    ) {
        let shared = history(&script);
        let one = written(&shared);
        let two = Scratch::empty();
        adopt(&two.0, &one.0);
        let reading = read(objects(&one.0));
        let standing: Vec<FactId> = reading.live().map(|(fact, _)| fact).collect();
        let top = reading.version();
        for (store, seeds, base, drop, merge) in [
            (&one, &mine, 1000, drop_mine, merge_mine),
            (&two, &theirs, 2000, drop_theirs, merge_theirs),
        ] {
            let mut next = top + base;
            for seed in seeds {
                next += 1;
                store.0.append(remembered(next, *seed)).expect("climbs");
            }
            // A consolidation of shared facts, beside the other replica's forgetting of one.
            let dropped = standing.get(drop % standing.len().max(1)).copied();
            if let (Some((a, b)), false) = (merge, standing.is_empty()) {
                let mut from: Vec<u64> = [a, b].iter().map(|pick| standing[pick % standing.len()].0).collect();
                from.sort_unstable();
                from.dedup();
                if dropped.is_none_or(|fact| !from.contains(&fact.0)) {
                    next += 1;
                    let number = if same_number { top + 1 } else { next };
                    store.0.append(consolidated(number, &from, next)).expect("climbs");
                }
            }
            if let Some(fact) = dropped {
                next += 1;
                store.0.append(forgotten(fact.0, next)).expect("climbs");
            }
        }
        let (a, b) = (read(objects(&one.0)), read(objects(&two.0)));
        let joined = a.clone().join(b.clone());
        adopt(&one.0, &two.0);
        adopt(&two.0, &one.0);
        let (after_one, after_two) = (read(objects(&one.0)), read(objects(&two.0)));
        prop_assert_eq!(&after_one, &after_two);
        prop_assert_eq!(after_one.held(), joined.held());
        crate::laws::join(&[reading, a, b, after_one.clone()], TwoPhase::held, TwoPhase::join)?;
        let live = |phase: &TwoPhase| phase.live().map(|(fact, _)| fact).collect::<Vec<_>>();
        prop_assert_eq!(live(&after_one), live(&joined));
        prop_assert_eq!(after_one.naming().dead(), joined.naming().dead());
        for &down in after_one.forgotten().keys() {
            let taken = after_one.forgotten_with(down);
            prop_assert!(
                after_one.live().all(|(fact, _)| !taken.contains_key(&fact)),
                "a consolidation of forgotten fact {} stands after the union", down
            );
        }
        let standing: BTreeSet<FactId> = after_one.live().map(|(fact, _)| fact).collect();
        for (&fact, kept) in after_one.remembered() {
            let stands = after_one.provenance(fact).is_some()
                && after_one.forgetting(fact).is_none()
                && !after_one.replaced().contains_key(&fact);
            prop_assert_eq!(standing.contains(&fact), stands, "fact {}", fact);
            if let Wrote::Merged { from, .. } = &kept.wrote {
                let through_a_source = after_one.forgetting(fact).is_some()
                    && !after_one.forgotten().contains_key(&fact);
                if through_a_source {
                    for source in from {
                        prop_assert!(
                            !after_one.replaced().get(source).is_some_and(|by| by.contains(&fact)),
                            "a consolidation read forgotten through a source replaces nothing"
                        );
                    }
                } else {
                    prop_assert!(from.is_disjoint(&standing), "no source stands beside its consolidation");
                }
            }
        }
        if same_number {
            // Every remembering's words but the one kept per number are named, and dead.
            let kept: BTreeSet<_> = after_one.remembered().values().map(|kept| kept.text.clone()).collect();
            let named = after_one.naming();
            for (_, object) in objects(&one.0) {
                if let Some(text) = object.event().and_then(|event| match event.body() {
                        Said::Remembered { text, .. } | Said::Consolidated { text, .. } => Some(text.clone()),
                        Said::Forgotten { .. } => None,
                    })
                    && !kept.contains(&text)
                {
                    prop_assert!(named.dead().contains(&text), "an outranked remembering's words are dead");
                }
            }
        }
    }
}

/// Two replicas consolidate shared facts under one new number: after the union, the least-named
/// consolidation stands in its sources' place, the other's sources are replaced by nothing and
/// stand, and the other's words are dead.
#[test]
fn two_consolidations_numbered_alike_keep_the_least_named() {
    let one = written(&[remembered(1, 0), remembered(2, 2), remembered(3, 4)]);
    let two = Scratch::empty();
    adopt(&two.0, &one.0);
    one.0.append(consolidated(4, &[1, 2], 10)).expect("climbs");
    two.0.append(consolidated(4, &[2, 3], 11)).expect("climbs");
    adopt(&one.0, &two.0);
    adopt(&two.0, &one.0);
    let (after_one, after_two) = (read(objects(&one.0)), read(objects(&two.0)));
    assert_eq!(after_one, after_two);
    let kept = &after_one.remembered()[&FactId(4)];
    let (won, lost, freed) = match &kept.wrote {
        Wrote::Merged { from, .. } if from.contains(&FactId(1)) => (name(10), name(11), FactId(3)),
        Wrote::Merged { .. } => (name(11), name(10), FactId(1)),
        Wrote::Said(_) => panic!("fact 4 is a consolidation"),
    };
    assert_eq!(kept.text, won);
    let live: BTreeSet<FactId> = after_one.live().map(|(fact, _)| fact).collect();
    assert_eq!(
        live,
        [freed, FactId(4)].into(),
        "the loser's other source stands again"
    );
    let dead = after_one.naming().dead();
    assert!(dead.contains(&lost), "the outranked words are dead");
    assert!(!dead.contains(&won));
}
