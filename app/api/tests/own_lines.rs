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
use suzatary_lines::secretary::held_replica::{AskedAgain, HeldReplica, NotAnAnswer, Sealing};
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
    let history = lines.lines.objects().expect("held").into_iter().collect();
    let trusting = suzatary_signatures::signatures::Trusting::held(
        &suzatary_signatures::signatures::Keeping::stores(&[
            &suzatary_stores::stores::phone_trusted(&["ada"]),
        ]),
        &suzatary_connect::cast::keys::root(),
    )
    .expect("reads");
    let slot_keys = SlotKeys::parse(&keys).expect("the keys parse");
    let received = suzatary_api::sync::receive_changes::<LinesRule>(
        Holding {
            history: &history,
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
/// Printed as a measurement; the law is only that it is taken. Measured again with the store's
/// first conversation holding 5,000 of its lines (`known_pull::shaped`), which the renames go to.
#[test]
fn the_box_s_acceptance_of_a_posted_line_measured() {
    acceptance_measured(&crate::known_pull::store(), "spread");
    acceptance_measured(&crate::known_pull::shaped(2_500), "long");
}

#[expect(
    clippy::too_many_lines,
    reason = "one measurement: the store, the post, each step timed"
)]
fn acceptance_measured(lines: &InMemory, shape: &str) {
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
    let mut tab = HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(lines))).expect("pulled");
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
        question: suzatary_lines::secretary::held_replica::Question {
            text: format!("is permit {n} due?"),
            references: Vec::new(),
            attachments: Vec::new(),
            seat: suzatary_signatures::signatures::Seat::Device,
        },
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
                        chains: &suzatary_signatures::signatures::Keeping::stores(&[&chain]),
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
        let mut locked: Vec<std::time::Duration> = steps.iter().map(under_the_lock).collect();
        locked.sort();
        println!(
            "{shape}: the box's acceptance of one posted {kind}: median {:?}, slowest {:?}, the lock held {:?}; its steps, each a median: {}",
            took[took.len() / 2],
            took[took.len() - 1],
            locked[locked.len() / 2],
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
        "{shape}: over {held} objects: one whole read of the store: median {read:?}; with its DAG: median {dag:?}"
    );
}

/// How long a post held the store's lock: every part of the route but those after it (the
/// receipt's history, its frontier and the asks' record).
fn under_the_lock(parts: &suzatary_lines::parts::Parts) -> std::time::Duration {
    parts
        .each()
        .iter()
        .filter(|(name, _, _)| !matches!(*name, "dag" | "frontier" | "record"))
        .map(|(_, spent, _)| *spent)
        .sum()
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
            chains: &suzatary_signatures::signatures::Keeping::stores(&[&chain]),
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
pub(crate) struct OnADevice;

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
                question: suzatary_lines::secretary::held_replica::Question {
                    text: typed.to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                    seat: suzatary_signatures::signatures::Seat::Device,
                },
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
                question: suzatary_lines::secretary::held_replica::Question {
                    text: said.to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                    seat: suzatary_signatures::signatures::Seat::Device,
                },
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
                question: suzatary_lines::secretary::held_replica::Question {
                    text: "Is the zephyr permit due this week?".to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                    seat: suzatary_signatures::signatures::Seat::Device,
                },
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
                question: suzatary_lines::secretary::held_replica::Question {
                    text: "Is the zephyr permit due this week?".to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                    seat: suzatary_signatures::signatures::Seat::Device,
                },
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
/// judged. Printed as a measurement; run with `--release --ignored`. Measured again with that
/// conversation holding 5,000 of the store's lines (`known_pull::shaped`).
#[test]
#[ignore = "a measurement, run by hand"]
fn the_box_s_acceptance_of_a_full_post_measured() {
    full_post_measured(&crate::known_pull::store(), "spread");
    full_post_measured(&crate::known_pull::shaped(2_500), "long");
}

fn full_post_measured(lines: &InMemory, shape: &str) {
    let conversation = crate::known_pull::id(0);
    let mut tab = HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(lines))).expect("pulled");
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
                chains: &suzatary_signatures::signatures::Keeping::stores(&[&chain]),
                root: &suzatary_connect::cast::keys::root(),
            },
            routes::Recording {
                asks: &asks,
                at: at(59),
            },
        )
    });
    let receipt = receipt.expect("answered");
    let took = started.elapsed();
    assert_eq!(
        receipt.adopted.len(),
        512,
        "every rename taken: {receipt:?}"
    );
    println!(
        "{shape}: the box's acceptance of a post of 256 renames and their signatures: {took:?}, the lock held {:?}; its steps: {}",
        under_the_lock(&parts),
        parts.line("")
    );
}

/// A key seed of its own for each `n`.
/// A question on the default conversation.
/// Each ruling by the name of the own line it is of.
fn names(
    rulings: Vec<(&OwnLine, suzatary_lines::secretary::held_replica::Ruling)>,
) -> Vec<(
    prodrome_core::event::Hash,
    suzatary_lines::secretary::held_replica::Ruling,
)> {
    rulings
        .into_iter()
        .map(|(own, ruling)| (own.name().clone(), ruling))
        .collect()
}

fn say(text: &str) -> Sealing {
    Sealing::Say {
        conversation: DEFAULT.clone(),
        question: suzatary_lines::secretary::held_replica::Question {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
            seat: suzatary_signatures::signatures::Seat::Device,
        },
    }
}

fn seed(n: u32) -> [u8; 32] {
    let mut bytes = [7u8; 32];
    bytes[..4].copy_from_slice(&n.to_be_bytes());
    bytes
}

/// A ruling is of a line the box does not hold: an own line whose post's answer was lost after the
/// box took it reads `Unsent` until a pull lands it, and none after, whatever its answer says.
#[test]
fn a_line_the_rows_hold_has_no_ruling_whatever_its_answer() {
    use suzatary_lines::secretary::held_replica::Ruling;
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
                question: suzatary_lines::secretary::held_replica::Question {
                    text: "Is the zephyr permit due this week?".to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                    seat: suzatary_signatures::signatures::Seat::Device,
                },
            },
            at(1),
            &ada,
            &Fixed(AtomicU8::new(23)),
        )
        .expect("sealed");
    assert_eq!(
        names(tab.rulings(&DEFAULT)),
        [(name.clone(), Ruling::Unsent)]
    );
    // The box takes it, and its answer never reaches the tab.
    handed_over(&lines, &owned[0].node, &ada);
    tab.piece(
        skeleton(&lines),
        suzatary_memory::secretary::key_ring::KeyRing::default(),
        1,
    )
    .expect("a piece");
    let _ = tab.land();
    assert_eq!(names(tab.rulings(&DEFAULT)), [], "the landed rows hold it");
}

/// A line the box refused for good leaves the tab's transcript at once, as it leaves the
/// skeleton's outlines: the reading is the box's again, and the words stay on the ruling. Before,
/// its print stayed among the prints every transcript reads, so a refused question still read as
/// a turn until the device evicted it.
#[test]
fn a_refused_own_line_leaves_the_tab_s_transcript() {
    use suzatary_lines::secretary::held_replica::{Ruling, Whole};
    use suzatary_store::replica::writes::{Answer, Refusal};
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
    let held: Vec<_> = lines.lines.objects().expect("held").into_iter().collect();
    tab.lines.hold(
        held.iter()
            .map(|(name, object)| (name.clone(), canonical_envelope(object).into_bytes()))
            .collect(),
        &[],
    );
    let ada = Actor::new("ada").expect("an actor");
    let (_, name, _) = tab
        .seal(
            say("Is the zephyr permit due this week?"),
            at(1),
            &ada,
            &Fixed(AtomicU8::new(29)),
        )
        .expect("sealed");
    let turns = |tab: &mut HeldReplica| match tab.transcript(&DEFAULT) {
        Some(Whole::Read(Ok(read))) => read.stored().turns.len(),
        other => panic!("the tab reads {other:?}"),
    };
    assert_eq!(turns(&mut tab), 1, "the question reads at once");
    let never = Answer::Refused {
        kind: Refusal::Final,
        reason: "never".to_owned(),
    };
    let _ = tab.answered(vec![(name.clone(), never)]);
    assert_eq!(turns(&mut tab), 0, "the refused question is no turn");
    assert!(
        matches!(names(tab.rulings(&DEFAULT)).as_slice(), [(n, Ruling::Refused { .. })] if *n == name),
        "its words stay on its ruling"
    );
}

/// An edit's question refused for good gives its words back as an edit of the question it edited,
/// so the composer takes them back in its editing mode and Send seals a fresh pair.
#[test]
fn a_refused_edit_gives_its_words_back_as_an_edit() {
    use suzatary_lines::secretary::held_replica::Ruling;
    use suzatary_store::replica::writes::{Answer, Refusal};
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
    let (_, asked, _) = tab
        .seal(
            say("Is the zephyr permit due?"),
            at(1),
            &ada,
            &Fixed(AtomicU8::new(31)),
        )
        .expect("sealed");
    let (owned, again, _) = tab
        .seal(
            Sealing::Again {
                conversation: DEFAULT.clone(),
                question: asked.clone(),
                how: AskedAgain::Edit(suzatary_lines::secretary::held_replica::Question {
                    text: "Is the zephyr permit due this week?".to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                    seat: suzatary_signatures::signatures::Seat::Device,
                }),
                seen: vec![asked.clone()],
            },
            at(2),
            &ada,
            &Fixed(AtomicU8::new(37)),
        )
        .expect("sealed");
    let edited = owned
        .iter()
        .find(|own| own.node.rests_on().contains(&again))
        .map(|own| own.node.name().clone())
        .expect("the edit's question");
    let never = Answer::Refused {
        kind: Refusal::Final,
        reason: "never".to_owned(),
    };
    let _ = tab.answered(vec![(edited.clone(), never)]);
    let ruled = names(tab.rulings(&DEFAULT));
    assert!(
        ruled.iter().any(|(name, ruling)| *name == edited
            && matches!(ruling, Ruling::Refused { words: suzatary_lines::secretary::own_line::Words::Edit { question, text, .. }, .. }
                if *question == asked && text == "Is the zephyr permit due this week?")),
        "{ruled:?}"
    );
}

/// A refused topic is defined by what the device wrote: a conversation whose opening is one of its
/// own lines refused for good. Its opening's title and the question resting on it are elsewhere; a
/// refused question of a conversation the device holds is not, and is not after the rows start
/// over either, when they lack it. A refused line the rows hold all the same has no ruling.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one law over a refused topic, a held conversation, a reset and a line the rows took"
)]
fn a_refused_topic_is_one_whose_opening_this_device_wrote() {
    use suzatary_lines::secretary::held_replica::Ruling;
    use suzatary_store::replica::writes::{Answer, Refusal};
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
    let question = |text: &str| suzatary_lines::secretary::held_replica::Question {
        text: text.to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
        seat: suzatary_signatures::signatures::Seat::Device,
    };
    let (begun, opening, _) = tab
        .seal(
            Sealing::Begin {
                title: Some(Title::new("Permits").expect("a title")),
                question: question("Is the permit due?"),
            },
            at(1),
            &ada,
            &Fixed(AtomicU8::new(31)),
        )
        .expect("sealed");
    let (_, held_question, _) = tab
        .seal(
            Sealing::Say {
                conversation: DEFAULT.clone(),
                question: question("And the fee?"),
            },
            at(2),
            &ada,
            &Fixed(AtomicU8::new(41)),
        )
        .expect("sealed");
    let never = |name: &prodrome_core::event::Hash| {
        (
            name.clone(),
            Answer::Refused {
                kind: Refusal::Final,
                reason: "say something".to_owned(),
            },
        )
    };
    let _ = tab.answered(vec![never(&opening), never(&held_question)]);
    let topic = begun
        .iter()
        .find_map(|own| own.node.conversation())
        .expect("its conversation")
        .to_owned();
    let elsewhere = |tab: &HeldReplica| -> Vec<String> {
        tab.refused_elsewhere()
            .into_iter()
            .map(|(_, ruling)| match ruling {
                Ruling::Refused { words, .. } => words.quoted().to_owned(),
                Ruling::Unsent | Ruling::Trust | Ruling::Later { .. } => String::new(),
            })
            .collect()
    };
    let mut found = elsewhere(&tab);
    found.sort();
    assert_eq!(
        found,
        ["Is the permit due?", "Permits"],
        "the topic's title and question"
    );
    assert_eq!(
        tab.refused_topics()
            .keys()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        [topic],
        "the held conversation's refused question is no refused topic"
    );
    let _ = tab.start_over();
    assert_eq!(
        elsewhere(&tab).len(),
        2,
        "nothing of a held conversation, after the rows start over"
    );
    let words: std::collections::BTreeSet<_> = tab
        .refused_elsewhere()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let _ = tab.acted(&words);
    assert!(
        tab.refused_topics().is_empty(),
        "a refused topic whose words the person acted on whole is absent"
    );

    // Another tab's post took the held question all the same: the rows hold it, and it has no ruling.
    let mut fresh =
        HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let (owned, name, _) = fresh
        .seal(
            Sealing::Say {
                conversation: DEFAULT.clone(),
                question: question("Is the deed signed?"),
            },
            at(3),
            &ada,
            &Fixed(AtomicU8::new(51)),
        )
        .expect("sealed");
    let _ = fresh.answered(vec![never(&name)]);
    assert_eq!(
        fresh.rulings(&DEFAULT).len(),
        1,
        "refused, and the rows lack it"
    );
    handed_over(&lines, &owned[0].node, &ada);
    fresh
        .piece(
            skeleton(&lines),
            suzatary_memory::secretary::key_ring::KeyRing::default(),
            1,
        )
        .expect("a piece");
    let _ = fresh.land();
    assert!(fresh.holds(&name));
    assert!(
        fresh.rulings(&DEFAULT).is_empty(),
        "a line the rows hold has no ruling"
    );
}

/// A tab that missed the wake of a let-go never posts the record again nor keeps it unsent: one tab
/// is told of a refused question, the person acts on it, and the tab lets it go; another, which
/// read the question from the row before its refusal, reads the act before it seals, so the
/// question is refused for good to it too ([`suzatary_lines::secretary::held_replica::LET_GO`]):
/// it posts nothing of it, and its new question rests on what the conversation holds instead.
#[test]
fn a_tab_that_missed_a_let_go_never_posts_it_again() {
    use std::collections::BTreeSet;
    use suzatary_store::replica::writes::{Answer, Fit, Refusal};
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
    let pulled =
        || HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let ada = Actor::new("ada").expect("an actor");
    let mut first = pulled();
    let (_, refused, _) = first
        .seal(
            say("Is the permit due?"),
            at(1),
            &ada,
            &Fixed(AtomicU8::new(81)),
        )
        .expect("sealed");
    // The second tab reads the row before the box's refusal reaches it: to it the line is unsent.
    let mut second = pulled();
    let _ = second.take_own(&first.own().iter().cloned().collect::<Vec<_>>());
    let _ = first.answered(vec![(
        refused.clone(),
        Answer::Refused {
            kind: Refusal::Final,
            reason: "never".to_owned(),
        },
    )]);
    let acted: BTreeSet<_> = [refused.clone()].into();
    let _ = first.acted(&acted);
    let released = first.releasable(&BTreeSet::new());
    assert_eq!(released.refused, acted, "let go as refused, by name");
    // The second reads the row, which no longer holds it, and the acts, as a seal does.
    let _ = second.acted(&acted);
    let posts = |tab: &HeldReplica| -> Vec<prodrome_core::event::Hash> {
        tab.unsent(
            Fit {
                most: 1000,
                bytes: usize::MAX,
            },
            |_| 1,
        )
        .into_iter()
        .flatten()
        .map(|own| own.name().clone())
        .collect()
    };
    assert!(!posts(&second).contains(&refused), "never posted again");
    let (owned, asked, _) = second
        .seal(say("And the fee?"), at(2), &ada, &Fixed(AtomicU8::new(91)))
        .expect("sealed");
    let new = owned
        .iter()
        .find(|own| *own.node.name() == asked)
        .expect("the new line");
    assert!(
        !new.node.rests_on().contains(&refused),
        "it rests on what the conversation holds"
    );
    assert!(
        owned.iter().all(|own| *own.node.name() != refused),
        "nothing of the refused question is kept again: {owned:?}"
    );
    assert!(posts(&second).contains(&asked));
    assert!(!posts(&second).contains(&refused));
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(48))]

    /// The core alone answers which own records the device may let go, and a reload of what the
    /// device keeps after it let them go reads the same: for a topic begun with no name and the
    /// three questions that rest on it in a chain, any of them refused, told of and acted on in any
    /// measure, no record goes while a line resting on it is still held back, every refused line
    /// that gives words back stays ruled until the person acts on it, and the reloaded tab, asked
    /// again, lets go of nothing a line it keeps rests on.
    #[test]
    fn no_refused_record_goes_while_a_line_resting_on_it_is_held_back(
        refused in proptest::collection::vec(proptest::prelude::any::<bool>(), 4),
        told in proptest::collection::vec(proptest::prelude::any::<bool>(), 4),
        acted in proptest::collection::vec(proptest::prelude::any::<bool>(), 4),
    ) {
        use std::collections::BTreeSet;
        use suzatary_store::replica::writes::{Answer, Refusal};
        let lines = InMemory::default();
        let opened = Entry::Opened {
            at: at(0),
            title: Words::kept("Secretary".to_owned()),
            model: "a-cli/small".to_owned(),
            kind: Kind::Default,
        };
        transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
        let pulled = || HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
        let mut tab = pulled();
        let ada = Actor::new("ada").expect("an actor");
        let question = |n: usize| suzatary_lines::secretary::held_replica::Question {
            text: format!("question {n}"),
            references: Vec::new(),
            attachments: Vec::new(),
            seat: suzatary_signatures::signatures::Seat::Device,
        };
        // The opening (no name, so no words) and the chain of questions resting on it, in order.
        let (begun, opening, _) = tab
            .seal(Sealing::Begin { title: None, question: question(0) }, at(1), &ada, &Fixed(AtomicU8::new(61)))
            .expect("sealed");
        let topic = suzatary_base::conversation_id::ConversationId::new(
            begun.iter().find_map(|own| own.node.conversation()).expect("its conversation"),
        )
        .expect("an id");
        let questioned: Vec<_> = begun
            .iter()
            .map(|own| own.node.name().clone())
            .filter(|name| *name != opening)
            .collect();
        let mut chain = vec![opening];
        chain.extend(questioned);
        // Four records deep on purpose: the opening, its question, and two questions over it.
        for n in 1..3usize {
            let (_, name, _) = tab
                .seal(
                    Sealing::Say { conversation: topic.clone(), question: question(n) },
                    at(2 + u32::try_from(n).expect("small")),
                    &ada,
                    &Fixed(AtomicU8::new(70 + u8::try_from(n).expect("small"))),
                )
                .expect("sealed");
            chain.push(name);
        }
        let never = Answer::Refused { kind: Refusal::Final, reason: "never".to_owned() };
        let _ = tab.answered(
            chain.iter().zip(&refused).filter(|(_, r)| **r).map(|(name, _)| (name.clone(), never.clone())).collect(),
        );
        let prints: BTreeMap<_, _> = tab.own().iter().map(|own| (own.node.name().clone(), own.node.print().to_owned())).collect();
        let told: BTreeSet<String> = chain.iter().zip(&told).filter(|(_, t)| **t).map(|(name, _)| prints[name].clone()).collect();
        let acted: BTreeSet<prodrome_core::event::Hash> = chain.iter().zip(&acted).filter(|(_, a)| **a).map(|(name, _)| name.clone()).collect();
        let _ = tab.acted(&acted);
        let going = tab.releasable(&told).prints;
        let held_back: BTreeSet<_> = tab.own().refusals().into_iter().map(|(own, _, _)| own.name().clone()).collect();

        // No record goes while a line still held back rests on it.
        for own in tab.own().iter() {
            if !held_back.contains(own.node.name()) || going.contains(own.node.print()) {
                continue;
            }
            // A line rests on its genesis too, which is no own record.
            for print in own.node.rests_on().iter().filter_map(|parent| prints.get(parent)) {
                proptest::prop_assert!(!going.contains(print), "a parent of a line still held back goes");
            }
        }
        // The device keeps what does not go, and a reload reads the same rulings.
        let mut reloaded = pulled();
        let kept: Vec<_> = tab.own().iter().filter(|own| !going.contains(own.node.print())).cloned().collect();
        let _ = reloaded.take_own(&kept);
        let _ = reloaded.acted(&acted);
        // Ruled on the topic's page, or on the page that begins topics where its opening was refused.
        let ruled: BTreeSet<_> = names(reloaded.rulings(&topic))
            .into_iter()
            .chain(reloaded.refused_elsewhere())
            .map(|(name, _)| name)
            .collect();
        for name in &chain {
            let worded = name != &chain[0];
            if held_back.contains(name) && worded && !acted.contains(name) {
                proptest::prop_assert!(ruled.contains(name), "a refused question not acted on is ruled after a reload");
                proptest::prop_assert!(!going.contains(&prints[name]));
            }
        }
        // The reloaded tab asked again: what it keeps rests on nothing either answer let go.
        let again = reloaded.releasable(&told).prints;
        let gone: BTreeSet<&String> = going.iter().chain(&again).collect();
        for own in reloaded.own().iter().filter(|own| !gone.contains(&own.node.print().to_owned())) {
            for print in own.node.rests_on().iter().filter_map(|parent| prints.get(parent)) {
                proptest::prop_assert!(!gone.contains(print), "a line kept rests on a record let go");
            }
        }
    }
}

/// One step of two tabs over the device's row, as the tab's worker takes it.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// The row read into the tab's core and settled: its records, then its acts, then what the
    /// core lets go removed, the refused ones kept as acted on.
    Settle,
    /// The row read, a question sealed over what the tab holds, and kept in the row.
    Seal,
    /// The row read, the latest question either tab sealed asked again over itself, in new
    /// words (`true`) or as it was, and kept in the row.
    Again(bool),
    /// The box refuses the tab's first unsent question for good, and the tab keeps the answer; the
    /// person acts on it at once (`true`), or not yet.
    Refuse(bool),
    /// The box takes the tab's first unsent question it can take; the answer reaches the tab, or
    /// is lost (`true`).
    Take(bool),
    /// The tab lands what the box serves, with its keys.
    Land,
    /// The tab hands over what it holds unsent, after reading the row.
    Post,
    /// The person acts on every refusal with words the tab shows.
    Act,
}

fn step() -> impl proptest::strategy::Strategy<Value = (usize, Step)> {
    use proptest::prelude::*;
    (
        0usize..2,
        prop_oneof![
            Just(Step::Settle),
            Just(Step::Seal),
            any::<bool>().prop_map(Step::Again),
            any::<bool>().prop_map(Step::Refuse),
            any::<bool>().prop_map(Step::Take),
            Just(Step::Land),
            Just(Step::Post),
            Just(Step::Act),
        ],
    )
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(1000))]

    /// For any order of two tabs' reads, seals (questions, regenerates and edits), takings (the
    /// answer lost or not), refusals,
    /// landings and acts over one row: no record stays in the row whose line the rows hold once
    /// its keys are held, whatever answer it carries; no record let go as refused is posted again
    /// by either tab; every refusal for good with words the person has not acted on stays ruled
    /// and is let go by no settle; the box takes every line a tab hands over whose parents it
    /// holds; and every line a tab seals rests on parents none of which lies beneath another, once
    /// the box holds them all.
    #[test]
    fn two_tabs_over_one_row_keep_no_taken_record_and_post_no_let_go_one(
        steps in proptest::collection::vec(step(), 1..24),
    ) {
        two_tabs(steps)?;
    }
}

/// The case the law above found: tab one seals P, then A on P; the box takes P and the answer is
/// lost; tab one lands P and lets P's record go. Tab two reads the row and finds A alone, not the
/// line it rests on, so it seals nothing until a pull brings P, and then seals over A alone;
/// before, it sealed C over A and the opening, which P already rests on, and the box took C.
#[test]
fn a_tab_that_lacks_what_a_line_rests_on_seals_nothing_over_it() {
    let sealed = two_tabs(vec![
        (0, Step::Seal),
        (0, Step::Seal),
        (0, Step::Take(true)),
        (0, Step::Land),
        (0, Step::Settle),
        (1, Step::Seal),
        (0, Step::Take(false)),
        (1, Step::Take(false)),
        (1, Step::Land),
        (1, Step::Seal),
        (1, Step::Take(false)),
    ])
    .expect("the law holds");
    let [p, a, d] = sealed.as_slice() else {
        panic!("tab one's two lines, then tab two's once it pulled: {sealed:?}");
    };
    assert!(a.rests_on().contains(p.name()));
    assert!(
        d.rests_on().contains(a.name()) && !d.rests_on().contains(p.name()),
        "tab two's line rests on A, and on nothing beneath it: {:?}",
        d.rests_on()
    );
}

#[expect(
    clippy::too_many_lines,
    reason = "one law: the two tabs, the row, the box, each step"
)]
/// The law over `steps`: the lines the tabs sealed, in order.
fn two_tabs(
    steps: Vec<(usize, Step)>,
) -> Result<Vec<OwnLine>, proptest::test_runner::TestCaseError> {
    use std::collections::BTreeSet;
    use suzatary_store::replica::writes::{Answer, Fit, Refusal, Writes};
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
    let pulled =
        || HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let ada = Actor::new("ada").expect("an actor");
    let mut tabs = [pulled(), pulled()];
    let mut row: Writes<OwnLine> = Writes::default();
    let mut acted_row: BTreeSet<prodrome_core::event::Hash> = BTreeSet::new();
    let mut told: BTreeSet<String> = BTreeSet::new();
    let mut let_go: BTreeSet<prodrome_core::event::Hash> = BTreeSet::new();
    let mut sealed: Vec<OwnLine> = Vec::new();
    let fit = Fit {
        most: 1000,
        bytes: usize::MAX,
    };
    let unsent_lines = |tab: &HeldReplica| -> Vec<OwnLine> {
        tab.unsent(fit, |_| 1)
            .into_iter()
            .flatten()
            .filter(|own| matches!(own, OwnLine::Line { .. }))
            .cloned()
            .collect()
    };
    let read = |tab: &mut HeldReplica,
                row: &Writes<OwnLine>,
                acted_row: &BTreeSet<prodrome_core::event::Hash>| {
        let _ = tab.take_own(&row.iter().cloned().collect::<Vec<_>>());
        let _ = tab.acted(acted_row);
    };
    let held = || -> BTreeSet<prodrome_core::event::Hash> {
        lines
            .lines
            .objects()
            .expect("held")
            .into_iter()
            .map(|(n, _)| n)
            .collect()
    };
    for (n, (t, step)) in steps.into_iter().enumerate() {
        let seq = u8::try_from(n).expect("few steps");
        match step {
            Step::Settle => {
                read(&mut tabs[t], &row, &acted_row);
                // Every refusal for good with words the person has not acted on stays ruled.
                let worded: Vec<OwnLine> = tabs[t]
                    .own()
                    .refusals()
                    .into_iter()
                    .filter(|(own, _, _)| own.words().is_some() && !tabs[t].holds(own.name()))
                    .filter(|(own, _, _)| !acted_row.contains(own.name()))
                    .map(|(own, _, _)| own.clone())
                    .collect();
                let ruled = names(tabs[t].rulings(&DEFAULT));
                let released = tabs[t].releasable(&told);
                for own in &worded {
                    use suzatary_lines::secretary::held_replica::Ruling;
                    proptest::prop_assert!(
                        ruled.iter().any(|(name, ruling)| name == own.name()
                            && matches!(ruling, Ruling::Refused { .. })),
                        "a refusal with words not acted on is not ruled"
                    );
                    proptest::prop_assert!(
                        !released.prints.contains(own.print())
                            && !released.refused.contains(own.name()),
                        "a refusal with words not acted on is let go"
                    );
                }
                acted_row.extend(released.refused.iter().cloned());
                let_go.extend(released.refused);
                row = Writes::of(
                    &row.iter()
                        .filter(|own| !released.prints.contains(own.node.print()))
                        .cloned()
                        .collect::<Vec<_>>(),
                );
                // Nothing whose line this tab's rows hold stays, whatever its answer: a landing
                // holds a line's keys beside its node.
                for own in row.iter() {
                    proptest::prop_assert!(
                        !tabs[t].holds(own.node.name()),
                        "a record the rows hold stayed, answered {:?}",
                        own.answer
                    );
                }
            }
            Step::Seal => {
                read(&mut tabs[t], &row, &acted_row);
                if let Ok((owned, _, _)) = tabs[t].seal(
                    say(&format!("question {n}")),
                    at(1 + u32::from(seq)),
                    &ada,
                    &Fixed(AtomicU8::new(seq.wrapping_mul(7).wrapping_add(3))),
                ) {
                    sealed.extend(owned.iter().take(1).map(|own| own.node.clone()));
                    row.apply(&owned);
                }
            }
            Step::Again(edit) => {
                read(&mut tabs[t], &row, &acted_row);
                let Some(asked) = sealed
                    .iter()
                    .rev()
                    .find(|own| own.drawn_on() == own.name())
                    .map(|own| own.name().clone())
                else {
                    continue;
                };
                let how = if edit {
                    AskedAgain::Edit(suzatary_lines::secretary::held_replica::Question {
                        text: format!("edited {n}"),
                        references: Vec::new(),
                        attachments: Vec::new(),
                        seat: suzatary_signatures::signatures::Seat::Device,
                    })
                } else {
                    AskedAgain::Regenerate
                };
                if let Ok((owned, _, _)) = tabs[t].seal(
                    Sealing::Again {
                        conversation: DEFAULT.clone(),
                        question: asked.clone(),
                        how,
                        seen: vec![asked],
                    },
                    at(1 + u32::from(seq)),
                    &ada,
                    &Fixed(AtomicU8::new(seq.wrapping_mul(7).wrapping_add(3))),
                ) {
                    sealed.extend(owned.iter().take(1).map(|own| own.node.clone()));
                    row.apply(&owned);
                }
            }
            Step::Refuse(act) => {
                if let Some(line) = unsent_lines(&tabs[t]).first() {
                    // The box answers as its door does: a line it holds is taken already, and a
                    // taken line has no ruling to act on.
                    if held().contains(line.name()) {
                        let (moved, _) =
                            tabs[t].answered(vec![(line.name().clone(), Answer::Taken)]);
                        row.apply(&moved);
                        continue;
                    }
                    told.insert(line.print().to_owned());
                    let never = Answer::Refused {
                        kind: Refusal::Final,
                        reason: "never".to_owned(),
                    };
                    let (moved, _) = tabs[t].answered(vec![(line.name().clone(), never)]);
                    row.apply(&moved);
                    if act {
                        let names: BTreeSet<_> = [line.name().clone()].into();
                        acted_row.extend(names.iter().cloned());
                        let _ = tabs[t].acted(&names);
                    }
                }
            }
            Step::Post => {
                // A hand-over reads the row first; what it posts is what the tab holds unsent.
                read(&mut tabs[t], &row, &acted_row);
                for line in unsent_lines(&tabs[t]) {
                    proptest::prop_assert!(
                        !let_go.contains(line.name()),
                        "a record let go as refused is posted again"
                    );
                }
            }
            Step::Take(lost) => {
                read(&mut tabs[t], &row, &acted_row);
                let held = held();
                let takeable = unsent_lines(&tabs[t])
                    .into_iter()
                    .find(|line| line.rests_on().iter().all(|p| held.contains(p)));
                if let Some(line) = takeable {
                    // The door's answer: taken (and that answer lost, or not), or refused by the
                    // rule, as a regenerate of a question with no answer is.
                    let answers =
                        crate::asked_again::handed(&lines, std::slice::from_ref(&line), &ada);
                    let taken = answers.iter().all(|(_, answer)| *answer == Answer::Taken);
                    if !taken {
                        told.insert(line.print().to_owned());
                    }
                    if !lost || !taken {
                        let (moved, _) = tabs[t].answered(answers);
                        row.apply(&moved);
                    }
                }
            }
            Step::Land => {
                let served = skeleton(&lines);
                let keys: BTreeMap<_, _> = served
                    .keys
                    .iter()
                    .map(|(n, k)| {
                        (
                            suzatary_memory::secretary::locked_text::KeyName::new(n.clone())
                                .expect("a name"),
                            k.clone(),
                        )
                    })
                    .collect();
                tabs[t]
                    .piece(
                        served,
                        suzatary_memory::secretary::key_ring::KeyRing::of(&keys),
                        u64::from(seq) + 1,
                    )
                    .expect("a piece");
                let _ = tabs[t].land();
            }
            Step::Act => {
                use suzatary_lines::secretary::held_replica::Ruling;
                let names: BTreeSet<_> = names(tabs[t].rulings(&DEFAULT))
                    .into_iter()
                    .filter(|(_, r)| matches!(r, Ruling::Refused { .. }))
                    .map(|(name, _)| name)
                    .collect();
                acted_row.extend(names.iter().cloned());
                let _ = tabs[t].acted(&names);
            }
        }
    }
    // Every line sealed whose parents the box holds rests on no parent beneath another.
    let dag: prodrome_core::dag::Dag<LineEvent> =
        lines.lines.objects().expect("held").into_iter().collect();
    for line in &sealed {
        let parents = line.rests_on();
        if parents.iter().all(|p| dag.objects().contains_key(p)) {
            // Its deps: what it rests on, less the genesis every line names.
            let deps: Vec<_> = parents
                .iter()
                .filter(|p| !matches!(dag.get(p), Some(Envelope::Genesis(_))))
                .collect();
            for parent in &deps {
                let beneath = dag.closure(
                    deps.iter()
                        .filter(|other| *other != parent)
                        .map(|other| (*other).clone()),
                );
                proptest::prop_assert!(
                    !beneath.contains(*parent),
                    "{} rests on {parent}, which another of its parents rests on",
                    line.name()
                );
            }
        }
    }
    Ok(sealed)
}
