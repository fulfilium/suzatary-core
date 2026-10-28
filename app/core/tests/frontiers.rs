//! A pull names what its replica holds by the frontier the box's last answer named
//! (`prodrome_core::store::frontier`, the Prodrome's law 47). Over random histories of a topic
//! store, written by other devices and by the tab itself, the tab pulling in pieces it may stop
//! short of, posting its own writes, and the box losing every frontier it recorded:
//!
//! - Safe: every frontier the tab names reaches only objects it holds.
//! - Complete: once a pull ends, the tab holds every object the box holds.
//! - Quiet: a pull by a tab that holds all of it sends no object.
//! - Fallback: a frontier the box lost, or one naming heads its store no longer holds (a later
//!   epoch holding less), is answered once as unknown, from nothing, and every other piece of a
//!   pull brings the tab something it lacked: no pull asks again for what it was sent.
//!
//! The tab keeps what an answer names as `replica.ts` does: each piece read against the frontier
//! it was asked from, its era moved on by an answer that did not know that one, and the later of
//! two kept by (era, count, digest).

use std::collections::BTreeSet;

use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope, seal_hash};
use prodrome_core::genesis::mk_genesis;
use prodrome_core::literal::Datetime;
use prodrome_core::schema::Schema as _;
use prodrome_core::store::{MemoryStore, Replica as _};
use proptest::prelude::*;

use prodrome_core::store::frontier::{Frontiers, Reached, Since, Token};
use suzatary_core::epoch::Epochs;
use suzatary_core::routes::{self, Books, Cut, Posted};
use suzatary_core::secretary::store::ConversationId;
use suzatary_core::secretary::topics::reading::MembershipEvent;
use suzatary_core::secretary::topics::target::Target;
use suzatary_core::secretary::topics::topic::Topic;
use suzatary_core::store::Reads as _;
use suzatary_core::store::{InMemory, Prodrome as _};
use suzatary_core::wire::{Frontier, SinceRead};

const KEY: &str = "store/topics";

fn now() -> Datetime {
    Datetime::new(2026, 10, 25, 12, 0, 0, 0).expect("an instant")
}

fn at(minute: u32) -> Datetime {
    Datetime::new(2026, 10, 25, 8 + minute / 60 % 12, minute % 60, 0, 0).expect("an instant")
}

/// The genesis every replica of the store begins from.
fn genesis() -> (Hash, String) {
    let genesis: Envelope<MembershipEvent> =
        Envelope::Genesis(mk_genesis("topics", &"0".repeat(32)).expect("a genesis"));
    (seal_hash(&genesis), canonical_envelope(&genesis))
}

/// One device's replica of the store, and its name as a writer.
struct Device {
    store: MemoryStore<MembershipEvent>,
    actor: Actor,
    clock: u32,
}

impl Device {
    fn new(name: &str) -> Self {
        let store = MemoryStore::default();
        let (name_of, print) = genesis();
        store
            .receive([name_of].into(), &|_| Some(print.clone().into_bytes()))
            .expect("begun");
        Self {
            store,
            actor: Actor::new(name).expect("an actor"),
            clock: 0,
        }
    }

    fn held(&self) -> BTreeSet<Hash> {
        self.store
            .held()
            .expect("held")
            .dag
            .objects()
            .keys()
            .cloned()
            .collect()
    }

    /// One membership this device's reading plans: adding target `n` to a topic.
    fn write(&mut self, n: usize) {
        let nodes = self.store.held().expect("held").dag.nodes().expect("nodes");
        self.clock += 1;
        let target = Target::Round {
            conversation: ConversationId::new("default").expect("a slug"),
            turn: n,
        };
        let topic = Topic::new(["travel", "work"][n % 2]).expect("a topic");
        if let Some(event) = suzatary_core::secretary::topics::reading::of(&nodes, &[])
            .expect("one genesis")
            .add(&target, &topic, at(self.clock), &self.actor)
        {
            self.store.append(event).expect("appended");
        }
    }

    /// Every object of this device's own writes, printed.
    fn own(&self) -> Vec<String> {
        self.store
            .held()
            .expect("held")
            .dag
            .objects()
            .values()
            .filter(|object| {
                object
                    .event()
                    .is_some_and(|event| event.actor() == &self.actor)
            })
            .map(canonical_envelope)
            .collect()
    }

    /// `objects` taken in, as a tab joins a piece.
    fn take(&self, objects: &[suzatary_core::wire::StoredObject]) {
        let prints: std::collections::BTreeMap<Hash, Vec<u8>> = objects
            .iter()
            .map(|o| {
                (
                    Hash::new(o.hash.clone()).expect("a name"),
                    o.literal.clone().into_bytes(),
                )
            })
            .collect();
        self.store
            .receive(prints.keys().cloned().collect(), &|name| {
                prints.get(name).cloned()
            })
            .expect("taken");
    }
}

/// The box: its store, its epochs and the frontiers it recorded, which it may lose, and every
/// store it held after a post, to go back to.
struct TheBox {
    store: InMemory<MembershipEvent>,
    epochs: Epochs,
    frontiers: Frontiers,
    held_before: Vec<Vec<(Hash, String)>>,
}

impl TheBox {
    fn new() -> Self {
        let store = InMemory::<MembershipEvent>::default();
        store.accept(&[genesis()]).expect("begun");
        Self {
            store,
            epochs: Epochs::remembered(),
            frontiers: Frontiers::remembered(prodrome_core::store::frontier::KEPT),
            held_before: vec![vec![genesis()]],
        }
    }

    /// What the store holds now, printed.
    fn prints(&self) -> Vec<(Hash, String)> {
        self.store
            .objects()
            .expect("reads")
            .iter()
            .map(|(name, object)| (name.clone(), canonical_envelope(object)))
            .collect()
    }

    /// The store as it was after an earlier post, its records kept: a later epoch holding less.
    fn go_back(&mut self, n: usize) {
        let earlier = &self.held_before[n % self.held_before.len()];
        let store = InMemory::<MembershipEvent>::default();
        store.accept(earlier).expect("restored");
        self.store = store;
    }

    fn books(&self) -> Books<'_> {
        Books {
            epochs: &self.epochs,
            frontiers: &self.frontiers,
        }
    }

    fn held(&self) -> BTreeSet<Hash> {
        self.store
            .objects()
            .expect("reads")
            .into_iter()
            .map(|(name, _)| name)
            .collect()
    }

    fn post(&mut self, device: &Device, since: &Since) -> suzatary_core::wire::ReceiptPayload {
        let own = device.own();
        let receipt = routes::receive_changes(
            &self.store,
            &Posted {
                objects: &own,
                since,
            },
            &self.frontiers,
            &device.actor,
        )
        .expect("posted");
        self.held_before.push(self.prints());
        receipt
    }
}

#[derive(Debug, Clone)]
enum Step {
    /// Another device writes and posts.
    Other(usize),
    /// The tab writes, and does not post yet.
    Write(usize),
    /// The tab posts its own writes.
    Post,
    /// The tab pulls in pieces of `budget` bytes, at most `pieces` of them.
    Pull { budget: usize, pieces: usize },
    /// The box loses every frontier it recorded.
    Forget,
    /// The box goes back to what it held after an earlier post, its records kept.
    GoBack(usize),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => (0..6usize).prop_map(Step::Other),
        2 => (0..6usize).prop_map(Step::Write),
        1 => Just(Step::Post),
        3 => (100..1500usize, 1..4usize).prop_map(|(budget, pieces)| Step::Pull { budget, pieces }),
        1 => Just(Step::Forget),
        1 => (0..40usize).prop_map(Step::GoBack),
    ]
}

/// Where the tab stands with the box, as `replica.ts`'s `Frontier` does: nothing, or a frontier
/// an answer named, with the era of answers that did not know the one asked from.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Standing {
    era: usize,
    count: usize,
    digest: Option<String>,
}

impl Standing {
    const NOTHING: Self = Self {
        era: 0,
        count: 0,
        digest: None,
    };

    fn since(&self) -> Since {
        self.digest.as_deref().map_or(Since::Nothing, |digest| {
            Since::Frontier(Token::parse(digest).expect("a digest"))
        })
    }

    /// `replica.answered`: the frontier an answer names, read against the one it was asked from.
    fn answered(asked: &Self, read: SinceRead, named: &Frontier) -> Self {
        Self {
            era: asked.era + usize::from(read == SinceRead::Unknown),
            count: named.count,
            digest: Some(named.digest.clone()),
        }
    }
}

/// The tab: its replica and where it stands, as the later of every answer it kept.
struct Tab {
    device: Device,
    standing: Standing,
}

impl Tab {
    /// An answer to a question asked from `asked`, kept: the frontier it names is safe, and joined
    /// as the later.
    fn answered(
        &mut self,
        the_box: &TheBox,
        asked: &Standing,
        read: SinceRead,
        frontier: &Frontier,
    ) -> Result<(), TestCaseError> {
        let token = Token::parse(&frontier.digest).expect("a token");
        let objects = the_box.store.objects().expect("reads");
        let dag: prodrome_core::dag::Dag<MembershipEvent> = objects.into_iter().collect();
        let lacking = the_box
            .frontiers
            .answer(KEY, &Since::Frontier(token), &dag)
            .expect("reads");
        let reached: BTreeSet<prodrome_core::event::Hash> = match lacking.reached() {
            Reached::Everything => dag.objects().keys().cloned().collect(),
            Reached::Known(reached) => reached.clone(),
            Reached::Unknown => {
                return Err(TestCaseError::fail("the answer's frontier is recorded"));
            }
        };
        prop_assert!(
            reached.is_subset(&self.device.held()),
            "the frontier names only what the tab holds"
        );
        prop_assert_eq!(reached.len(), frontier.count);
        self.standing = self
            .standing
            .clone()
            .max(Standing::answered(asked, read, frontier));
        Ok(())
    }

    /// At most `pieces` pieces of a pull, or until the box says none is left, each asked from where
    /// the tab stands; true when it ended. A pull makes progress, so it never loops: every answer in
    /// it becomes where the tab stands, and one that sent anything reaches further than the
    /// frontier it was asked from (a larger count, or a later era after an answer from nothing);
    /// at most one is from nothing.
    fn pull(
        &mut self,
        the_box: &TheBox,
        budget: usize,
        pieces: usize,
    ) -> Result<bool, TestCaseError> {
        let mut unknown = 0;
        for _ in 0..pieces {
            let asked = self.standing.clone();
            let piece = routes::store_objects_since(
                &the_box.store,
                &asked.since(),
                the_box.books(),
                now(),
                Cut::Bytes(budget),
            )
            .expect("a piece");
            self.device.take(&piece.objects.objects);
            let read = piece.objects.since;
            self.answered(the_box, &asked, read, &piece.objects.frontier)?;
            if read == SinceRead::Unknown {
                unknown += 1;
                prop_assert!(unknown == 1, "a pull answered from nothing twice");
            }
            prop_assert_eq!(
                &self.standing,
                &Standing::answered(&asked, read, &piece.objects.frontier),
                "the answer stands"
            );
            // A quiet answer, nothing missing, stands where it was asked from; any other moves on.
            prop_assert!(
                piece.objects.objects.is_empty()
                    || self.standing.era > asked.era
                    || self.standing.count > asked.count,
                "a piece from {:?} reached no further",
                asked.digest
            );
            if piece.rest == 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn law_a_pull_by_frontier_is_safe_complete_quiet_and_falls_back_once(
        steps in prop::collection::vec(step(), 1..40),
        budget in 100..1500usize,
    ) {
        let mut the_box = TheBox::new();
        let mut others = Device::new("ada");
        let mut tab = Tab { device: Device::new("tab"), standing: Standing::NOTHING };
        for step in &steps {
            match step {
                Step::Other(n) => {
                    others.write(*n);
                    the_box.post(&others, &Since::Nothing);
                }
                Step::Write(n) => tab.device.write(*n),
                Step::Post => {
                    let asked = tab.standing.clone();
                    let receipt = the_box.post(&tab.device, &asked.since());
                    tab.answered(&the_box, &asked, receipt.since, &receipt.frontier)?;
                }
                Step::Pull { budget, pieces } => {
                    tab.pull(&the_box, *budget, *pieces)?;
                }
                Step::Forget => the_box.frontiers = Frontiers::remembered(prodrome_core::store::frontier::KEPT),
                Step::GoBack(n) => the_box.go_back(*n),
            }
        }
        // One pull to the end, making progress however the box lost or went back.
        prop_assert!(tab.pull(&the_box, budget, the_box.held().len() + 2)?, "a pull ends");
        prop_assert!(the_box.held().is_subset(&tab.device.held()), "complete");
        let quiet = routes::store_objects_since(&the_box.store, &tab.standing.since(), the_box.books(), now(), Cut::Bytes(budget))
            .expect("a piece");
        prop_assert_eq!(quiet.objects.objects.len(), 0, "quiet");
        prop_assert_eq!(quiet.objects.since, SinceRead::Known);
    }
}

/// A frontier recorded before the store went back to holding less is answered as unknown, from
/// nothing, and the frontier that answer names stands over it: the next pull is quiet.
#[test]
fn a_store_that_went_back_answers_a_frontier_past_it_once_from_nothing() {
    let mut the_box = TheBox::new();
    let mut others = Device::new("ada");
    for n in 0..4 {
        others.write(n);
        the_box.post(&others, &Since::Nothing);
    }
    let mut tab = Tab {
        device: Device::new("tab"),
        standing: Standing::NOTHING,
    };
    assert!(tab.pull(&the_box, 1 << 20, 1).expect("pulled"));
    let before = tab.standing.clone();
    the_box.go_back(1);
    let asked = tab.standing.clone();
    let piece = routes::store_objects_since(
        &the_box.store,
        &asked.since(),
        the_box.books(),
        now(),
        Cut::Whole,
    )
    .expect("a piece");
    assert_eq!(
        piece.objects.since,
        SinceRead::Unknown,
        "its heads are past what the store holds"
    );
    tab.answered(
        &the_box,
        &asked,
        piece.objects.since,
        &piece.objects.frontier,
    )
    .expect("kept");
    assert!(
        tab.standing.count < before.count && tab.standing > before,
        "the smaller frontier stands"
    );
    let quiet = routes::store_objects_since(
        &the_box.store,
        &tab.standing.since(),
        the_box.books(),
        now(),
        Cut::Whole,
    )
    .expect("a piece");
    assert_eq!(quiet.objects.objects.len(), 0);
    assert_eq!(quiet.objects.since, SinceRead::Known);
}
