//! The memory's laws over any interleaving of remembers, consolidations and forgets, through its
//! real store (a prodrome and a text store in memory): the memory is a plain map of its standing
//! facts, the cap holds over the reading, no object ever holds a fact's words, a forgotten fact's
//! words are deleted, and a consolidation replaces its sources (law 11).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;
use proptest::prelude::*;
use suzatary_core::capability::Entropy;
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::memories::Memories;
use suzatary_core::secretary::memory::{self, Fact, FactId, Kind, MAX_LIVE, Origin};
use suzatary_core::secretary::memory_event::MemoryEvent;
use suzatary_core::secretary::salted_text::{Text, TextName};
use suzatary_core::secretary::store::ConversationId;
use suzatary_core::store::Reads as _;
use suzatary_core::store::{Gets as _, InMemory, InMemoryDoc};

/// Each salt the next number.
#[derive(Default)]
struct Counting(AtomicU64);

impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(0);
        bytes[..8].copy_from_slice(&self.0.fetch_add(1, Ordering::Relaxed).to_le_bytes());
        Ok(())
    }
}

/// A memory held in memory.
#[derive(Default)]
struct Held {
    history: InMemory<MemoryEvent>,
    texts: InMemoryDoc<TextName, Text<Fact>>,
    cards: InMemory<InboxEvent>,
    entropy: Counting,
    lines: suzatary_core::secretary::conversations::InMemory,
    owners: std::collections::BTreeSet<prodrome_core::event::Actor>,
    digests: InMemory<suzatary_core::secretary::digest_event::DigestEvent>,
    digest_texts: InMemoryDoc<TextName, Text<suzatary_core::secretary::digest_line::Line>>,
}

impl Held {
    fn memories(&self) -> Memories<'_> {
        Memories {
            history: &self.history,
            texts: &self.texts,
            cards: &self.cards,
            entropy: &self.entropy,
            digests: suzatary_core::secretary::digests::Digests {
                history: &self.digests,
                texts: &self.digest_texts,
                lines: self.lines.conversations(),
                owners: &self.owners,
                compacting: &suzatary_core::secretary::compacting::Compacting::Off,
            },
        }
    }

    /// Every object, printed.
    fn prints(&self) -> String {
        self.history
            .objects()
            .expect("objects")
            .iter()
            .map(|(_, object)| prodrome_core::event::canonical_envelope(object))
            .collect()
    }

    /// Every text kept, its words.
    fn words(&self) -> Vec<String> {
        self.texts
            .keys()
            .iter()
            .map(|name| {
                self.texts
                    .get(name)
                    .expect("reads")
                    .expect("held")
                    .words()
                    .to_string()
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
enum Step {
    Remember(usize, Kind),
    Forget(usize),
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    let kind = prop_oneof![
        Just(Kind::Owner),
        Just(Kind::Person),
        Just(Kind::Preference),
        Just(Kind::Standing),
    ];
    proptest::collection::vec(
        prop_oneof![
            (0usize..30, kind).prop_map(|(n, kind)| Step::Remember(n, kind)),
            (0usize..30).prop_map(Step::Forget),
        ],
        0..60,
    )
}

/// What a law 11 script does: remember a fact said at a turn, forget a standing one, or
/// consolidate some standing ones into new words.
#[derive(Debug, Clone)]
enum Act {
    Remember(usize, usize),
    Forget(usize),
    Consolidate(Vec<usize>, usize),
}

fn acts() -> impl Strategy<Value = Vec<Act>> {
    proptest::collection::vec(
        prop_oneof![
            3 => (0usize..40, 0usize..5).prop_map(|(n, turn)| Act::Remember(n, turn)),
            1 => (0usize..40).prop_map(Act::Forget),
            2 => (proptest::collection::vec(0usize..40, 1..4), 40usize..80)
                .prop_map(|(picks, n)| Act::Consolidate(picks, n)),
        ],
        0..50,
    )
}

fn turn(turn: usize) -> Origin {
    Origin::Turn {
        conversation: ConversationId::new("default").expect("a slug"),
        turn,
    }
}

fn said() -> Origin {
    Origin::Turn {
        conversation: ConversationId::new("default").expect("a slug"),
        turn: 0,
    }
}

fn now() -> Datetime {
    Datetime::new(2026, 10, 2, 9, 0, 0, 0).expect("an instant")
}

fn ada() -> Actor {
    Actor::new("ada").expect("an actor")
}

/// A text no other fact's text contains, so "no byte spells it" is exact.
fn text(n: usize) -> String {
    format!("fact number {n} is qx{n}qx")
}

proptest! {
    /// Replaying any script leaves exactly the facts a plain map holds, oldest first, with a
    /// version counting every write; the texts hold exactly the standing facts' words, and no
    /// object holds any fact's.
    #[test]
    fn memory_is_a_map_and_keeps_words_apart(script in steps()) {
        let held = Held::default();
        let memories = held.memories();
        let mut model: BTreeMap<FactId, (String, Kind)> = BTreeMap::new();
        let mut writes = 0u64;
        for step in script {
            match step {
                Step::Remember(n, kind) => {
                    let fact = Fact::new(text(n)).expect("a fact");
                    let standing = model.values().any(|(t, _)| *t == text(n));
                    let done = memories.remember(&fact, kind, &said(), now());
                    prop_assert_eq!(done.is_ok(), !standing, "a duplicate is refused");
                    if let Ok(remembered) = done {
                        writes += 1;
                        prop_assert_eq!(remembered.id, FactId(writes));
                        model.insert(remembered.id, (text(n), kind));
                    }
                }
                Step::Forget(pick) => {
                    let Some(&id) = model.keys().nth(pick % (model.len().max(1))) else {
                        prop_assert!(memories.forget(FactId(9999), now(), &ada()).is_err());
                        continue;
                    };
                    memories.forget(id, now(), &ada()).expect("forgets a standing fact");
                    prop_assert!(memories.forget(id, now(), &ada()).is_err(), "once is all");
                    writes += 1;
                    model.remove(&id);
                }
            }
        }
        let read = memories.reads().read().expect("reads");
        let facts: Vec<(FactId, String, Kind)> = read
            .facts
            .iter()
            .map(|r| (r.id, r.fact.as_str().to_owned(), r.kind))
            .collect();
        let expected: Vec<(FactId, String, Kind)> =
            model.iter().map(|(id, (t, kind))| (*id, t.clone(), *kind)).collect();
        prop_assert_eq!(facts, expected);
        prop_assert_eq!(read.version, writes);
        let mut words = held.words();
        words.sort();
        let mut standing: Vec<String> = model.values().map(|(t, _)| t.clone()).collect();
        standing.sort();
        prop_assert_eq!(words, standing, "the texts are exactly the standing facts' words");
        let prints = held.prints();
        for n in 0..30 {
            prop_assert!(!prints.contains(&format!("qx{n}qx")), "an object holds fact {}'s words", n);
        }
    }
}

proptest! {
    /// Law 11, clause 1, on one box: when every source stands, `live(h + Consolidated{from,
    /// into}) = live(h) − from + {into}` and `origins(into) = ⋃ origins(from)`; after the write's
    /// sweep no text holds a source's words, and no object ever holds any fact's. A consolidation
    /// naming a forgotten or replaced fact is refused, so clause 2 (a source resting on a
    /// forgotten fact) arises only across replicas (`memory_schema.rs`). Forgetting a
    /// consolidated fact forgets what it stood on, and no standing fact rests on a forgotten one.
    #[test]
    fn a_consolidation_replaces_its_sources(script in acts()) {
        let held = Held::default();
        let memories = held.memories();
        // Each standing fact's words and the origins it stands on, as the law says they are.
        let mut model: BTreeMap<FactId, (String, BTreeSet<Origin>)> = BTreeMap::new();
        let mut gone: BTreeSet<FactId> = BTreeSet::new();
        let mut replaced: BTreeSet<FactId> = BTreeSet::new();
        let live = |memories: &Memories<'_>| -> BTreeSet<FactId> {
            memories.reads().history().expect("reads").live().map(|(id, _)| id).collect()
        };
        for act in script {
            match act {
                Act::Remember(n, at) => {
                    let words = text(n);
                    if let Ok(kept) = memories.remember(&Fact::new(&words).expect("a fact"), Kind::Owner, &turn(at), now()) {
                        model.insert(kept.id, (words, BTreeSet::from([turn(at)])));
                    }
                }
                Act::Forget(pick) => {
                    if let Some(&id) = model.keys().nth(pick % model.len().max(1)) {
                        memories.forget(id, now(), &ada()).expect("forgets a standing fact");
                        model.remove(&id);
                        gone.insert(id);
                    }
                }
                Act::Consolidate(picks, n) => {
                    // Every id ever written, so a pick may name a forgotten or replaced fact.
                    let ids: Vec<FactId> = model.keys().chain(&gone).chain(&replaced).copied().collect();
                    let from: BTreeSet<FactId> = picks
                        .iter()
                        .filter_map(|pick| ids.get(pick % ids.len().max(1)).copied())
                        .collect();
                    let words = text(n);
                    let before = live(&memories);
                    let done = memories.consolidate(&from, &Fact::new(&words).expect("a fact"), Kind::Person, &turn(9), now());
                    let elsewhere = model.iter().any(|(id, (said, _))| *said == words && !from.contains(id));
                    let standing = from.iter().all(|id| model.contains_key(id));
                    prop_assert_eq!(
                        done.is_ok(),
                        !from.is_empty() && standing && !elsewhere,
                        "a consolidation of a forgotten or replaced fact is refused: {:?}", done
                    );
                    let Ok(merged) = done else {
                        prop_assert_eq!(live(&memories), before, "a refused consolidation changes nothing");
                        continue;
                    };
                    let mut expected = before;
                    expected.retain(|id| !from.contains(id));
                    expected.insert(merged.id);
                    prop_assert_eq!(live(&memories), expected, "live − from + into");
                    let origins: BTreeSet<Origin> = from.iter().flat_map(|id| model[id].1.clone()).collect();
                    let read: BTreeSet<Origin> = merged.said.origins.iter().cloned().collect();
                    prop_assert_eq!(&read, &origins, "the union of the sources' origins");
                    let history = memories.reads().history().expect("reads");
                    let stored = history.provenance(merged.id).expect("reads");
                    prop_assert_eq!(stored.origins.iter().cloned().collect::<BTreeSet<_>>(), origins.clone());
                    for id in &from {
                        model.remove(id);
                        replaced.insert(*id);
                    }
                    model.insert(merged.id, (words, origins));
                    // A consolidation of the sources again is refused: they no longer stand.
                    prop_assert!(memories.consolidate(&from, &Fact::new("again").expect("a fact"), Kind::Owner, &turn(9), now()).is_err());
                }
            }
            let mut words = held.words();
            words.sort();
            let mut standing: Vec<String> = model.values().map(|(said, _)| said.clone()).collect();
            standing.sort();
            prop_assert_eq!(words, standing, "the texts hold exactly the standing facts' words");
            prop_assert_eq!(live(&memories), model.keys().copied().collect::<BTreeSet<_>>());
            let history = memories.reads().history().expect("reads");
            for (id, _) in history.live() {
                for down in &gone {
                    prop_assert!(!history.forgotten_with(*down).contains_key(&id), "fact {id} rests on forgotten fact {down}");
                }
            }
        }
        let prints = held.prints();
        for n in 0..80 {
            prop_assert!(!prints.contains(&format!("qx{n}qx")), "an object holds fact {}'s words", n);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    /// However remembers and forgets interleave past the cap, the reading never holds more than
    /// [`MAX_LIVE`] facts, and a remember at the cap is refused naming it.
    #[test]
    fn the_cap_holds_over_the_reading(forgets in proptest::collection::vec(0usize..400, 0..20)) {
        let held = Held::default();
        let memories = held.memories();
        let mut n = 0;
        for round in 0..=forgets.len() {
            while memories.reads().read().expect("reads").facts.len() < MAX_LIVE {
                n += 1;
                memories.remember(&Fact::new(text(n)).expect("a fact"), Kind::Owner, &said(), now())
                    .expect("room");
            }
            n += 1;
            let refused = memories
                .remember(&Fact::new(text(n)).expect("a fact"), Kind::Owner, &said(), now())
                .expect_err("at the cap");
            prop_assert!(refused.message().contains(&MAX_LIVE.to_string()), "{}", refused.message());
            let read = memories.reads().read().expect("reads");
            prop_assert!(read.facts.len() <= MAX_LIVE);
            prop_assert!(memories.reads().history().expect("reads").live().count() <= MAX_LIVE);
            if let Some(pick) = forgets.get(round) {
                let id = read.facts[pick % read.facts.len()].id;
                memories.forget(id, now(), &ada()).expect("forgets");
            }
        }
    }
}

#[derive(Debug, Clone)]
enum Moment {
    Remember(usize),
    Forget(usize),
    /// Consolidate the standing facts the picks name into new words.
    Consolidate(usize, usize, usize),
    /// A round in conversation `.0`; `.1` says its session is fresh.
    Round(usize, bool),
}

fn moments() -> impl Strategy<Value = Vec<Moment>> {
    proptest::collection::vec(
        prop_oneof![
            (0usize..20).prop_map(Moment::Remember),
            (0usize..20).prop_map(Moment::Forget),
            (0usize..20, 0usize..20, 20usize..40)
                .prop_map(|(a, b, n)| Moment::Consolidate(a, b, n)),
            (0usize..3, proptest::bool::weighted(0.2)).prop_map(|(c, f)| Moment::Round(c, f)),
        ],
        0..80,
    )
}

proptest! {
    /// Across three conversations, each round's heading tells the model exactly the memory as it
    /// stood, no fact twice and no forgotten fact it was never told.
    #[test]
    fn one_memory_every_conversation(script in moments()) {
        let held = Held::default();
        let memories = held.memories();
        let opened = || vec![suzatary_core::secretary::store::Entry::Opened {
            at: suzatary_core::capability::Datetime::new(2026, 10, 2, 9, 0, 0, 0).expect("an instant"),
            title: suzatary_core::secretary::words::Words::kept("t".to_owned()),
            model: "m".to_owned(),
            kind: suzatary_core::secretary::store::Kind::Topic,
        }];
        let mut files: Vec<Vec<suzatary_core::secretary::store::Entry>> = vec![opened(), opened(), opened()];
        let mut knows: Vec<std::collections::BTreeSet<FactId>> = vec![std::collections::BTreeSet::default(); 3];
        for moment in script {
            match moment {
                Moment::Remember(n) => {
                    let _ = memories.remember(&Fact::new(text(n)).expect("a fact"), Kind::Owner, &said(), now());
                }
                Moment::Forget(pick) => {
                    let standing = memories.reads().read().expect("reads").facts;
                    if let Some(one) = standing.get(pick % standing.len().max(1)) {
                        memories.forget(one.id, now(), &ada()).expect("forgets");
                    }
                }
                Moment::Consolidate(a, b, n) => {
                    let standing = memories.reads().read().expect("reads").facts;
                    let from: BTreeSet<FactId> = [a, b]
                        .iter()
                        .filter_map(|pick| standing.get(pick % standing.len().max(1)).map(|one| one.id))
                        .collect();
                    let _ = memories.consolidate(&from, &Fact::new(text(n)).expect("a fact"), Kind::Owner, &said(), now());
                }
                Moment::Round(c, fresh) => {
                    let id = ConversationId::new(format!("c{c}")).expect("a slug");
                    let stored = suzatary_core::secretary::store::Stored::of(&id, &files[c]).expect("folds");
                    let now = memories.reads().read().expect("reads");
                    let change = memory::change(&now, if fresh { None } else { stored.memory_seen });
                    match &change {
                        memory::Change::Whole { facts, .. } => {
                            knows[c] = facts.iter().map(|r| r.id).collect();
                        }
                        memory::Change::Since { added, forgotten, merged, .. } => {
                            for gone in forgotten {
                                prop_assert!(knows[c].remove(gone), "told to forget {gone}, never told of it");
                            }
                            for merge in merged {
                                prop_assert!(knows[c].remove(&merge.fact), "told {} is merged, never told of it", merge.fact);
                                prop_assert!(now.get(merge.into).is_some(), "told to rely on fact {}, which does not stand", merge.into);
                            }
                            for added in added {
                                prop_assert!(knows[c].insert(added.id), "told of {} twice", added.id);
                            }
                        }
                    }
                    let expected: std::collections::BTreeSet<FactId> = now.facts.iter().map(|r| r.id).collect();
                    prop_assert_eq!(&knows[c], &expected);
                    if stored.memory_seen != Some(change.version()) {
                        files[c].push(suzatary_core::secretary::store::Entry::MemorySeen {
                            at: suzatary_core::capability::Datetime::new(2026, 10, 2, 9, 0, 0, 0).expect("an instant"),
                            version: change.version(),
                        });
                    }
                }
            }
        }
    }
}

/// A salt that repeats does not cost a standing fact its words: forgetting the first holder of a
/// name leaves the text while another standing fact names it.
#[test]
fn the_sweep_spares_a_text_a_standing_fact_names() {
    struct Same;
    impl Entropy for Same {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
            bytes.fill(9);
            Ok(())
        }
    }
    let (history, texts) = (InMemory::default(), InMemoryDoc::default());
    let cards = InMemory::<InboxEvent>::default();
    let digests = Held::default();
    let memories = Memories {
        history: &history,
        texts: &texts,
        cards: &cards,
        entropy: &Same,
        digests: digests.memories().digests,
    };
    let fact = Fact::new("He takes the 7:40 train").expect("a fact");
    let first = memories
        .remember(&fact, Kind::Owner, &said(), now())
        .expect("remembers");
    memories.forget(first.id, now(), &ada()).expect("forgets");
    let again = memories
        .remember(&fact, Kind::Owner, &said(), now())
        .expect("remembers again, as a new fact");
    let other = memories
        .remember(
            &Fact::new("Another").expect("a fact"),
            Kind::Owner,
            &said(),
            now(),
        )
        .expect("remembers");
    memories.forget(other.id, now(), &ada()).expect("forgets");
    assert_eq!(memories.reads().read().expect("reads").facts, vec![again]);
}
