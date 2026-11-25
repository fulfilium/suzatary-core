//! THE CALL LOSES NO TURN, asked as the box asks it: every line of the log is in the call once,
//! under a record line, one `zoom` away, or a turn of the window, which ends in the batch waiting.

use std::collections::BTreeSet;

use proptest::prelude::*;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::{Earlier, ToolCall, Turn};
use suzatary_conversations::secretary::outcome::Outcome;
use suzatary_conversations::secretary::queue;
use suzatary_conversations::secretary::store::Entry;
use suzatary_digest::secretary::compaction::Reading;
use suzatary_digest::secretary::memories::Memories;
use suzatary_io_model::prompt::{Reads, prompt_of};
use suzatary_lines::nothing::Counting;
use suzatary_lines::secretary::run::Runs as _;
use suzatary_lines::secretary::transcript::{self, Retry};
use suzatary_secretary::conversation::ask::Ask;
use suzatary_secretary::secretary::live_shape::{self, Shape, Stores, Summaries};
use suzatary_stores::stores::InMemory;
use suzatary_topics::secretary::topics::stores::TopicStores;

#[derive(Debug, Clone)]
enum Step {
    /// The owner says something, with a photo or without.
    Owner {
        photo: bool,
    },
    /// The model answers, closing the round.
    Answer,
    /// The model calls a verb and its answer is stored, with a file or without.
    Verb {
        file: bool,
    },
    /// The model calls a verb whose answer an earlier round sends as its marker.
    Long,
    Stop,
    /// The owner rewinds to a turn, as a share of the conversation.
    Rewind(u8),
    /// Another conversation says something.
    Elsewhere,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        30 => any::<bool>().prop_map(|photo| Step::Owner { photo }),
        25 => Just(Step::Answer),
        10 => any::<bool>().prop_map(|file| Step::Verb { file }),
        2 => Just(Step::Long),
        3 => Just(Step::Stop),
        3 => any::<u8>().prop_map(Step::Rewind),
        15 => Just(Step::Elsewhere),
    ]
}

/// A generated store and the conversation asked in.
struct Scene {
    stores: Stores,
    a: ConversationId,
    b: ConversationId,
    second: std::cell::Cell<usize>,
    /// How many lines the unopened conversation holds, which no transcript reads.
    elsewhere: std::cell::Cell<usize>,
}

impl Scene {
    fn new() -> Self {
        let scene = Self {
            stores: Shape {
                lines: 0,
                conversations: 1,
                answer_bytes: 0,
                summaries: Summaries::Every,
            }
            .build(),
            a: ConversationId::new("a").expect("a slug"),
            b: ConversationId::new("b").expect("a slug"),
            second: std::cell::Cell::new(0),
            elsewhere: std::cell::Cell::new(0),
        };
        // Only the asked conversation opens: another's lines are noise around it.
        scene.stores.append(
            &scene.a,
            0,
            &Entry::Opened {
                at: live_shape::at(0),
                title: suzatary_memory::secretary::words::Words::kept("a".to_owned()),
                model: "m".to_owned(),
                kind: suzatary_conversations::secretary::store::Kind::Topic,
            },
        );
        scene
    }

    fn read(&self, id: &ConversationId) -> std::sync::Arc<transcript::Transcript> {
        transcript::read(self.stores.lines.conversations(), id).expect("the conversation reads")
    }

    fn write(&self, id: &ConversationId, entry: &Entry) {
        self.second.set(self.second.get() + 1);
        let next = if *id == self.b {
            self.elsewhere.replace(self.elsewhere.get() + 1)
        } else {
            self.read(id).lines().len()
        };
        self.stores.append(id, next, entry);
    }

    fn say(&self, id: &ConversationId, turn: Turn) {
        let by = matches!(turn, Turn::Person { .. }).then(|| "ada".into());
        let at = live_shape::at(self.second.get() + 1);
        self.write(id, &Entry::Said { at, turn, by });
    }

    fn step(&self, n: usize, step: &Step) {
        let (a, b) = (self.a.clone(), self.b.clone());
        match step {
            Step::Owner { photo } => {
                let attachments = photo
                    .then(|| {
                        suzatary_held::attachment::Attachment::accept(
                            format!("\u{89}PNG\r\n\u{1a}\n{n}").as_bytes(),
                        )
                        .ok()
                    })
                    .flatten()
                    .into_iter()
                    .collect();
                self.say(
                    &a,
                    Turn::Person {
                        text: format!("Owner says {n}"),
                        references: Vec::new(),
                        attachments,
                    },
                );
            }
            Step::Answer => self.say(&a, model(&format!("Answer {n}"), None)),
            Step::Verb { file } => {
                self.say(&a, model("", Some("show")));
                let files = file
                    .then(|| suzatary_held::enclosed::Enclosed {
                        id: format!("{n:064x}").parse().expect("an id"),
                        name: format!("notice {n}.pdf"),
                        media: suzatary_held::enclosed::Enclosure::Pdf,
                        size: 10,
                        text: String::new(),
                    })
                    .into_iter()
                    .collect();
                self.say(
                    &a,
                    Turn::Tool {
                        verb: "show".into(),
                        answer: format!("{{\"mail\": \"a mail {n}\"}}"),
                        outcome: Outcome::Answered,
                        files,
                    },
                );
            }
            Step::Long => {
                self.say(&a, model("", Some("show")));
                self.say(
                    &a,
                    Turn::Tool {
                        verb: "show".into(),
                        answer: format!(
                            "{{\"mail\": \"{}\"}}",
                            "a long mail ".repeat(queue::LONG_ANSWER / 6)
                        ),
                        outcome: Outcome::Answered,
                        files: Vec::new(),
                    },
                );
            }
            Step::Stop => {
                let at = live_shape::at(self.second.get() + 1);
                self.write(
                    &a,
                    &Entry::Stopped {
                        at,
                        by: "ada".to_owned(),
                    },
                );
            }
            Step::Rewind(share) => {
                let turns = self.read(&a).stored().turns.len();
                let at = live_shape::at(self.second.get() + 1);
                self.write(
                    &a,
                    &Entry::Rewound {
                        to: turns * usize::from(*share) / 255,
                        at,
                        by: "ada".to_owned(),
                    },
                );
            }
            Step::Elsewhere => self.say(
                &b,
                Turn::Person {
                    text: format!("Elsewhere {n}"),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
            ),
        }
    }
}

fn model(text: &str, verb: Option<&str>) -> Turn {
    Turn::Model {
        text: text.to_owned(),
        calls: verb
            .map(|verb| ToolCall {
                verb: verb.into(),
                args: std::collections::BTreeMap::new(),
            })
            .into_iter()
            .collect(),
    }
}

/// The span a record line names by its handle, `first+lines` at its head, or by the handle its
/// note of what is cut gives; `None` for any other line.
fn named_span(line: &str) -> Option<std::ops::Range<usize>> {
    let handle = match line.split_once("zoom(") {
        Some((_, rest)) if line.starts_with("(the log's first") => rest.split(')').next()?,
        _ => line.split(' ').next()?,
    };
    let (first, lines) = handle.split_once('+')?;
    let first: usize = first.parse().ok()?;
    Some(first..first + lines.parse::<usize>().ok()?)
}

/// The name of the log's line at each position.
fn log(reading: &Reading) -> Vec<prodrome_core::event::Hash> {
    let mut at = Vec::new();
    let mut stack: Vec<_> = reading
        .digest()
        .tree()
        .map(prodrome_core::digest::Tree::root)
        .into_iter()
        .collect();
    while let Some(node) = stack.pop() {
        if node.is_leaf() {
            at.push((
                node.span().start,
                reading.digest().line(node.name()).clone(),
            ));
        } else {
            stack.extend(node.children());
        }
    }
    at.sort();
    at.into_iter().map(|(_, name)| name).collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn every_turn_is_sent_summarised_or_one_zoom_away(
        script in proptest::collection::vec(step(), 0..60),
        asked in 1usize..3,
        lag in 0usize..40,
    ) {
        let scene = Scene::new();
        for (n, step) in script.iter().enumerate() {
            scene.step(n, step);
        }
        for k in 0..asked {
            scene.step(1_000 + k, &Step::Owner { photo: k == 0 });
        }
        let held = scene.stores.lines.runs.begin().expect("a run");
        let claimed = transcript::claim(
            scene.stores.lines.conversations(),
            &scene.a,
            &held.run,
            live_shape::at(scene.second.get() + 1),
            Retry::Allowed,
            &|_| true,
            |_| Vec::new(),
        )
        .expect("claims");
        prop_assert_eq!(claimed, transcript::Claimed::Began);
        scene.stores.summarise(lag, scene.second.get() + 1);

        let (cards, memberships, aliases) = (InMemory::default(), InMemory::default(), InMemory::default());
        let entropy = Counting::default();
        let memories = Memories {
            history: &scene.stores.facts,
            texts: &scene.stores.words,
            cards: &cards,
            entropy: &entropy,
            digests: scene.stores.digests(),
        };
        let topics = TopicStores { memberships: &memberships, aliases: &aliases };
        let ask = Ask::Digest { model: &suzatary_base::capability::Nothing, memories: &memories, topics: &topics };
        let read = scene.read(&scene.a);
        let call = ask.call("s", &read);

        // The window is the one function's, and ends in the batch waiting, as itself.
        let window = queue::window(read.stored(), &read.queued());
        let turns = |part: &[(usize, &suzatary_conversations::secretary::store::Said)]| -> Vec<Turn> {
            part.iter().map(|(_, said)| said.turn.clone()).collect()
        };
        prop_assert_eq!(call.earlier.len(), window.earlier.len());
        for (sent, (index, said)) in call.earlier.iter().zip(&window.earlier) {
            match sent {
                Earlier::Long { verb, .. } => {
                    prop_assert!(queue::long_answer(read.stored(), *index), "only a long answer is a marker");
                    let answered = matches!(
                        suzatary_conversations::secretary::turn_shape::shape(&said.turn),
                        suzatary_conversations::secretary::turn_shape::TurnShape::Tool { verb: said, .. } if said == *verb
                    );
                    prop_assert!(answered, "the marker names its verb");
                }
                Earlier::Said(turn) | Earlier::Answered { turn, .. } => {
                    prop_assert!(!queue::long_answer(read.stored(), *index));
                    prop_assert_eq!(turn, &said.turn);
                }
            }
        }
        prop_assert_eq!(&call.turns, &turns(&window.round));
        let waiting = turns(&queue::delta(read.stored()));
        prop_assert!(call.turns.ends_with(&waiting), "the batch waiting is sent as itself");

        // A photo of an earlier round is its marker; one of the round is sent beside it.
        let printed = prompt_of(&call, &serde_json::json!([]), Reads::Files).transcript();
        for turn in call.earlier.iter().filter_map(Earlier::turn) {
            if let Turn::Person { attachments, .. } = turn {
                for photo in attachments {
                    let marker = format!("photo {} (image/png) was sent with an earlier round", photo.id);
                    prop_assert!(printed.contains(&marker), "{}", printed);
                }
            }
        }

        // Every line is under one record line, or a line of the window from its first on.
        let reading = Reading::of(&scene.stores.digests().reads()).expect("reads whole");
        let told = call.context.cached.iter().chain(&call.context.rest).cloned().collect::<Vec<_>>().join("\n");
        let log = log(&reading);
        let mut covered = vec![0u8; log.len()];
        for span in told.lines().filter_map(named_span) {
            for at in span {
                covered[at] += 1;
            }
        }
        prop_assert!(covered.iter().all(|n| *n <= 1), "a line named twice: {}", told);
        let lines_of_a: Vec<&prodrome_core::event::Hash> = read.lines().iter().map(|written| &written.name).collect();
        let from = window
            .first()
            .and_then(|first| read.line_of(first))
            .unwrap_or(lines_of_a.len());
        let sent: BTreeSet<&prodrome_core::event::Hash> = lines_of_a[from..].iter().copied().collect();
        for span in told.lines().filter_map(named_span) {
            prop_assert!(
                !span.clone().all(|at| sent.contains(&log[at])),
                "lines {:?}, all sent as turns, stand in the record too: {}",
                span,
                told
            );
        }
        for (at, name) in log.iter().enumerate() {
            prop_assert!(
                covered[at] == 1 || sent.contains(name),
                "line {} is neither in the record nor in the window: {}",
                at,
                told
            );
        }
    }
}

/// The owner's tap on a summary.
fn tap() -> suzatary_signatures::signatures::Tap {
    use suzatary_signatures::signatures::ActHeaders;
    suzatary_signatures::signatures::Tap::of(
        &suzatary_stores::stores::scene("ada"),
        &ActHeaders {
            act: Some("1"),
            content_type: Some("application/json"),
            site: Some("same-origin"),
            client: Some(suzatary_signatures::signatures::CLIENT),
        },
        suzatary_signatures::signatures::Seat::Device,
    )
    .expect("the owner's tap")
}

/// A summary the owner retracted is shown in no later call: one round is told summaries, the
/// owner retracts every summary standing after it, the compactor catches up, and the next
/// round's call holds summaries again and none of the retracted ones.
#[test]
fn a_retracted_summary_is_in_no_later_call() {
    let scene = Scene::new();
    // Longer, all told, than the record's room, so the view shows summaries.
    for n in 0..400 {
        scene.say(
            &scene.b,
            Turn::Person {
                text: format!("Elsewhere {n} {}", "word ".repeat(100)),
                references: Vec::new(),
                attachments: Vec::new(),
            },
        );
    }
    let (cards, memberships, aliases) = (
        InMemory::default(),
        InMemory::default(),
        InMemory::default(),
    );
    let entropy = Counting::default();
    let memories = Memories {
        history: &scene.stores.facts,
        texts: &scene.stores.words,
        cards: &cards,
        entropy: &entropy,
        digests: scene.stores.digests(),
    };
    let topics = TopicStores {
        memberships: &memberships,
        aliases: &aliases,
    };
    let ask = Ask::Digest {
        model: &suzatary_base::capability::Nothing,
        memories: &memories,
        topics: &topics,
    };
    let told = |scene: &Scene| {
        let context = ask.call("s", &scene.read(&scene.a)).context;
        context
            .cached
            .into_iter()
            .chain(context.rest)
            .collect::<Vec<_>>()
            .join("\n")
    };
    scene.step(100, &Step::Owner { photo: false });
    scene.stores.summarise_saying(usize::MAX, 500, "FIRSTPASS");
    assert!(told(&scene).contains("FIRSTPASS"), "a summary is told");
    scene.step(101, &Step::Answer);

    let read = Reading::of(&scene.stores.digests().reads()).expect("reads whole");
    let mut stack: Vec<_> = read
        .digest()
        .tree()
        .map(prodrome_core::digest::Tree::root)
        .into_iter()
        .collect();
    let held = memories.hold().expect("held");
    while let Some(node) = stack.pop() {
        stack.extend(node.children());
        if read.digest().summary(node.name()).is_some() {
            held.retract(node.name(), live_shape::at(600), &tap())
                .expect("retracted");
        }
    }
    drop(held);
    scene.step(102, &Step::Owner { photo: false });
    scene.stores.summarise_saying(usize::MAX, 700, "SECONDPASS");
    let after = told(&scene);
    assert!(after.contains("SECONDPASS"), "summaries are told again");
    assert!(
        !after.contains("FIRSTPASS"),
        "no retracted summary is told: {after}"
    );
}

/// What a conversation's fold says of its window and of every turn: the window's turns, and each
/// turn's carry and weight.
fn reading_of(scene: &Scene) -> (BTreeSet<usize>, Vec<(bool, usize)>) {
    let read = scene.read(&scene.a);
    let stored = read.stored();
    (
        queue::window(stored, &BTreeSet::new())
            .all()
            .map(|(at, _)| *at)
            .collect(),
        stored
            .carried
            .iter()
            .map(|carried| {
                (
                    carried.taint == suzatary_conversations::secretary::taint::Taint::Tainted,
                    carried.bytes,
                )
            })
            .collect(),
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    /// A TURN WEIGHS WHAT ITS LINE WAS WRITTEN AS: erasing a turn's words, or losing the key of
    /// another's, moves no window and changes no turn's carry or weight.
    #[test]
    fn an_erasure_or_a_lost_key_moves_no_window_and_no_label(
        script in proptest::collection::vec(step(), 1..60),
        erased_share in 0u8..=255,
        lost_share in 0u8..=255,
    ) {
        let scene = Scene::new();
        for (n, step) in script.iter().enumerate() {
            scene.step(n, step);
        }
        let before = reading_of(&scene);
        let read = scene.read(&scene.a);
        let turns = read.stored().turns.len();
        prop_assume!(turns > 0);
        let keys_of = |turn: usize| -> BTreeSet<suzatary_memory::secretary::locked_text::KeyName> {
            read.line_of(turn)
                .map(|line| read.lines()[line].line.keys().cloned().collect())
                .unwrap_or_default()
        };
        let erased = keys_of((turns - 1) * usize::from(erased_share) / 255);
        let lost = keys_of((turns - 1) * usize::from(lost_share) / 255);
        drop(read);
        if !erased.is_empty() {
            transcript::erase(
                scene.stores.lines.conversations(),
                &scene.a,
                erased,
                suzatary_memory::secretary::erasure::Erasure::Asked {
                    by: transcript::writer(),
                },
                live_shape::at(scene.second.get() + 1),
            )
            .expect("erased");
        }
        for key in &lost {
            suzatary_store::store::Doc::remove(&scene.stores.lines.keys, key).expect("lost");
        }
        prop_assert_eq!(reading_of(&scene), before);
    }
}

/// A turn that lands between a round's claim and its call is not sent: no round has claimed it,
/// and the round that does answers it.
#[test]
fn a_turn_landing_after_the_claim_is_not_sent() {
    let scene = Scene::new();
    scene.step(0, &Step::Owner { photo: false });
    let held = scene.stores.lines.runs.begin().expect("a run");
    let claimed = transcript::claim(
        scene.stores.lines.conversations(),
        &scene.a,
        &held.run,
        live_shape::at(scene.second.get() + 1),
        Retry::Allowed,
        &|_| true,
        |_| Vec::new(),
    )
    .expect("claims");
    assert_eq!(claimed, transcript::Claimed::Began);
    scene.say(
        &scene.a,
        Turn::Person {
            text: "landed late".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
    );
    let (cards, memberships, aliases) = (
        InMemory::default(),
        InMemory::default(),
        InMemory::default(),
    );
    let entropy = Counting::default();
    let memories = Memories {
        history: &scene.stores.facts,
        texts: &scene.stores.words,
        cards: &cards,
        entropy: &entropy,
        digests: scene.stores.digests(),
    };
    let topics = TopicStores {
        memberships: &memberships,
        aliases: &aliases,
    };
    let ask = Ask::Digest {
        model: &suzatary_base::capability::Nothing,
        memories: &memories,
        topics: &topics,
    };
    let read = scene.read(&scene.a);
    assert_eq!(read.queued().len(), 1, "the late turn is owed");
    let call = ask.call("s", &read);
    assert!(
        !call
            .turns
            .iter()
            .any(|turn| matches!(turn, Turn::Person { text, .. } if text == "landed late")),
        "{:?}",
        call.turns
    );
    assert!(
        matches!(call.turns.as_slice(), [Turn::Person { text, .. }] if text == "Owner says 0"),
        "{:?}",
        call.turns
    );
}
