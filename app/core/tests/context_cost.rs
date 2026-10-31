//! THE AGENT'S `context` AT THE LIVE SHAPE. One call reads the facts, builds the record from every
//! line of the conversations and the digest's history (`Recollection::record`), and fits the
//! memory into its answer (`agent::context`), which reads the digest's view about twenty times
//! while it searches for the largest view that fits. A view asks for every node of the tree how
//! its summary stands, and that answer finds the node by name, so a lookup that searched the
//! whole tree made the call cost the square of the store. Two laws bound the call's CPU time:
//! it grows with the store and not with its square, and at the live shape an optimised build
//! answers in under two seconds of one core.

use std::collections::BTreeSet;
use std::time::Duration;

use prodrome_core::event::Actor;
use prodrome_core::literal::Datetime;
use suzatary_core::capability::{Entropy as _, Manual, Manuals, Turn};
use suzatary_core::chains::{ChainName, ChainPolicy, Chains};
use suzatary_core::deployment::Deployment;
use suzatary_core::model_name::ModelName;
use suzatary_core::secretary::compact_ask::Beside;
use suzatary_core::secretary::conversations::{Counting, InMemory as Lines};
use suzatary_core::secretary::digest_event::{self, Attempt, DigestEvent};
use suzatary_core::secretary::digest_line::Line;
use suzatary_core::secretary::digests::{Digests, compactor};
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::memories::ReadMemories;
use suzatary_core::secretary::memory::Fact;
use suzatary_core::secretary::memory_event::MemoryEvent;
use suzatary_core::secretary::recollection::Recollection;
use suzatary_core::secretary::salted_text::{Text, TextName};
use suzatary_core::secretary::store::{ConversationId, Entry, Kind};
use suzatary_core::secretary::words::Words;
use suzatary_core::store::{Doc as _, InMemory, InMemoryDoc, Prodrome as _};

/// The conversations, the digest and the facts, in memory.
#[derive(Default)]
struct Stores {
    lines: Lines,
    owners: BTreeSet<Actor>,
    digests: InMemory<DigestEvent>,
    summaries: InMemoryDoc<TextName, Text<Line>>,
    facts: InMemory<MemoryEvent>,
    words: InMemoryDoc<TextName, Text<Fact>>,
}

impl Stores {
    fn digests(&self) -> Digests<'_> {
        Digests {
            history: &self.digests,
            texts: &self.summaries,
            lines: self.lines.conversations(),
            owners: &self.owners,
            compacting: &suzatary_core::secretary::compacting::Compacting::Off,
        }
    }

    /// The memory as the agent's record holds it, with the default `memory_bytes`.
    fn recollection(&self) -> Recollection<'_> {
        Recollection::of(
            ReadMemories {
                history: &self.facts,
                texts: &self.words,
                digests: self.digests().reads(),
            },
            16 * 1024,
        )
    }
}

fn ada() -> Actor {
    Actor::new("ada").expect("an actor")
}

/// The `second`th second of the store's year.
fn at(second: usize) -> Datetime {
    let start =
        prodrome_core::fpl::instant_of(Datetime::new(2026, 1, 1, 0, 0, 0, 0).expect("an instant"));
    let offset = chrono::Duration::seconds(i64::try_from(second).expect("a year of seconds"));
    prodrome_core::fpl::datetime_of(start + offset).expect("an instant")
}

/// Line `line` of a conversation: its opening, then the owner's question and, in a conversation
/// the secretary answers, its answer of about 3 kB, alternating.
fn entry(line: usize, second: usize, answered: bool) -> Entry {
    let person = |text: String| Entry::Said {
        at: at(second),
        turn: Turn::Person {
            text,
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some("ada".into()),
    };
    match (line, answered && line.is_multiple_of(2)) {
        (0, _) => Entry::Opened {
            at: at(second),
            title: Words::kept(format!("Conversation at second {second}")),
            model: "a-cli/small".to_owned(),
            kind: Kind::Topic,
        },
        (_, false) => person(format!("Question {line} about the permit, asked again.")),
        (_, true) => Entry::Said {
            at: at(second),
            turn: Turn::Model {
                text: format!("Answer {line} about the permit. ").repeat(100),
                calls: Vec::new(),
            },
            by: None,
        },
    }
}

/// `lines` lines over `conversations` conversations, one in four of the owner's notes alone, so
/// the record shows summaries; then every node of the digest's tree summarised, level by level.
fn store(lines: usize, conversations: usize) -> Stores {
    let stores = Stores {
        owners: [ada()].into_iter().collect(),
        ..Stores::default()
    };
    let mut second = 0;
    for conversation in 0..conversations {
        let id = ConversationId::new(format!("c-{conversation}")).expect("a slug");
        for held in 0..lines / conversations {
            second += 1;
            let (line, keys) = LineEvent::write(
                id.clone(),
                ada(),
                u32::try_from(held).expect("few lines"),
                &entry(held, second, conversation % 4 != 0),
                &stores.lines.entropy,
            )
            .expect("a line");
            for key in keys {
                stores.lines.keys.set(&key.name(), &key).expect("kept");
            }
            stores.lines.lines.append(line).expect("said");
        }
    }
    let salts = Counting::default();
    loop {
        let read = stores.digests().reads().read().expect("the digest reads");
        let pending = read.pending();
        if pending.is_empty() {
            return stores;
        }
        for node in pending {
            let label = *read.node(&node).expect("in the tree").measure();
            let text = Text::new(
                Line::new(&format!("The owner asked about the permit, node {node}."))
                    .expect("a line"),
                salts.salt().expect("a salt"),
            );
            stores.summaries.set(&text.name(), &text).expect("kept");
            let event = digest_event::summarized(
                node.clone(),
                at(second + 1),
                compactor(),
                &text,
                Attempt {
                    over: read.over(&node).expect("its children read"),
                    model: ModelName::parse("a-model").expect("a model"),
                    taint: label.taint,
                    beside: Beside::of(&label),
                },
            )
            .expect("a summary");
            stores.digests.append(event).expect("appended");
        }
    }
}

struct Library;

impl Manuals for Library {
    fn document(&self, _: Manual) -> Option<String> {
        Some("The operator's manual.".to_owned())
    }
}

fn deployment() -> Deployment {
    Deployment {
        viewers: BTreeSet::new(),
        actors: std::collections::BTreeMap::new(),
        chains: Chains::of(vec![ChainPolicy {
            name: ChainName::new("life").expect("a slug"),
            untrusted: prodrome_core::policy::Untrusted::of([]),
            roster: BTreeSet::new(),
            root: suzatary_core::cast::keys::root(),
        }])
        .expect("one chain"),
        categories: std::collections::BTreeMap::new(),
        app_url: "https://example.invalid".to_owned(),
        coding: None,
        zone: suzatary_core::zone::Zone::UTC,
    }
}

/// The CPU time this thread has run, which the kernel counts in nanoseconds: what other
/// processes and threads do on the machine does not add to it, as it adds to wall time.
fn cpu() -> Duration {
    let read = std::fs::read_to_string("/proc/thread-self/schedstat")
        .expect("Linux counts a thread's CPU time in /proc/thread-self/schedstat");
    let nanoseconds = read
        .split_whitespace()
        .next()
        .and_then(|first| first.parse().ok())
        .expect("its first field is the nanoseconds the thread ran");
    Duration::from_nanos(nanoseconds)
}

/// The least of three CPU times of one `context` call over `stores`, as the verb makes it.
fn context(stores: &Stores) -> Duration {
    (0..3)
        .map(|_| {
            let started = cpu();
            let recollection = stores.recollection();
            let answer = suzatary_core::agent::context(
                &Library,
                &deployment(),
                recollection.facts().as_ref(),
                recollection.record().as_ref(),
                recollection.budget(),
            );
            let took = cpu().saturating_sub(started);
            assert!(
                answer["memory"]
                    .as_str()
                    .is_some_and(|memory| memory.contains("derived:")),
                "{answer}"
            );
            took
        })
        .min()
        .expect("three timings")
}

/// Four times the lines cost `context` less than eight times the CPU: about four and a half
/// times after the index, and fourteen times when each lookup searched the tree. The ratio of
/// two CPU times holds on any machine and under any load the gate puts beside it.
#[test]
fn the_context_costs_what_the_store_holds_and_not_its_square() {
    let small = context(&store(1_000, 8));
    let large = context(&store(4_000, 32));
    assert!(
        large < small * 8,
        "1,000 lines took {small:?} of CPU and 4,000 took {large:?}"
    );
}

/// The live shape, about 13,000 lines over 100 conversations under a summarised tree, costs
/// under two seconds of CPU in an optimised build.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "a bound on the optimised build: cargo test --release -p suzatary-core --test all context_cost"
)]
fn the_context_at_the_live_shape_answers_in_two_seconds() {
    let took = context(&store(13_000, 100));
    assert!(took < Duration::from_secs(2), "{took:?} of CPU");
}
