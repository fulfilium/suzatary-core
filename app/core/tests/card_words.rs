//! A remember card's words, through the same holds and stores the box uses: however cards are
//! asked, tapped and dismissed and facts remembered and forgotten, the memory's text store holds
//! exactly the words something live names (a standing fact, or a card the owner may still tap),
//! every such card reads its words, and no object of either store holds any.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;
use prodrome_core::schema::Schema;
use proptest::prelude::*;
use suzatary_core::capability::Entropy;
use suzatary_core::secretary::inbox::{self, Fold, ProposalId};
use suzatary_core::secretary::inbox_event::InboxEvent;
use suzatary_core::secretary::memories::{Kept, Memories};
use suzatary_core::secretary::memory::{Fact, FactId, Kind, Origin};
use suzatary_core::secretary::memory_event::MemoryEvent;
use suzatary_core::secretary::responded::{Done, Performed, Responded, Tried};
use suzatary_core::secretary::salted_text::{Text, TextName};
use suzatary_core::secretary::store::{ConversationId, Proposal, Reasons};
use suzatary_core::secretary::texts;
use suzatary_core::store::{InMemory, InMemoryDoc, Prodrome};

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

#[derive(Default)]
struct Box {
    history: InMemory<MemoryEvent>,
    texts: InMemoryDoc<TextName, Text<Fact>>,
    cards: InMemory<InboxEvent>,
    entropy: Counting,
    lines: suzatary_core::secretary::conversations::InMemory,
    owners: std::collections::BTreeSet<prodrome_core::event::Actor>,
    digests: InMemory<suzatary_core::secretary::digest_event::DigestEvent>,
    digest_texts: InMemoryDoc<TextName, Text<suzatary_core::secretary::digest_line::Line>>,
}

impl Box {
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

    fn fold(&self) -> Fold {
        inbox::fold(&self.cards).expect("the inbox folds")
    }

    /// The cards still standing, oldest id first.
    fn open(&self) -> Vec<ProposalId> {
        self.fold()
            .objects()
            .filter(|object| object.reading.open())
            .map(|object| object.id.clone())
            .collect()
    }
}

fn now() -> Datetime {
    Datetime::new(2026, 10, 3, 9, 0, 0, 0).expect("an instant")
}

fn owner() -> Actor {
    Actor::new("ada").expect("an actor")
}

const WORDS: [&str; 4] = [
    "Mira keeps the books",
    "He takes calls after ten",
    "The café is closed on Mondays",
    "Always copy the registry office",
];

fn fact(n: usize) -> Fact {
    Fact::new(WORDS[n % WORDS.len()]).expect("a fact")
}

fn conversation() -> ConversationId {
    ConversationId::new("default").expect("a slug")
}

/// File a card about `fact`, as the `remember` verb does in a tainted round.
fn ask(the: &Box, fact: &Fact, kind: Kind, index: usize) -> ProposalId {
    let held = the.memories().hold().expect("held");
    let kept = held.keep_card(fact).expect("kept");
    let asked = inbox::Asked {
        proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
        proposal: Proposal::Remember {
            text: kept.name().clone(),
            kind,
        },
        reasons: Reasons::new(
            "a message said so",
            vec!["mail:abcdef012345".parse().expect("a ref")],
        )
        .expect("reasons"),
        asked: format!("proposal:default/{index}").parse().expect("a ref"),
        price: inbox::Price::unpriced(),
    };
    inbox::ask(
        &the.cards,
        asked,
        &Actor::new("assistant").expect("an actor"),
        now(),
    )
    .expect("filed")
}

/// Tap a card, as the route does: the memory's hold first, then the inbox's.
fn tap(the: &Box, id: &ProposalId, index: usize) {
    let held = the.memories().hold().expect("held");
    inbox::submit(&the.cards, id, &owner(), now(), |asked| {
        let Proposal::Remember { text, kind } = &asked.proposal else {
            unreachable!("only cards are asked here")
        };
        let origin = Origin::Tapped {
            conversation: conversation(),
            proposal: index,
        };
        Ok(match held.remember_kept(text, *kind, &origin, now()) {
            Ok(remembered) => Performed::Done(Done::Acted {
                what: format!("remembered as fact {}", remembered.id),
            }),
            Err(refusal) => Performed::Tried(Tried::NotSent {
                reason: refusal.message().to_owned(),
            }),
        })
    })
    .expect("tapped");
}

/// Dismiss a card, as `secretary::dismiss` does: the response, then the sweep.
fn dismiss(the: &Box, id: &ProposalId) {
    inbox::dismiss(&the.cards, id, &owner(), now(), "not from a newsletter").expect("dismissed");
    the.memories().sweep().expect("swept");
}

/// What something live names: a standing fact's text or a standing card's.
fn live(the: &Box) -> BTreeSet<TextName> {
    let history = the.memories().reads().history().expect("reads");
    history
        .live()
        .map(|(_, kept)| kept.text.clone())
        .chain(the.fold().objects().filter_map(|object| {
            match (&object.asked.proposal, object.reading.open()) {
                (Proposal::Remember { text, .. }, true) => Some(text.clone()),
                _ => None,
            }
        }))
        .collect()
}

/// Every object a store holds, printed: what is pushed, and so what must never hold words.
fn prints<E: Schema>(store: &dyn Prodrome<E>) -> String {
    store
        .objects()
        .expect("objects")
        .iter()
        .map(|(_, object)| prodrome_core::event::canonical_envelope(object))
        .collect()
}

#[derive(Debug, Clone)]
enum Step {
    Ask(usize, Kind),
    Tap(usize),
    Dismiss(usize),
    Remember(usize),
    Forget(usize),
}

fn kind() -> impl Strategy<Value = Kind> {
    prop_oneof![Just(Kind::Person), Just(Kind::Preference)]
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    proptest::collection::vec(
        prop_oneof![
            (0usize..4, kind()).prop_map(|(n, kind)| Step::Ask(n, kind)),
            (0usize..8).prop_map(Step::Tap),
            (0usize..8).prop_map(Step::Dismiss),
            (0usize..4).prop_map(Step::Remember),
            (0usize..8).prop_map(Step::Forget),
        ],
        0..24,
    )
}

proptest! {
    /// And what the sweep may delete only grows: a text nothing live names is never named by
    /// anything live again, so a deletion is never one a later write would have needed.
    #[test]
    fn the_text_store_holds_exactly_what_is_live(script in steps()) {
        let the = Box::default();
        let mut was_dead = BTreeSet::new();
        for (index, step) in script.into_iter().enumerate() {
            match step {
                Step::Ask(n, kind) => {
                    ask(&the, &fact(n), kind, index);
                }
                Step::Tap(pick) => {
                    let open = the.open();
                    if let Some(id) = open.get(pick % open.len().max(1)) {
                        tap(&the, id, index);
                    }
                }
                Step::Dismiss(pick) => {
                    let open = the.open();
                    if let Some(id) = open.get(pick % open.len().max(1)) {
                        dismiss(&the, id);
                    }
                }
                Step::Remember(n) => {
                    let said = Origin::Turn { conversation: conversation(), turn: index };
                    let _ = the.memories().remember(&fact(n), Kind::Owner, &said, now());
                }
                Step::Forget(pick) => {
                    let standing: Vec<FactId> =
                        the.memories().reads().read().expect("reads").facts.iter().map(|f| f.id).collect();
                    if let Some(id) = standing.get(pick % standing.len().max(1)) {
                        the.memories().forget(*id, now(), &owner()).expect("forgets");
                    }
                }
            }
            let dead = texts::dead([
                the.memories().reads().history().expect("reads").naming(),
                the.fold().naming(),
            ]);
            prop_assert!(was_dead.is_subset(&dead), "a dead text is dead for good");
            was_dead = dead;
            let kept: BTreeSet<TextName> = the.texts.keys().into_iter().collect();
            prop_assert_eq!(&kept, &live(&the), "the text store holds exactly what is live");
            for object in the.fold().objects() {
                if let (Proposal::Remember { text, .. }, true) =
                    (&object.asked.proposal, object.reading.open())
                {
                    prop_assert!(the.memories().reads().words(text).expect("the texts read").is_some(), "a standing card reads its words");
                }
            }
            let prints = prints(&the.cards) + &prints(&the.history);
            for words in WORDS {
                prop_assert!(!prints.contains(words), "no object holds the words");
            }
        }
    }
}

/// The same fact asked again while a card keeps its words is that card; once the card is
/// dismissed, its words are gone and asking again is a new card with words of its own.
#[test]
fn a_fact_asked_again_is_its_card_until_the_card_is_answered() {
    let the = Box::default();
    let first = ask(&the, &fact(0), Kind::Person, 0);
    assert_eq!(ask(&the, &fact(0), Kind::Person, 1), first);
    let held = the.memories().hold().expect("held");
    assert!(matches!(
        held.keep_card(&fact(0)).expect("kept"),
        Kept::Found(_)
    ));
    drop(held);
    dismiss(&the, &first);
    assert!(
        the.texts.keys().is_empty(),
        "the dismissed card's words are deleted"
    );
    assert!(matches!(
        the.fold()
            .get(&first)
            .expect("the card")
            .reading
            .shown()
            .map(|answer| &answer.response),
        Some(Responded::Dismissed { .. })
    ));
    let again = ask(&the, &fact(0), Kind::Person, 2);
    assert_ne!(again, first);
    assert_eq!(the.texts.keys().len(), 1);
}

/// A tapped card and the fact it became share one text: it stays while the fact stands and goes
/// when the fact is forgotten.
#[test]
fn a_tapped_cards_words_go_with_its_fact() {
    let the = Box::default();
    let card = ask(&the, &fact(1), Kind::Preference, 0);
    tap(&the, &card, 0);
    assert_eq!(the.texts.keys().len(), 1, "the fact names the card's text");
    let read = the.memories().reads().read().expect("reads");
    assert_eq!(read.facts.len(), 1);
    assert_eq!(read.facts[0].fact, fact(1));
    the.memories()
        .forget(read.facts[0].id, now(), &owner())
        .expect("forgets");
    assert!(
        the.texts.keys().is_empty(),
        "forgetting the fact deletes the card's words"
    );
}
