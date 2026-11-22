//! The in-memory store's `verify` is the store on disk's over the same objects.
//!
//! A missing dependency and a broken seal cannot be given to `MemoryStore`, which verifies on
//! receipt and refuses them, so `findings_of` over the same prints stands in for it.

use std::collections::BTreeSet;

use prodrome_core::change::mk_change;
use prodrome_core::dag::Dag;
use prodrome_core::event::{Envelope, Hash, canonical_envelope, parents_of, seal_hash};
use prodrome_core::laws::script::Who;
use prodrome_core::policy::Untrusted;
use proptest::prelude::*;

use suzatary_inbox::secretary::inbox::ProposalId;
use suzatary_inbox::secretary::inbox_event::{InboxEvent, InboxVocabulary, Said};
use suzatary_inbox::secretary::responded::{Done, Responded, Tried};
use suzatary_io::objects::Objects;
use suzatary_signatures::signatures::findings_of;
use suzatary_store::store::{Prodrome as _, Reads as _};
use suzatary_stores::stores::{Faulty, InMemory};

use crate::script_law::{STEPS, an_inbox_write};

#[derive(Debug, Clone)]
enum Fault {
    None,
    /// A try resting on a mail already sent: below the reading it rests on.
    Below,
    /// The print of the object at this position, among those something rests on, is absent.
    Missing(prop::sample::Index),
    /// The print of the object at this position is not the bytes its name is the hash of.
    Broken(prop::sample::Index),
}

fn a_fault() -> impl Strategy<Value = Fault> {
    prop_oneof![
        Just(Fault::None),
        Just(Fault::Below),
        any::<prop::sample::Index>().prop_map(Fault::Missing),
        any::<prop::sample::Index>().prop_map(Fault::Broken),
    ]
}

fn a_history() -> impl Strategy<Value = Vec<(InboxEvent, u64)>> {
    let write = prop::sample::select(Who::ALL.to_vec()).prop_flat_map(an_inbox_write);
    prop::collection::vec((write, any::<u64>()), 1..STEPS)
}

/// An event on a proposal no drawn write touches.
fn on_its_own(said: Responded) -> InboxEvent {
    InboxEvent {
        proposal: ProposalId::new("below").expect("a proposal id"),
        at: prodrome_core::literal::Datetime::new(2026, 10, 1, 9, 0, 0, 0).expect("an instant"),
        actor: Who::Ada.actor(),
        said: Said::Responded(said),
    }
}

/// `objects` and a change by hand that falls below what it rests on: a try after the send.
fn with_below(
    mut objects: Vec<(Hash, Envelope<InboxEvent>)>,
) -> (Vec<(Hash, Envelope<InboxEvent>)>, Hash) {
    let genesis = objects[0].0.clone();
    let mut change = |deps: Vec<Hash>, said| {
        let object =
            Envelope::Change(mk_change(genesis.clone(), deps, on_its_own(said)).expect("a change"));
        let name = seal_hash(&object);
        objects.push((name.clone(), object));
        name
    };
    let sent = change(
        Vec::new(),
        Responded::Done(Done::Sent {
            recipients: vec!["mira@example.com".to_owned()],
            message_id: "<1@box>".to_owned(),
        }),
    );
    let below = change(
        vec![sent],
        Responded::Tried {
            attempt: 1,
            tried: Tried::NotSent {
                reason: String::new(),
            },
        },
    );
    (objects, below)
}

fn on_disk(prints: &[(Hash, Vec<u8>)]) -> (suzatary_testkit::TempDir, Objects<InboxEvent>) {
    let dir = suzatary_testkit::scratch("verify-law");
    let objects = dir.path().join("inbox/objects");
    std::fs::create_dir_all(&objects).expect("a directory");
    for (name, print) in prints {
        std::fs::write(objects.join(format!("{}.py", name.as_str())), print).expect("written");
    }
    let store = Objects::open(dir.path().join("inbox"));
    (dir, store)
}

/// What `findings_of` reports of `prints` read as a store reads its files.
fn read_as_prints(prints: &[(Hash, Vec<u8>)]) -> Vec<String> {
    let dag: Dag<InboxEvent> = Dag::from_prints(
        &InboxVocabulary,
        prints
            .iter()
            .map(|(name, print)| (name.clone(), Ok(print.clone()))),
        prodrome_core::store::Sender::Keeper,
    );
    findings_of(&dag, &Untrusted::none())
        .iter()
        .map(ToString::to_string)
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn the_store_in_memory_finds_what_the_store_on_disk_finds(
        events in a_history(),
        fault in a_fault(),
    ) {
        let objects = prodrome_core::laws::history(&events).expect("a history");
        let (objects, below) = match fault {
            Fault::Below => {
                let (objects, below) = with_below(objects);
                (objects, Some(below))
            }
            Fault::None | Fault::Missing(_) | Fault::Broken(_) => (objects, None),
        };
        let mut prints: Vec<(Hash, Vec<u8>)> = objects
            .iter()
            .map(|(name, object)| (name.clone(), canonical_envelope(object).into_bytes()))
            .collect();
        let rested_on: BTreeSet<Hash> = objects.iter().flat_map(|(_, object)| parents_of(object)).collect();
        match &fault {
            Fault::None | Fault::Below => {}
            Fault::Missing(at) => {
                let parents: Vec<usize> = (0..prints.len()).filter(|i| rested_on.contains(&prints[*i].0)).collect();
                prints.remove(parents[at.index(parents.len())]);
            }
            Fault::Broken(at) => prints[at.index(objects.len())].1.extend_from_slice(b" "),
        }
        let (_dir, disk) = on_disk(&prints);
        let found = disk.verify();
        prop_assert_eq!(&read_as_prints(&prints), &found);

        let memory = InMemory::<InboxEvent, Faulty>::faulty();
        let given: Vec<(Hash, String)> = prints
            .iter()
            .map(|(name, print)| (name.clone(), String::from_utf8(print.clone()).expect("text")))
            .collect();
        match fault {
            Fault::None | Fault::Below => {
                memory.accept(&given).expect("taken");
                prop_assert_eq!(memory.verify(), found.clone());
                if let Some(below) = below {
                    prop_assert!(found.iter().any(|finding| finding.contains(below.as_str())), "{found:?}");
                }
            }
            Fault::Missing(_) | Fault::Broken(_) => {
                prop_assert!(!found.is_empty());
                prop_assert!(memory.accept(&given).is_err(), "a replica verified on receipt takes neither");
                prop_assert!(memory.objects().expect("reads").is_empty());
            }
        }
    }
}
