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
use suzatary_api::sync::Holding;
use suzatary_base::capability::Entropy;
use suzatary_base::conversation_id::DEFAULT;
use suzatary_conversations::secretary::store::{Entry, Kind, Namer, Title};
use suzatary_lines::secretary::conversations;
use suzatary_lines::secretary::held_replica::{HeldReplica, NotAnAnswer, Sealing};
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::secretary::lines_rule::{LinesRule, SlotKeys};
use suzatary_lines::secretary::own_line::OwnLine;
use suzatary_lines::secretary::transcript;
use suzatary_memory::secretary::words::Words;
use suzatary_store::admission::Beside as _;
use suzatary_store::replica::held::Node as _;
use suzatary_store::store::{Doc as _, Gets as _, Prodrome as _, Reads as _};
use suzatary_stores::stores::conversations::InMemory;

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
fn skeleton(lines: &InMemory) -> suzatary_lines::wire::lines::SkeletonPayload {
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
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
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
        &suzatary_lines::secretary::line_event::LineVocabulary,
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
    let held = <LinesRule as suzatary_store::admission::Admission>::held(&objects);
    let trusting = suzatary_signatures::signatures::Trusting::held(
        &[&suzatary_stores::stores::phone_trusted(&["ada"])],
        &suzatary_connect::cast::keys::root(),
    )
    .expect("reads");
    let slot_keys = SlotKeys::parse(&keys).expect("the keys parse");
    let received = suzatary_api::sync::receive_changes::<LinesRule>(
        Holding {
            names: &names,
            held,
            world: &OnADevice as &dyn suzatary_lines::secretary::lines_rule::World,
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
#[expect(
    clippy::too_many_lines,
    reason = "one measurement: the store, the post, each step timed"
)]
fn the_box_s_acceptance_of_a_posted_line_measured() {
    let lines = crate::known_pull::store();
    let conversation = crate::known_pull::id(0);
    // A conversation with room for the questions: every generated one holds many unanswered.
    let roomy = suzatary_base::conversation_id::ConversationId::new("measured").expect("an id");
    let opened = Entry::Opened {
        at: at(1),
        title: Words::kept("Measured".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Topic,
    };
    transcript::start(lines.conversations(), &roomy, &opened).expect("opened");
    let mut tab = HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("pulled");
    let ada = Actor::new("ada").expect("an actor");
    let chain = suzatary_stores::stores::phone_trusted(&["ada"]);
    let asks: suzatary_stores::stores::InMemory<suzatary_stores::asks::AskEvent> =
        suzatary_stores::stores::InMemory::default();
    let frontiers = Frontiers::remembered(KEPT);
    // A rename, the line a tab posted before it wrote turns, and a question, whose rule reads its
    // conversation as a transcript.
    let renamed = |n: u8| Sealing::Rename {
        conversation: conversation.clone(),
        title: Title::new(format!("Permits {n}")).expect("a title"),
    };
    let asked = |n: u8| Sealing::Say {
        conversation: roomy.clone(),
        text: format!("is permit {n} due?"),
        references: Vec::new(),
        attachments: Vec::new(),
        seat: suzatary_signatures::signatures::Seat::Device,
    };
    let kinds: [(&str, &dyn Fn(u8) -> Sealing); 2] = [("rename", &renamed), ("question", &asked)];
    for (kind, sealing) in kinds {
        let mut took = Vec::new();
        let mut steps: Vec<suzatary_lines::parts::Parts> = Vec::new();
        for n in 0..5u8 {
            let n = if kind == "rename" { n } else { n + 5 };
            let (owned, name, _) = tab
                .seal(
                    sealing(n),
                    at(10 + u32::from(n)),
                    &ada,
                    &Fixed(AtomicU8::new(40 + n)),
                )
                .expect("sealed");
            let own = &owned[0].node;
            let signed: Envelope<LineEvent> = Envelope::Signed(
                suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE)
                    .sign(&name),
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
            let (receipt, parts) = suzatary_lines::parts::scoped(|| {
                routes::receive_changes(
                    &lines.lines,
                    &routes::Posted {
                        objects: &objects,
                        since: &Since::Nothing,
                    },
                    &routes::Beside {
                        posted: &slot_keys,
                        keep: &keep,
                        world: &OnADevice as &dyn suzatary_lines::secretary::lines_rule::World,
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
            });
            let receipt = receipt.expect("answered");
            took.push(started.elapsed());
            steps.push(parts);
            assert!(
                receipt.adopted.contains(&name.as_str().to_owned()),
                "taken: {receipt:?}"
            );
        }
        took.sort();
        println!(
            "the box's acceptance of one posted {kind}: median {:?}, slowest {:?}; its steps, each a median: {}",
            took[took.len() / 2],
            took[took.len() - 1],
            medians(&steps),
        );
    }
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
        "over {held} objects: one whole read of the store: median {read:?}; with its DAG: median {dag:?}"
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
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
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
    let chain = suzatary_stores::stores::phone_trusted(&["ada"]);
    let asks: suzatary_stores::stores::InMemory<suzatary_stores::asks::AskEvent> =
        suzatary_stores::stores::InMemory::default();
    let receipt = routes::receive_changes(
        &lines.lines,
        &routes::Posted {
            objects: &objects,
            since: &Since::Nothing,
        },
        &routes::Beside {
            posted: &slot_keys,
            keep: &keep,
            world: &OnADevice as &dyn suzatary_lines::secretary::lines_rule::World,
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

/// Each step's median over the posts, in the order the steps ran.
fn medians(steps: &[suzatary_lines::parts::Parts]) -> String {
    steps[0]
        .each()
        .iter()
        .map(|(name, _, _)| {
            let mut spent: Vec<std::time::Duration> = steps
                .iter()
                .filter_map(|parts| parts.each().iter().find(|(n, _, _)| n == name))
                .map(|(_, spent, _)| *spent)
                .collect();
            spent.sort();
            format!("{name} {:?}", spent[spent.len() / 2])
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A post from a device, of a box that keeps no uploads.
struct OnADevice;

impl suzatary_lines::secretary::lines_rule::World for OnADevice {
    fn now(&self) -> prodrome_core::literal::Datetime {
        prodrome_core::literal::Datetime::new(2026, 11, 17, 9, 1, 0, 0).expect("an instant")
    }

    fn seat(&self, _: &prodrome_core::sign::PublicKey) -> suzatary_signatures::signatures::Seat {
        suzatary_signatures::signatures::Seat::Device
    }

    fn upload(
        &self,
        _: &suzatary_held::attachment::AttachmentId,
    ) -> Result<Option<suzatary_lines::secretary::lines_rule::Upload>, String> {
        Ok(None)
    }
}

/// Laws 3 and 4 of the stage's note: a question the tab seals reads as the box's from the same
/// words, actor, instant and stamp, and once a claim covers it, carries the taint the box's does;
/// and no number its person typed (an SSN, a code) is in any byte the tab keeps or posts.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one law's three readings of one question"
)]
fn a_tab_s_question_reads_as_the_box_s_and_holds_no_number_it_was_typed_with() {
    use suzatary_conversations::secretary::model::Turn;
    use suzatary_inbox::secretary::stamp::{Speaker, Stamp};
    let typed = "my number is 123-45-6789 and the verification code is 482913.";
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
    let mut tab =
        HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let ada = Actor::new("ada").expect("an actor");
    let (owned, name, _) = tab
        .seal(
            Sealing::Say {
                conversation: DEFAULT.clone(),
                text: typed.to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
                seat: suzatary_signatures::signatures::Seat::Device,
            },
            at(1),
            &ada,
            &Fixed(AtomicU8::new(11)),
        )
        .expect("sealed");
    let own = &owned[0].node;
    let kept: Vec<String> = std::iter::once(own.print().to_owned())
        .chain(own.keys().into_values())
        .collect();
    for number in ["123-45-6789", "123456789", "482913"] {
        assert!(
            kept.iter().all(|bytes| !bytes.contains(number)),
            "{number} is in what the tab keeps and posts"
        );
    }
    let slot_keys = handed_over(&lines, own, &ada);

    // The box sealing the same turn reads alike.
    let entry = Entry::Said {
        at: at(1),
        turn: Turn::Person {
            text: typed.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some(Stamp::Actor(Speaker::device("ada"))),
    };
    let (boxed, box_keys) =
        LineEvent::write(DEFAULT.clone(), ada, 1, &entry, &Fixed(AtomicU8::new(11)))
            .expect("a line");
    let envelope: Envelope<LineEvent> = prodrome_core::event::parse_envelope(
        &suzatary_lines::secretary::line_event::LineVocabulary,
        own.print(),
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

    // Unjudged, the tab's turn reads tainted; once a claim judges it, it reads as the box's.
    let taint = || {
        let read = transcript::read(lines.conversations(), &DEFAULT).expect("reads");
        suzatary_conversations::secretary::taint::turn(&read.stored().turns[0])
    };
    assert_eq!(
        taint(),
        suzatary_conversations::secretary::taint::Taint::Tainted
    );
    let run = lines.conversations().runs.begin().expect("a run");
    transcript::claim(
        lines.conversations(),
        &DEFAULT,
        &run.run,
        at(2),
        transcript::Retry::Allowed,
        &|_| true,
        |_| Vec::new(),
    )
    .expect("claims");
    let boxed_read = suzatary_conversations::secretary::store::Stored::of(
        &DEFAULT,
        &[opened, boxed.entry(&opens(&box_keys))],
    )
    .expect("a conversation");
    assert_eq!(
        taint(),
        suzatary_conversations::secretary::taint::turn(&boxed_read.turns[0])
    );
}

/// A tab's own turn keeps its words once the box took it and the tab's landed row names it: the
/// hand-over's receipt lands the line's node with no key beside it (a receipt answers nodes, not
/// keys), a later pull starts past it, and the tab holds the line's print so it fetches no content
/// for it; the key the tab sealed it under is the one that opens it, and a purge keeps it.
#[test]
fn an_own_turn_reads_its_words_after_the_skeleton_names_it() {
    use suzatary_conversations::secretary::model::Turn;
    use suzatary_lines::secretary::held_replica::Whole;
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
    let mut tab =
        HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let ada = Actor::new("ada").expect("an actor");
    let said = "Is the zephyr permit due this week?";
    let (owned, _, _) = tab
        .seal(
            Sealing::Say {
                conversation: DEFAULT.clone(),
                text: said.to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
                seat: suzatary_signatures::signatures::Seat::Device,
            },
            at(1),
            &ada,
            &Fixed(AtomicU8::new(13)),
        )
        .expect("sealed");
    handed_over(&lines, &owned[0].node, &ada);

    // The line landed as a receipt lands it, no key beside it; the opening's content fetched.
    let served = skeleton(&lines);
    let opening: Vec<_> = served
        .nodes
        .iter()
        .filter(|n| n.hash != owned[0].node.name().as_str())
        .map(|n| prodrome_core::event::Hash::new(n.hash.clone()).expect("a name"))
        .collect();
    tab.piece(
        served,
        suzatary_memory::secretary::key_ring::KeyRing::default(),
        1,
    )
    .expect("a piece");
    let _ = tab.land();
    let prints: Vec<_> = opening
        .iter()
        .zip(lines.lines.literals(&opening).expect("reads"))
        .filter_map(|(name, literal)| Some((name.clone(), literal?.into_bytes())))
        .collect();
    tab.lines.hold(prints, &[]);
    // A purge husks each line no key held opens; the own line's key is held.
    let _ = tab.forget(std::collections::BTreeSet::new());
    let whole = tab.transcript(&DEFAULT);
    let Some(Whole::Read(Ok(read))) = whole else {
        panic!("the tab reads the conversation whole: {whole:?}");
    };
    let turns = &read.stored().turns;
    assert!(
        matches!(&turns[0].turn, Turn::Person { text, .. } if text == said),
        "{:?}",
        turns[0].turn
    );
}

/// A taken own line's record may be let go once the landed rows hold its keys, and not before:
/// the box's skeleton hands a line's keys beside its node, so a pull that lands the line frees it.
#[test]
fn a_taken_own_line_is_free_once_its_keys_land() {
    use suzatary_store::replica::writes::Answer;
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
    let mut tab =
        HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let ada = Actor::new("ada").expect("an actor");
    let (owned, name, _) = tab
        .seal(
            Sealing::Say {
                conversation: DEFAULT.clone(),
                text: "Is the zephyr permit due this week?".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
                seat: suzatary_signatures::signatures::Seat::Device,
            },
            at(1),
            &ada,
            &Fixed(AtomicU8::new(17)),
        )
        .expect("sealed");
    handed_over(&lines, &owned[0].node, &ada);
    let _ = tab.answered(vec![(name.clone(), Answer::Taken)]);
    let free = |tab: &HeldReplica| -> Vec<bool> {
        tab.own_taken()
            .into_iter()
            .filter(|(own, _)| *own.name() == name)
            .map(|(_, free)| free)
            .collect()
    };
    assert_eq!(free(&tab), [false], "taken, and its keys in no landed row");

    let served = skeleton(&lines);
    let keys: BTreeMap<_, _> = served
        .keys
        .iter()
        .map(|(n, k)| {
            (
                suzatary_memory::secretary::locked_text::KeyName::new(n.clone()).expect("a name"),
                k.clone(),
            )
        })
        .collect();
    tab.piece(
        served,
        suzatary_memory::secretary::key_ring::KeyRing::of(&keys),
        1,
    )
    .expect("a piece");
    let _ = tab.land();
    assert_eq!(free(&tab), [true], "its keys landed with its node");
}

/// An erasure reaches a tab's own line as it reaches a landed one: the owner erases the tab's
/// question from another device, and the tab, which lands the skeleton and fetches no content,
/// holds the line's key no longer, and may let its own record go.
#[test]
fn an_own_line_erased_elsewhere_lets_its_key_go() {
    use suzatary_store::replica::writes::Answer;
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
    let mut tab =
        HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let ada = Actor::new("ada").expect("an actor");
    let (owned, name, _) = tab
        .seal(
            Sealing::Say {
                conversation: DEFAULT.clone(),
                text: "Is the zephyr permit due this week?".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
                seat: suzatary_signatures::signatures::Seat::Device,
            },
            at(1),
            &ada,
            &Fixed(AtomicU8::new(19)),
        )
        .expect("sealed");
    let key_names: std::collections::BTreeSet<_> = owned[0].node.keys().into_keys().collect();
    handed_over(&lines, &owned[0].node, &ada);
    let _ = tab.answered(vec![(name.clone(), Answer::Taken)]);

    transcript::erase(
        lines.conversations(),
        &DEFAULT,
        key_names.clone(),
        suzatary_memory::secretary::erasure::Erasure::Asked { by: ada.clone() },
        at(2),
    )
    .expect("erased");
    // The skeleton landed with the keys the box still sends; no content is fetched.
    let served = skeleton(&lines);
    let keys: BTreeMap<_, _> = served
        .keys
        .iter()
        .map(|(n, k)| {
            (
                suzatary_memory::secretary::locked_text::KeyName::new(n.clone()).expect("a name"),
                k.clone(),
            )
        })
        .collect();
    tab.piece(
        served,
        suzatary_memory::secretary::key_ring::KeyRing::of(&keys),
        1,
    )
    .expect("a piece");
    let _ = tab.land();

    let (_, read) = tab.reading();
    for key in &key_names {
        assert!(
            !read.key_ring().holds(key),
            "{key} still opens the erased words"
        );
    }
    let free: Vec<bool> = tab
        .own_taken()
        .into_iter()
        .filter(|(own, _)| *own.name() == name)
        .map(|(_, free)| free)
        .collect();
    assert_eq!(free, [true], "an erased line's record may go");
}

/// The box's acceptance of a post at the route's limit, 256 renames and their signatures (512
/// objects), each resting on the one before, into one conversation of the store the size of the
/// box's own: every rename is taken, so the conversation is read again before each next line is
/// judged. Printed as a measurement; run with `--release --ignored`.
#[test]
#[ignore = "a measurement, run by hand"]
fn the_box_s_acceptance_of_a_full_post_measured() {
    let lines = crate::known_pull::store();
    let conversation = crate::known_pull::id(0);
    let mut tab = HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("pulled");
    let ada = Actor::new("ada").expect("an actor");
    let chain = suzatary_stores::stores::phone_trusted(&["ada"]);
    let asks: suzatary_stores::stores::InMemory<suzatary_stores::asks::AskEvent> =
        suzatary_stores::stores::InMemory::default();
    let frontiers = Frontiers::remembered(KEPT);
    let mut objects = Vec::new();
    let mut keys: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for n in 0..256u32 {
        let (owned, name, _) = tab
            .seal(
                Sealing::Rename {
                    conversation: conversation.clone(),
                    title: Title::new(format!("Permits {n}")).expect("a title"),
                },
                at(10 + n % 45),
                &ada,
                &Given(seed(n)),
            )
            .expect("sealed");
        let own = &owned[0].node;
        let signed: Envelope<LineEvent> = Envelope::Signed(
            suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE).sign(&name),
        );
        objects.push(own.print().to_owned());
        objects.push(canonical_envelope(&signed));
        keys.insert(name.as_str().to_owned(), own.keys().into_values().collect());
    }
    let slot_keys = SlotKeys::parse(&keys).expect("the keys parse");
    let keep = |_: &[prodrome_core::event::Hash]| Ok(());
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
            world: &OnADevice as &dyn suzatary_lines::secretary::lines_rule::World,
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
    let took = started.elapsed();
    assert_eq!(
        receipt.adopted.len(),
        512,
        "every rename taken: {receipt:?}"
    );
    println!("the box's acceptance of a post of 256 renames and their signatures: {took:?}");
}

/// A key seed of its own for each `n`.
fn seed(n: u32) -> [u8; 32] {
    let mut bytes = [7u8; 32];
    bytes[..4].copy_from_slice(&n.to_be_bytes());
    bytes
}
