//! A line the tab seals, as the box takes it. The laws:
//! - the node the tab computes for its own line equals the node the box serves for it once the
//!   box takes it, so a reading of the tab's own line and of the box's are one;
//! - a line the tab seals and the line the box seals from the same words and keys read alike;
//! - an own line reads back from its print and its keys alone, the device's whole record of it.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};

use prodrome_core::event::{Actor, Envelope, canonical_envelope};
use prodrome_core::literal::Datetime;
use prodrome_core::store::frontier::{Frontiers, KEPT, Since};
use suzatary_api::routes::{self, Books, Cut};
use suzatary_base::capability::Entropy;
use suzatary_base::conversation_id::DEFAULT;
use suzatary_core::secretary::conversations::{self, InMemory};
use suzatary_core::secretary::held_replica::{HeldReplica, NotAnAnswer, Sealing};
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::lines_rule::{LinesRule, SlotKeys};
use suzatary_core::secretary::own_line::OwnLine;
use suzatary_core::secretary::store::{Entry, Kind, Namer, Title};
use suzatary_core::sync::Holding;
use suzatary_memory::secretary::words::Words;
use suzatary_store::admission::Beside as _;
use suzatary_store::replica::held::Node as _;
use suzatary_store::store::{Doc as _, Gets as _, Prodrome as _, Reads as _};

/// The same bytes every time it is asked: two sealers drawing from two of these draw one key.
struct Fixed(AtomicU8);

impl Entropy for Fixed {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(self.0.load(Ordering::Relaxed));
        Ok(())
    }
}

fn at(second: u32) -> Datetime {
    Datetime::new(2026, 11, 17, 9, 0, second, 0).expect("an instant")
}

/// The box's skeleton, whole.
fn skeleton(lines: &InMemory) -> suzatary_core::wire::lines::SkeletonPayload {
    let (epochs, frontiers) = (
        suzatary_signatures::epoch::Epochs::remembered(),
        Frontiers::remembered(KEPT),
    );
    let books = Books {
        epochs: &epochs,
        frontiers: &frontiers,
    };
    routes::lines_skeleton(
        lines.conversations(),
        &Since::Nothing,
        books,
        at(59),
        Cut::Whole,
    )
    .expect("the skeleton")
}

#[test]
fn the_tab_s_own_line_is_the_line_the_box_serves() {
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    suzatary_core::secretary::transcript::start(lines.conversations(), &DEFAULT, &opened)
        .expect("opened");
    let first = skeleton(&lines);
    let mut tab =
        HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(first.clone())).expect("the tab pulled");

    let ada = Actor::new("ada").expect("an actor");
    let title = Title::new("The zephyr filing").expect("a title");
    let (owned, name, _) = tab
        .seal(
            Sealing::Rename {
                conversation: DEFAULT.clone(),
                title: title.clone(),
            },
            at(1),
            &ada,
            &Fixed(AtomicU8::new(9)),
        )
        .expect("sealed");
    let OwnLine::Line { node, print, .. } = &owned[0].node else {
        panic!("a seal writes a line");
    };
    assert_eq!(*node.name(), name);

    // The device keeps the print and the keys alone; they read back to the same own line.
    let back = OwnLine::read(print.clone(), &owned[0].node.keys()).expect("reads back");
    assert_eq!(back, owned[0].node);

    // The box takes it by the lines' rule, beside its signature and its key.
    let slot_keys = handed_over(&lines, &owned[0].node, &ada);

    let served = skeleton(&lines);
    let theirs = served
        .nodes
        .iter()
        .find(|n| n.hash == name.as_str())
        .expect("the box serves the tab's line");
    assert_eq!(
        theirs,
        node.given(),
        "the node the tab computed is the box's"
    );
    let read = conversations::read(lines.conversations(), &DEFAULT).expect("read");
    assert_eq!(
        read.title.get().map(String::as_str),
        Some("The zephyr filing")
    );

    // The box sealing the same words with the same key reads alike.
    let entry = Entry::Titled {
        at: at(1),
        title: Words::kept(title),
        by: Namer::Person {
            actor: "ada".to_owned(),
        },
    };
    let (boxed, box_keys) =
        LineEvent::write(DEFAULT.clone(), ada, 1, &entry, &Fixed(AtomicU8::new(9)))
            .expect("a line");
    let envelope: Envelope<LineEvent> = prodrome_core::event::parse_envelope(
        &suzatary_core::secretary::line_event::LineVocabulary,
        print,
    )
    .expect("parses");
    let tabbed = envelope.event().expect("a line");
    let opens = |keys: &[suzatary_memory::secretary::locked_text::TextKey]| {
        let keys: Vec<_> = keys
            .iter()
            .map(|k| (k.name(), k.bytes().to_vec()))
            .collect();
        move |name: &suzatary_memory::secretary::locked_text::KeyName| {
            keys.iter()
                .find(|(n, _)| n == name)
                .and_then(|(n, b)| suzatary_memory::secretary::locked_text::TextKey::parse(n, b))
                .map_or_else(
                    || suzatary_memory::secretary::words::Gone::Missing.into(),
                    Words::kept,
                )
        }
    };
    assert_eq!(boxed.shape(), tabbed.shape());
    assert_eq!(
        boxed.entry(&opens(&box_keys)),
        tabbed.entry(&opens(slot_keys.of(&name))),
    );
}

/// `own` handed over as a tab posts it, beside its signature and its slots' keys, and taken by the
/// lines' rule as the route takes it: the keys posted.
fn handed_over(lines: &InMemory, own: &OwnLine, ada: &Actor) -> SlotKeys {
    let name = own.name().clone();
    let print = own.print().to_owned();
    let signed: Envelope<LineEvent> = Envelope::Signed(
        suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE).sign(&name),
    );
    let posted = [print, canonical_envelope(&signed)];
    let keys: BTreeMap<String, Vec<String>> =
        [(name.as_str().to_owned(), own.keys().into_values().collect())].into();
    let objects = lines.lines.objects().expect("held");
    let names = objects.iter().map(|(n, _)| n.clone()).collect();
    let held = <LinesRule as suzatary_store::admission::Admission>::held(
        objects.iter().filter_map(|(_, o)| o.event()),
    );
    let trusting = suzatary_signatures::signatures::Trusting::held(
        &[&suzatary_core::stores::phone_trusted(&["ada"])],
        &suzatary_connect::cast::keys::root(),
    )
    .expect("reads");
    let slot_keys = SlotKeys::parse(&keys).expect("the keys parse");
    let received = suzatary_core::sync::receive_changes::<LinesRule>(
        &Holding {
            names: &names,
            held: &held,
        },
        &posted,
        &slot_keys,
        ada,
        &trusting,
    );
    assert!(
        received.outcome.refused.is_empty(),
        "{:?}",
        received.outcome.refused
    );
    for key in slot_keys.of(&name) {
        lines.keys.set(&key.name(), key).expect("kept");
    }
    lines.lines.accept(&received.outcome.taken).expect("taken");

    slot_keys
}

/// The box's acceptance of one posted rename, measured on a store the size of the box's own
/// (`known_pull::store`, about 14,700 lines): the route's whole decision under the lines' lock
/// (the store read, the trust read, the rule, the key kept, the line taken and the receipt).
/// Printed as a measurement; the law is only that it is taken.
#[test]
fn the_box_s_acceptance_of_a_posted_line_measured() {
    let lines = crate::known_pull::store();
    let conversation = crate::known_pull::id(0);
    let mut tab = HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("pulled");
    let ada = Actor::new("ada").expect("an actor");
    let chain = suzatary_core::stores::phone_trusted(&["ada"]);
    let asks: suzatary_core::stores::InMemory<suzatary_core::asks::AskEvent> =
        suzatary_core::stores::InMemory::default();
    let frontiers = Frontiers::remembered(KEPT);
    let mut took = Vec::new();
    for n in 0..5u8 {
        let (owned, name, _) = tab
            .seal(
                Sealing::Rename {
                    conversation: conversation.clone(),
                    title: Title::new(format!("Permits {n}")).expect("a title"),
                },
                at(10 + u32::from(n)),
                &ada,
                &Fixed(AtomicU8::new(40 + n)),
            )
            .expect("sealed");
        let own = &owned[0].node;
        let signed: Envelope<LineEvent> = Envelope::Signed(
            suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE).sign(&name),
        );
        let objects = [own.print().to_owned(), canonical_envelope(&signed)];
        let keys: BTreeMap<String, Vec<String>> =
            [(name.as_str().to_owned(), own.keys().into_values().collect())].into();
        let slot_keys = SlotKeys::parse(&keys).expect("the keys parse");
        let keep = |taken: &[prodrome_core::event::Hash]| {
            for name in taken {
                for key in slot_keys.of(name) {
                    lines.keys.set(&key.name(), key)?;
                }
            }
            Ok(())
        };
        let started = std::time::Instant::now();
        let receipt = routes::receive_changes(
            &lines.lines,
            &routes::Posted {
                objects: &objects,
                since: &Since::Nothing,
            },
            &routes::Beside {
                posted: &slot_keys,
                keep: &keep,
            },
            &frontiers,
            routes::Writer {
                actor: &ada,
                chains: &[&chain],
                root: &suzatary_connect::cast::keys::root(),
            },
            routes::Recording {
                asks: &asks,
                at: at(59),
            },
        )
        .expect("answered");
        took.push(started.elapsed());
        assert!(receipt.adopted.contains(&name.as_str().to_owned()), "taken");
    }
    took.sort();
    // One whole read of the store, the part each of the post's two reads costs, and that read
    // with the DAG the receipt builds of it.
    let timed = |with_dag: bool| {
        let mut runs: Vec<_> = (0..5)
            .map(|_| {
                let started = std::time::Instant::now();
                let held = lines.lines.objects().expect("held");
                let count = held.len();
                if with_dag {
                    let dag: prodrome_core::dag::Dag<LineEvent> = held.into_iter().collect();
                    std::hint::black_box(dag);
                }
                (started.elapsed(), count)
            })
            .collect();
        runs.sort();
        runs[runs.len() / 2]
    };
    let (read, held) = timed(false);
    let (dag, _) = timed(true);
    println!(
        "the box's acceptance of one posted line over {held} objects: median {:?}, slowest {:?}; one whole read of the store: median {read:?}; with its DAG: median {dag:?}",
        took[took.len() / 2],
        took[took.len() - 1],
    );
}

/// The same bytes every time, given.
struct Given([u8; 32]);

impl Entropy for Given {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.copy_from_slice(&self.0[..bytes.len()]);
        Ok(())
    }
}

/// No post makes an erased slot readable: the box destroys the key of the default
/// conversation's title, then a device that kept the key's bytes seals a rename under it and
/// posts it to the lines' route. The line is refused for good, the key store does not take the
/// key back, and the erased title stays closed.
#[test]
fn a_posted_key_never_opens_erased_words() {
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    suzatary_core::secretary::transcript::start(lines.conversations(), &DEFAULT, &opened)
        .expect("opened");
    let mut tab =
        HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let objects = lines.lines.objects().expect("held");
    let line = objects
        .iter()
        .find_map(|(_, o)| o.event())
        .expect("the opening");
    let name = line.keys().next().expect("a slot").clone();
    let kept: [u8; 32] = *lines.keys.get(&name).expect("reads").expect("kept").bytes();
    // The erasure's sweep destroys the key.
    lines.keys.remove(&name).expect("destroyed");

    let ada = Actor::new("ada").expect("an actor");
    let (owned, posted, _) = tab
        .seal(
            Sealing::Rename {
                conversation: DEFAULT.clone(),
                title: Title::new("Garden").expect("a title"),
            },
            at(1),
            &ada,
            &Given(kept),
        )
        .expect("sealed");
    let own = &owned[0].node;
    assert_eq!(
        own.keys().keys().next(),
        Some(&name),
        "sealed under the destroyed key"
    );
    let signed: Envelope<LineEvent> = Envelope::Signed(
        suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE).sign(&posted),
    );
    let objects = [own.print().to_owned(), canonical_envelope(&signed)];
    let keys: BTreeMap<String, Vec<String>> = [(
        posted.as_str().to_owned(),
        own.keys().into_values().collect(),
    )]
    .into();
    let slot_keys = SlotKeys::parse(&keys).expect("the keys parse");
    let keep = |taken: &[prodrome_core::event::Hash]| {
        for taken in taken {
            for key in slot_keys.of(taken) {
                lines.keys.set(&key.name(), key)?;
            }
        }
        Ok(())
    };
    let chain = suzatary_core::stores::phone_trusted(&["ada"]);
    let asks: suzatary_core::stores::InMemory<suzatary_core::asks::AskEvent> =
        suzatary_core::stores::InMemory::default();
    let receipt = routes::receive_changes(
        &lines.lines,
        &routes::Posted {
            objects: &objects,
            since: &Since::Nothing,
        },
        &routes::Beside {
            posted: &slot_keys,
            keep: &keep,
        },
        &Frontiers::remembered(KEPT),
        routes::Writer {
            actor: &ada,
            chains: &[&chain],
            root: &suzatary_connect::cast::keys::root(),
        },
        routes::Recording {
            asks: &asks,
            at: at(59),
        },
    )
    .expect("answered");
    assert!(receipt.adopted.is_empty(), "{receipt:?}");
    assert!(
        receipt.refused.iter().any(|r| r.hash == posted.as_str()),
        "{receipt:?}"
    );
    assert!(
        lines.keys.get(&name).expect("reads").is_none(),
        "the key stays destroyed"
    );
    let read = conversations::read(lines.conversations(), &DEFAULT).expect("read");
    assert_eq!(read.title.get(), None, "the erased title stays closed");
}
