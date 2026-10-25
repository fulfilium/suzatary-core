//! The conversations' laws, over conversations generated as the box writes them: the
//! transcript reads as `Stored::of` over the same lines through the redaction door, the owed
//! register is `queue::delta`, the standing register is the open proposals, none of them depends
//! on the order the objects are handed over in, every line prints and parses back, the Prodrome's
//! own store reads as the one in memory, and two replicas that wrote without seeing each other
//! read the same once each holds the other's objects.

use std::collections::BTreeMap;
use std::path::PathBuf;

use proptest::prelude::*;

use prodrome_core::event::{Actor, Envelope, Hash, canonical, canonical_envelope, parse_event};
use prodrome_core::literal::Datetime;
use prodrome_core::policy::Everything;
use prodrome_core::store::EventStore;

use suzatary_core::capability::{SessionId, ToolCall, Turn};
use suzatary_core::cast::CAST;
use suzatary_core::secretary::conversations::Counting;
use suzatary_core::secretary::inbox::{self, ProposalId};
use suzatary_core::secretary::inbox_event::{InboxEvent, Said};
use suzatary_core::secretary::line_event::{LineEvent, LineVocabulary};
use suzatary_core::secretary::line_shape;
use suzatary_core::secretary::locked_text::KeyName;
use suzatary_core::secretary::queue;
use suzatary_core::secretary::responded::{Done, Responded, Tried};
use suzatary_core::secretary::store::{
    ConversationId, Decision, Draft, Entry, Line, Namer, Proposal, Reasons, Rewind, Stored, Title,
    Unheld, stamp,
};
use suzatary_core::secretary::transcript::{self, Transcript};
use suzatary_core::secretary::words::Words;
use suzatary_core::store::Reads as _;
use suzatary_core::store::{InMemory, Prodrome, linearised};

type Memory = suzatary_core::secretary::conversations::InMemory;
type Disk = EventStore<LineEvent, Everything>;

/// Every key the lines written straight to a store were locked under, and where they came from.
static KEYS: std::sync::LazyLock<suzatary_core::store::InMemoryKeys> =
    std::sync::LazyLock::new(Default::default);
static ENTROPY: std::sync::LazyLock<Counting> = std::sync::LazyLock::new(Default::default);

/// A key `KEYS` holds, or missing.
fn key(name: &KeyName) -> Words<suzatary_core::secretary::locked_text::TextKey> {
    use suzatary_core::store::Doc as _;
    KEYS.get(name).ok().flatten().map_or_else(
        || suzatary_core::secretary::words::Gone::Missing.into(),
        Words::kept,
    )
}

fn by() -> Actor {
    Actor::new("assistant").expect("an actor")
}

fn id() -> ConversationId {
    ConversationId::new("2026-09-30-1").expect("a slug")
}

fn at(n: u32) -> Datetime {
    Datetime::new(2026, 9, 30, 9, n / 60, n % 60, 0).expect("an instant")
}

/// A scratch directory of this process's own, gone when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "suzatary-transcript-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Line `entry` of conversation `id` as the store keeps it: each slot of its words through the
/// redaction door, as `LineEvent::write` sends them.
fn scrubbed(id: &ConversationId, entry: &Entry) -> Entry {
    let (shape, words) = line_shape::split(entry);
    let words = words
        .expect("a line with its words")
        .iter()
        .map(|words| Words::kept(words.scrubbed().expect("scrubs")))
        .collect();
    line_shape::join(id, shape, entry.at(), words)
}

/// How many bytes the words of `entry` print to, which is most of what a line weighs.
fn weight(entry: &Entry) -> usize {
    line_shape::split(entry)
        .1
        .expect("a line with its words")
        .iter()
        .map(|words| suzatary_core::secretary::salted_text::Words::print(words).len())
        .sum()
}

fn opened() -> Entry {
    Entry::Opened {
        at: at(0),
        title: suzatary_core::secretary::words::Words::kept("Morning".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: suzatary_core::secretary::store::Kind::Topic,
    }
}

fn said(n: u32, turn: Turn) -> Entry {
    Entry::Said {
        at: at(n),
        turn,
        by: None,
    }
}

/// Line `held` of the conversation, as the box writes it, its keys kept in `KEYS`.
fn event(held: usize, entry: &Entry) -> LineEvent {
    line_of(&id(), held, entry)
}

fn line_of(id: &ConversationId, held: usize, entry: &Entry) -> LineEvent {
    use suzatary_core::store::Doc as _;
    let (line, keys) = LineEvent::write(
        id.clone(),
        by(),
        u32::try_from(held).expect("a count"),
        entry,
        &*ENTROPY,
    )
    .expect("a line");
    for opens in keys {
        KEYS.set(&opens.name(), &opens).expect("kept");
    }
    line
}

/// A store on disk, begun as the box begins one.
fn disk(scratch: &Scratch) -> Disk {
    let store = Disk::new(&scratch.0, Everything);
    store.init("lines").expect("begun");
    store
}

/// One thing the box writes into a conversation; the builder turns each into a line, or skips it
/// where the box would refuse it.
#[derive(Debug, Clone)]
enum Step {
    Person(u8),
    Answer,
    Call,
    Tool { refused: bool, link: bool },
    Act,
    Propose { revise: bool },
    Stop { stored_shape: bool },
    Rewind(bool),
    Session,
    Unheard,
    MemorySeen,
    Title,
    Review,
    ReviewFailed,
    Coding,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (0u8..4).prop_map(Step::Person),
        3 => Just(Step::Answer),
        2 => Just(Step::Call),
        2 => (any::<bool>(), any::<bool>()).prop_map(|(refused, link)| Step::Tool { refused, link }),
        1 => Just(Step::Act),
        1 => any::<bool>().prop_map(|revise| Step::Propose { revise }),
        1 => any::<bool>().prop_map(|stored_shape| Step::Stop { stored_shape }),
        3 => any::<bool>().prop_map(Step::Rewind),
        1 => Just(Step::Session),
        1 => Just(Step::Unheard),
        1 => Just(Step::MemorySeen),
        1 => Just(Step::Title),
        1 => Just(Step::Review),
        1 => Just(Step::ReviewFailed),
        1 => Just(Step::Coding),
    ]
}

/// A conversation's steps, each with the second its line is stamped at: stamps wander, since
/// order is causal and time is data.
fn steps() -> impl Strategy<Value = Vec<(Step, u32)>> {
    proptest::collection::vec((step(), 0u32..3600), 0..60)
}

fn object(n: usize) -> ProposalId {
    ProposalId::new(format!("p{n:032x}")).expect("an id")
}

fn person(n: u8) -> Turn {
    Turn::Person {
        text: ["what changed", "and the flight?", "ok", "ok"][usize::from(n)].to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
    }
}

/// The lines a script comes to, as the box hands them to the store.
fn conversation(script: &[(Step, u32)]) -> Vec<Entry> {
    let mut lines = vec![opened()];
    let mut proposals = 0usize;
    let mut sessions = 0u32;
    let now =
        |lines: &[Entry]| Stored::of(&id(), lines).expect("the builder writes a conversation");
    for (step, second) in script {
        let at = at(*second);
        let line = match step {
            Step::Person(n) => said(*second, person(*n)),
            Step::Answer => said(
                *second,
                Turn::Model {
                    text: "done".to_owned(),
                    calls: Vec::new(),
                },
            ),
            Step::Call => said(
                *second,
                Turn::Model {
                    text: String::new(),
                    calls: vec![ToolCall {
                        verb: "todo_show".into(),
                        args: [("id".to_owned(), serde_json::json!("permit"))].into(),
                    }],
                },
            ),
            Step::Tool { refused, link } => said(
                *second,
                Turn::Tool {
                    verb: if *refused { "remember" } else { "todo_show" }.into(),
                    answer: if *link {
                        "sign in at https://example.test/login?token=signin-tok&next=/".to_owned()
                    } else {
                        "the permit is due Friday".to_owned()
                    },
                    outcome: if *refused {
                        suzatary_core::secretary::outcome::Outcome::Refused
                    } else {
                        suzatary_core::secretary::outcome::Outcome::Answered
                    },
                    files: Vec::new(),
                },
            ),
            Step::Act => said(
                *second,
                Turn::Act {
                    text: format!("{} dismissed a proposal", CAST.owner.name),
                    references: Vec::new(),
                },
            ),
            Step::Propose { revise } => {
                proposals += 1;
                Entry::Proposed {
                    at,
                    object: object(proposals - 1),
                    revises: (*revise && proposals > 1).then(|| proposals - 2),
                }
            }
            Step::Stop { stored_shape } => stopped(&mut lines, *second, *stored_shape),
            Step::Rewind(edit) => {
                let how = if *edit {
                    Rewind::Edit
                } else {
                    Rewind::Regenerate
                };
                let Ok(to) = now(&lines).rewind(how) else {
                    continue;
                };
                Entry::Rewound {
                    to,
                    at,
                    by: CAST.owner.actor.to_owned(),
                }
            }
            Step::Session | Step::MemorySeen | Step::Title | Step::Review | Step::ReviewFailed => {
                bookkeeping(step, *second, &mut sessions)
            }
            Step::Unheard => match unheard(&now(&lines), at) {
                Some(line) => line,
                None => continue,
            },
            Step::Coding => {
                if proposals == 0 {
                    continue;
                }
                Entry::Coding {
                    at,
                    proposal: proposals - 1,
                    session: suzatary_core::coding::Session::Fired {
                        id: "session_1".parse().expect("a session id"),
                        url: "https://claude.ai/code/session_1".parse().expect("a url"),
                        at: stamp(at),
                    },
                }
            }
        };
        lines.push(line);
    }
    lines
}

/// The line ending the session `stored` keeps, at `at`; the box writes it for no other.
fn unheard(stored: &Stored, at: Datetime) -> Option<Entry> {
    stored.session.as_ref().map(|kept| Entry::Unheard {
        at,
        session: kept.session.id.clone(),
        model: "openai-compatible/deepseek-v4-flash".to_owned(),
    })
}

/// A stop's line, or, as stored before a stop closed its question by itself, the stop pushed
/// and then an empty end.
fn stopped(lines: &mut Vec<Entry>, second: u32, stored_shape: bool) -> Entry {
    let stop = Entry::Stopped {
        at: at(second),
        by: CAST.owner.actor.to_owned(),
    };
    if stored_shape {
        lines.push(stop);
        said(
            second,
            Turn::Model {
                text: String::new(),
                calls: Vec::new(),
            },
        )
    } else {
        stop
    }
}

/// The lines the box writes about a conversation rather than in it, which read no other line.
fn bookkeeping(step: &Step, second: u32, sessions: &mut u32) -> Entry {
    let at = at(second);
    match step {
        Step::Session => {
            *sessions += 1;
            let session =
                SessionId::new(format!("96f709a3-43d7-4453-bf59-{sessions:012}")).expect("a uuid");
            // The box writes `Kept`, and `Scoped` when an ask's scope moves; a `Session` line from
            // before `Kept` is still read.
            if sessions.is_multiple_of(3) {
                Entry::Scoped {
                    at,
                    scope: if sessions.is_multiple_of(2) {
                        suzatary_core::secretary::store::Scope::Window
                    } else {
                        suzatary_core::secretary::store::Scope::Transcript
                    },
                }
            } else if sessions.is_multiple_of(2) {
                Entry::Kept {
                    at,
                    session,
                    model: "a-cli/small".to_owned(),
                    cwd: "/srv/secretary".to_owned(),
                    surface: None,
                    from: *sessions as usize,
                }
            } else {
                Entry::Session {
                    at,
                    session,
                    model: "a-cli/small".to_owned(),
                    cwd: "/srv/secretary".to_owned(),
                    surface: None,
                }
            }
        }
        Step::MemorySeen => Entry::MemorySeen {
            at,
            version: u64::from(second),
        },
        Step::Title => Entry::Titled {
            at,
            title: suzatary_core::secretary::words::Words::kept(
                Title::new(format!("Topic {second}")).expect("a title"),
            ),
            by: Namer::Person {
                actor: CAST.owner.actor.to_owned(),
            },
        },
        Step::Review => Entry::Reviewed {
            at,
            chain: "life".to_owned(),
            batch: format!("{second:064x}"),
            decision: Decision::Held,
            why: suzatary_core::secretary::words::Words::kept("about money".to_owned()),
        },
        Step::ReviewFailed => Entry::ReviewFailed {
            at,
            reason: suzatary_core::secretary::words::Words::kept(
                "the model was unreachable".to_owned(),
            ),
            standing: 1,
        },
        _ => unreachable!("a step that reads the conversation"),
    }
}

/// The inbox holding every proposal a conversation made, each answered as `answers` says, but
/// with no asking for a proposal `dropped` marks: its record not held, or answered and never
/// asked where it has an answer.
fn inbox_of(proposals: usize, answers: &[u8], dropped: &[bool]) -> inbox::Fold {
    let owner = Actor::new(CAST.owner.actor).expect("an actor");
    let at = Datetime::new(2026, 9, 30, 9, 0, 0, 0).expect("an instant");
    let events = (0..proposals).flat_map(|n| {
        let asked = inbox::Asked {
            proposer: suzatary_core::secretary::proposer::Proposer::Secretary,
            proposal: Proposal::Email(Draft {
                account: CAST.owner.email.to_owned(),
                to: vec![CAST.counsel.email.to_owned()],
                cc: Vec::new(),
                subject: format!("proposal {n}"),
                body: "Sending the documents today.".to_owned(),
                attachments: Vec::new(),
            }),
            reasons: Reasons::new(
                "he asked",
                vec!["mail:abcdef012345".parse().expect("a ref")],
            )
            .expect("reasons"),
            asked: format!("proposal:{}/{n}", id()).parse().expect("a ref"),
            price: inbox::Price::unpriced(),
        };
        let record = InboxEvent {
            proposal: object(n),
            at,
            actor: Actor::new("assistant").expect("an actor"),
            said: Said::Asked(Box::new(asked)),
        };
        let response = match answers.get(n).copied().unwrap_or(0) % 5 {
            0 => None,
            1 => Some(Responded::Done(Done::Sent {
                recipients: vec![CAST.counsel.email.to_owned()],
                message_id: format!("<{n}@box>"),
            })),
            2 => Some(Responded::Dismissed {
                reason: "not now".to_owned(),
            }),
            3 => Some(Responded::Tried {
                attempt: 1,
                tried: Tried::NotSent {
                    reason: "refused".to_owned(),
                },
            }),
            _ => Some(Responded::Done(Done::MayHaveGone {
                reason: "no answer".to_owned(),
            })),
        };
        let asking = (!dropped.get(n).copied().unwrap_or(false)).then_some(record);
        asking
            .into_iter()
            .chain(response.map(|response| InboxEvent {
                proposal: object(n),
                at,
                actor: owner.clone(),
                said: Said::Responded(response),
            }))
    });
    inbox::Fold::of_chain(&events.collect::<Vec<_>>())
}

/// The lines through the door: what the store holds and what `Stored::of` is asked about.
fn through_the_door(lines: &[Entry]) -> Vec<Entry> {
    lines.iter().map(|line| scrubbed(&id(), line)).collect()
}

/// The lines written into a fresh store, one push each, and read back.
fn written(lines: &[Entry]) -> (Memory, Transcript) {
    let memory = Memory::default();
    transcript::start(memory.conversations(), &id(), &lines[0]).expect("started");
    for line in &lines[1..] {
        transcript::push(memory.conversations(), &id(), line).expect("pushed");
    }
    let read = transcript::read(memory.conversations(), &id()).expect("read");
    (memory, read)
}

/// Every line of `read` as the next line of a store on disk: each one's deps the frontier.
fn on_disk(store: &Disk, lines: &[Entry], from: usize) -> Vec<Hash> {
    lines
        .iter()
        .enumerate()
        .map(|(n, line)| store.append(event(from + n, line)).expect("appended"))
        .collect()
}

fn objects_of(store: &Disk) -> Vec<(Hash, Envelope<LineEvent>)> {
    linearised(store).expect("linearised")
}

/// Hand every object `from` holds to `to`, as a replica adopts.
fn adopt(to: &Disk, from: &Disk) {
    let prints: BTreeMap<Hash, String> = objects_of(from)
        .iter()
        .map(|(name, object)| (name.clone(), canonical_envelope(object)))
        .collect();
    for tip in from.tips().expect("tips") {
        to.adopt_objects(&prints, &tip).expect("adopted");
    }
}

/// A conversation whose lines do not fold is a finding of `transcript::findings` by name, beside
/// the store's own (which rehashes objects and never folds them), and the others still read.
#[test]
fn a_conversation_that_does_not_fold_is_a_finding() {
    let memory = Memory::default();
    let said = Entry::Said {
        at: at(1),
        turn: Turn::Person {
            text: "what is due".to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: None,
    };
    // A said line with no opening line before it: every conversation this program writes opens.
    memory.lines.append(event(0, &said)).expect("appended");
    let inbox = InMemory::<suzatary_core::secretary::inbox_event::InboxEvent>::default();
    assert_eq!(
        transcript::findings(&memory.lines, &inbox),
        vec![format!(
            "conversation {} does not read: {} has no opening line — a conversation starts with one",
            id(),
            id()
        )]
    );
}

proptest! {
    /// A line is its shape and its words: joining what `split` takes apart, every slot kept,
    /// gives the line back, and a slot gone joins as words gone, never as the line.
    #[test]
    fn split_and_join_are_a_lawful_pair(script in steps(), gone in any::<prop::sample::Index>()) {
        use suzatary_core::secretary::line_shape::{join, split};
        use suzatary_core::secretary::words::{Gone, Words};
        for entry in through_the_door(&conversation(&script)) {
            let (shape, words) = split(&entry);
            let words = words.expect("a line the box writes holds its words");
            prop_assert_eq!(words.len(), shape.slots());
            let kept: Vec<_> = words.iter().cloned().map(Words::kept).collect();
            prop_assert_eq!(join(&id(), shape.clone(), entry.at(), kept.clone()), entry.clone());
            if !kept.is_empty() {
                let mut lost = kept;
                let at = gone.index(lost.len());
                lost[at] = Gone::Missing.into();
                let read = join(&id(), shape.clone(), entry.at(), lost);
                prop_assert_ne!(&read, &entry);
                prop_assert_eq!(split(&read).0, shape);
            }
        }
    }

    /// The transcript register is the file's fold through the door: `Stored::of` over the same
    /// lines, scrubbed.
    #[test]
    fn the_transcript_reads_as_the_file(script in steps()) {
        let lines = conversation(&script);
        let (_, read) = written(&lines);
        let door = through_the_door(&lines);
        prop_assert_eq!(read.entries(), door.clone());
        prop_assert_eq!(read.stored(), &Stored::of(&id(), &door).expect("folds"));
    }

    /// The owed register is the queue: every turn `queue::delta` would send, and nothing else.
    #[test]
    fn owed_is_the_queue(script in steps()) {
        let lines = conversation(&script);
        let (_, read) = written(&lines);
        let file = Stored::of(&id(), &through_the_door(&lines)).expect("folds");
        let owed: Vec<_> = read.owed().into_iter().map(|(turn, said)| (turn, said.clone())).collect();
        let queued: Vec<_> = queue::delta(&file).into_iter().map(|(turn, said)| (turn, said.clone())).collect();
        prop_assert_eq!(owed, queued);
    }

    /// THE OWED LAW, by descent: a live turn is owed exactly while no live answer with no calls
    /// and no live stop rests on it or is it. So a stop closes its question in its own one write,
    /// and no reading after that write can call the question owed again.
    #[test]
    fn owed_is_what_no_answer_and_no_stop_rests_on(script in steps()) {
        let lines = conversation(&script);
        let (_, read) = written(&lines);
        let stored = read.stored();
        // The turns' lines, as `Transcript::of` places them: every said line and every stop.
        let turn_lines: Vec<usize> = read
            .entries()
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, entry)| matches!(entry, Entry::Said { .. } | Entry::Stopped { .. }))
            .map(|(at, _)| at)
            .collect();
        let rests_on = |later: usize, earlier: usize| {
            later == earlier || read.beneath(turn_lines[later]).contains(&turn_lines[earlier])
        };
        let closing = |turn: usize| {
            let closes = match &stored.turns[turn].turn {
                Turn::Model { calls, .. } => calls.is_empty(),
                Turn::Act { .. } => stored.stops.contains(&turn),
                Turn::Person { .. } | Turn::Tool { .. } | Turn::Erased { .. } => false,
            };
            stored.is_live(turn) && closes
        };
        let expected: Vec<usize> = stored
            .live()
            .into_iter()
            .map(|(turn, _)| turn)
            .filter(|&turn| !(0..stored.turns.len()).any(|c| closing(c) && rests_on(c, turn)))
            .collect();
        let owed: Vec<usize> = read.owed().into_iter().map(|(turn, _)| turn).collect();
        prop_assert_eq!(owed, expected);
    }

    /// THE LINES LAW: with any subset of the inbox's askings removed, the lines are the
    /// conversation's proposals, one each, in order; a line is unplaced exactly when its asking
    /// is missing, as answered-and-never-asked where it has an answer and as not held where it has
    /// none; and every placed line reads as it does against the whole inbox.
    #[test]
    fn the_lines_are_the_proposals_each_placed_or_named(
        script in steps(),
        answers in proptest::collection::vec(any::<u8>(), 0..60),
        dropped in proptest::collection::vec(any::<bool>(), 0..60),
    ) {
        let lines = conversation(&script);
        let file = Stored::of(&id(), &through_the_door(&lines)).expect("folds");
        let proposals = file.proposals.len();
        let whole = file.lines(&inbox_of(proposals, &answers, &[]));
        let read = file.lines(&inbox_of(proposals, &answers, &dropped));
        let askings: Vec<_> = read.iter().map(|line| line.asking().clone()).collect();
        prop_assert_eq!(&askings, &file.proposals);
        for (n, (line, all)) in read.iter().zip(whole.iter()).enumerate() {
            let missing = dropped.get(n).copied().unwrap_or(false);
            // The whole inbox's names differ, since a missing asking moves every later seal.
            let seen = |standing: &suzatary_core::secretary::store::Standing| {
                (
                    standing.object.id.clone(),
                    standing.object.asked.clone(),
                    standing.object.reading.shown().map(|answer| answer.response.clone()),
                )
            };
            match (line, all) {
                (Line::Unplaced(unplaced), _) => {
                    prop_assert!(missing);
                    let answered = answers.get(n).copied().unwrap_or(0) % 5 != 0;
                    prop_assert_eq!(
                        unplaced.why,
                        if answered { Unheld::Unasked } else { Unheld::NotHeld }
                    );
                }
                (Line::Placed(standing), Line::Placed(whole)) => {
                    prop_assert!(!missing);
                    prop_assert_eq!(seen(standing.as_ref()), seen(whole.as_ref()));
                }
                (Line::Placed(_), Line::Unplaced(_)) => {
                    prop_assert!(false, "the whole inbox places every line");
                }
            }
        }
    }

    /// Nothing read depends on the order the objects are handed over in.
    #[test]
    fn the_reading_ignores_the_order_objects_arrive_in(script in steps(), seed in any::<u64>()) {
        let lines = conversation(&script);
        let (memory, read) = written(&lines);
        let mut objects = memory.lines.objects().expect("objects");
        let mut state = seed | 1;
        for at in (1..objects.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            objects.swap(at, usize::try_from(state % (at as u64 + 1)).expect("an index"));
        }
        let all = transcript::of_objects(objects, &memory.conversations().slots()).expect("reads");
        prop_assert_eq!(all.get(&id()), Some(&read));
    }

    /// Every line prints and parses back to itself, so the store's bytes are its reading.
    #[test]
    fn every_line_prints_and_parses_back(script in steps()) {
        let lines = conversation(&script);
        for (held, line) in lines.iter().enumerate() {
            let event = event(held, line);
            let printed = canonical(&event);
            prop_assert_eq!(parse_event::<LineEvent>(&LineVocabulary, &printed).expect("parses"), event);
        }
    }

    /// No stored line holds its words: neither a credential nor any word a turn, a title or a
    /// review said is in an object, and each opens again through its keys.
    #[test]
    fn a_stored_line_holds_no_words(script in steps()) {
        let lines = conversation(&script);
        let (memory, read) = written(&lines);
        for (_, object) in memory.lines.objects().expect("objects") {
            let print = canonical_envelope(&object);
            for words in ["signin-tok", "sign in", "permit is due", "what changed", "flight", "Morning", "Topic ", "about money", "unreachable", "dismissed a proposal"] {
                prop_assert!(!print.contains(words), "{} in {}", words, print);
            }
        }
        prop_assert_eq!(read.entries(), through_the_door(&lines));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// The Prodrome's own store, a `Genesis` and then a `Change` per line, reads as the store in
    /// memory and verifies clean.
    #[test]
    fn the_prodrome_store_reads_as_memory(script in steps()) {
        let lines = conversation(&script);
        let (_, read) = written(&lines);
        let scratch = Scratch::new("fidelity");
        let store = disk(&scratch);
        on_disk(&store, &lines, 0);
        let all = transcript::of_objects(objects_of(&store), &key).expect("reads");
        prop_assert_eq!(all[&id()].stored(), read.stored());
        prop_assert!(store.verify().is_empty(), "{:?}", store.verify());
    }

    /// Two replicas share a prefix, each writes lines the other does not see, and each adopts the
    /// other's objects in either order: both read the same, every line of both is there, and
    /// each replica's own lines keep their order. The convergence is the history's (a set of
    /// objects), never the order they arrived in.
    #[test]
    fn two_replicas_converge_whatever_order_lines_arrive_in(
        script in steps(),
        split in 0usize..60,
        mine in 1usize..6,
        theirs in 1usize..6,
    ) {
        let lines = conversation(&script);
        let shared = 1 + split % lines.len();
        let (one, two) = (Scratch::new("one"), Scratch::new("two"));
        let (a, b) = (disk(&one), Disk::new(&two.0, Everything));
        on_disk(&a, &lines[..shared], 0);
        adopt(&b, &a);
        let ask = |n: usize, who: &str| said(
            u32::try_from(n).expect("a second"),
            Turn::Person {
                text: format!("{who} {n}"),
                references: Vec::new(),
                attachments: Vec::new(),
            },
        );
        let a_lines: Vec<Entry> = (0..mine).map(|n| ask(n, "phone")).collect();
        let b_lines: Vec<Entry> = (0..theirs).map(|n| ask(n, "laptop")).collect();
        on_disk(&a, &a_lines, shared);
        on_disk(&b, &b_lines, shared);
        adopt(&a, &b);
        adopt(&b, &a);
        let read_a = transcript::of_objects(objects_of(&a), &key).expect("reads");
        let read_b = transcript::of_objects(objects_of(&b), &key).expect("reads");
        prop_assert_eq!(&read_a, &read_b);
        let entries = read_a[&id()].entries();
        prop_assert_eq!(entries.len(), shared + mine + theirs);
        for own in [&a_lines, &b_lines] {
            let at: Vec<usize> = own
                .iter()
                .map(|line| entries.iter().position(|entry| entry == line).expect("held"))
                .collect();
            prop_assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{:?}", at);
        }
        prop_assert!(a.verify().is_empty() && b.verify().is_empty());
    }
}

/// Two phones ask without seeing each other and the answer rests on one question: the other is
/// still owed, though the order puts it before the answer, where the queue would call it read.
#[test]
fn a_question_the_answer_did_not_see_stays_owed() {
    let (one, two) = (Scratch::new("seen"), Scratch::new("unseen"));
    let (a, b) = (disk(&one), Disk::new(&two.0, Everything));
    on_disk(&a, &[opened()], 0);
    adopt(&b, &a);
    let ask = |second: u32, text: &str| {
        said(
            second,
            Turn::Person {
                text: text.to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
        )
    };
    on_disk(&a, &[ask(2, "seen")], 1);
    on_disk(&b, &[ask(1, "unseen")], 1);
    on_disk(
        &a,
        &[said(
            3,
            Turn::Model {
                text: "answered".to_owned(),
                calls: Vec::new(),
            },
        )],
        2,
    );
    adopt(&a, &b);
    let all = transcript::of_objects(objects_of(&a), &key).expect("reads");
    let read = &all[&id()];
    assert_eq!(
        read.entries()[1],
        ask(1, "unseen"),
        "of two lines nothing orders, the earlier stamp first"
    );
    assert!(queue::delta(read.stored()).is_empty());
    let owed: Vec<usize> = read.owed().iter().map(|(turn, _)| *turn).collect();
    assert_eq!(owed, [0], "the unseen question");
}

/// A stop closes what it rests on and nothing else, however a replica receives the lines: the
/// question it stopped is not owed on either side, and one a second phone asked without seeing
/// the stop stays owed on both, whichever replica adopts the other.
#[test]
fn a_stop_closes_what_it_saw_on_every_replica() {
    let (one, two) = (Scratch::new("stopper"), Scratch::new("asker"));
    let (a, b) = (disk(&one), Disk::new(&two.0, Everything));
    let ask = |second: u32, text: &str| {
        said(
            second,
            Turn::Person {
                text: text.to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
        )
    };
    on_disk(&a, &[opened(), ask(1, "read every thread")], 0);
    adopt(&b, &a);
    on_disk(
        &a,
        &[Entry::Stopped {
            at: at(3),
            by: CAST.owner.actor.to_owned(),
        }],
        2,
    );
    on_disk(&b, &[ask(2, "and the flight?")], 2);
    let owed_on = |store: &Disk| -> Vec<String> {
        let all = transcript::of_objects(objects_of(store), &key).expect("reads");
        all[&id()]
            .owed()
            .into_iter()
            .filter_map(|(_, said)| match &said.turn {
                Turn::Person { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    assert!(owed_on(&a).is_empty(), "the stopped question is closed");
    adopt(&a, &b);
    adopt(&b, &a);
    assert_eq!(owed_on(&a), ["and the flight?"]);
    assert_eq!(owed_on(&b), owed_on(&a), "both replicas read the same");
}

/// The live files' shape, measured on the box: a median line of 700 bytes, the 99th percentile
/// 59 KB, the largest 196 KB.
const MEDIAN: f64 = 700.0;
const P99: f64 = 59_000.0;
const LARGEST: usize = 196_000;

/// `n` line sizes at the quantiles of the log-normal through the median and the 99th
/// percentile, capped at the largest, the largest once at least, shuffled from a fixed seed.
fn sizes(n: usize) -> Vec<usize> {
    let sigma = (P99 / MEDIAN).ln() / normal_quantile(0.99);
    let count = f64::from(u32::try_from(n).expect("a count"));
    let mut sizes: Vec<usize> = (0..n)
        .map(|at| {
            let p = (f64::from(u32::try_from(at).expect("a count")) + 0.5) / count;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let size = (MEDIAN * (sigma * normal_quantile(p)).exp()) as usize;
            size.clamp(16, LARGEST)
        })
        .collect();
    if let Some(largest) = sizes.last_mut() {
        *largest = LARGEST;
    }
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for at in (1..sizes.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let other = usize::try_from(state % u64::try_from(at + 1).expect("a count")).expect("fits");
        sizes.swap(at, other);
    }
    sizes
}

/// The standard normal's quantile, to four places (Abramowitz and Stegun 26.2.23).
fn normal_quantile(p: f64) -> f64 {
    let tail = |q: f64| {
        let t = (-2.0 * q.ln()).sqrt();
        t - (2.515_517 + 0.802_853 * t + 0.010_328 * t * t)
            / (1.0 + 1.432_788 * t + 0.189_269 * t * t + 0.001_308 * t * t * t)
    };
    if p < 0.5 { -tail(p) } else { tail(1.0 - p) }
}

/// Prose of about `size` bytes, a sign-in link now and then.
fn prose(size: usize, n: usize) -> String {
    const WORDS: [&str; 8] = [
        "permit",
        "the",
        "renewal",
        "office",
        "Friday",
        "documents",
        "and",
        "sent",
    ];
    let mut text = String::with_capacity(size + 64);
    if n.is_multiple_of(50) {
        text.push_str("https://example.test/login?token=s3cret&next=/ ");
    }
    let mut at = n;
    while text.len() < size {
        text.push_str(WORDS[at % WORDS.len()]);
        text.push(' ');
        at = at.wrapping_mul(31).wrapping_add(7);
    }
    text
}

/// A conversation of the live shape: rounds of a question, a call, a verb's answer and an answer,
/// each line its drawn size as the file would hold it.
fn live_shaped(lines: usize) -> Vec<Entry> {
    let mut entries = vec![opened()];
    for (n, size) in sizes(lines - 1).into_iter().enumerate() {
        let line = |text: String| {
            let turn = match n % 4 {
                0 => Turn::Person {
                    text,
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                1 => Turn::Model {
                    text,
                    calls: vec![ToolCall {
                        verb: "mail_search".into(),
                        args: [("q".to_owned(), serde_json::json!("permit"))].into(),
                    }],
                },
                2 => Turn::Tool {
                    verb: "mail_search".into(),
                    answer: text,
                    outcome: suzatary_core::secretary::outcome::Outcome::Answered,
                    files: Vec::new(),
                },
                _ => Turn::Model {
                    text,
                    calls: Vec::new(),
                },
            };
            said(u32::try_from(n).expect("a second"), turn)
        };
        let around = weight(&line(String::new()));
        entries.push(line(prose(size.saturating_sub(around).max(16), n)));
    }
    entries
}

fn timed<T>(what: &str, work: impl FnOnce() -> T) -> T {
    let started = std::time::Instant::now();
    let out = work();
    println!(
        "{what:<44} {:>10.1} ms",
        started.elapsed().as_secs_f64() * 1e3
    );
    out
}

/// The sizes of one conversation's lines, printed.
fn describe(entries: &[Entry]) {
    let mut bytes: Vec<usize> = entries.iter().map(weight).collect();
    bytes.sort_unstable();
    println!(
        "per conversation: lines {}, median {} B, p99 {} B, largest {} B, total {} B",
        bytes.len(),
        bytes[bytes.len() / 2],
        bytes[bytes.len() * 99 / 100],
        bytes[bytes.len() - 1],
        bytes.iter().sum::<usize>()
    );
}

/// Not a law: the numbers in the stage-2 report. Run in release with
/// `cargo test --release -p suzatary-core --test all transcript:: -- --ignored --nocapture`.
///
/// One store holds every conversation, so its size is the live total: 35 conversations of 100
/// lines of the live shape, about the 3,472 lines measured on the box. All but the last ten
/// lines of each are written as objects at once; the rest go through the store's append, one
/// line at a time, as the server writes them.
#[test]
#[ignore = "a measurement, printed, not a law"]
fn measure_the_store_at_the_live_size() {
    const CONVERSATIONS: usize = 35;
    const LINES: usize = 100;
    const APPENDED: usize = 10;
    let entries = live_shaped(LINES);
    describe(&entries);
    let ids: Vec<ConversationId> = (0..CONVERSATIONS)
        .map(|n| ConversationId::new(format!("2026-09-{:02}-{n}", 1 + n % 30)).expect("an id"))
        .collect();
    let line = |id: &ConversationId, held: usize| line_of(id, held, &entries[held]);

    let scratch = Scratch::new("measure");
    let store = disk(&scratch);
    let genesis = store
        .tips()
        .expect("tips")
        .into_iter()
        .next()
        .expect("a genesis");
    let mut prints = BTreeMap::new();
    let mut heads = Vec::new();
    timed("build: objects for all but the last lines", || {
        for id in &ids {
            let mut head: Option<Hash> = None;
            for held in 0..LINES - APPENDED {
                let change = Envelope::Change(
                    prodrome_core::change::mk_change(
                        genesis.clone(),
                        head.iter().cloned().collect(),
                        line(id, held),
                    )
                    .expect("a change"),
                );
                let name = prodrome_core::event::seal_hash(&change);
                prints.insert(name.clone(), canonical_envelope(&change));
                head = Some(name);
            }
            heads.extend(head);
        }
    });
    timed("adopt them", || {
        for head in &heads {
            store.adopt_objects(&prints, head).expect("adopted");
        }
    });
    let mut each = Vec::new();
    timed("append the last lines, one at a time", || {
        for held in LINES - APPENDED..LINES {
            for id in &ids {
                let started = std::time::Instant::now();
                store.append(line(id, held)).expect("appended");
                each.push(started.elapsed());
            }
        }
    });
    let mean =
        each.iter().sum::<std::time::Duration>() / u32::try_from(each.len()).expect("a count");
    println!(
        "  one append into {} lines (mean of {})   {:>6.1} ms",
        CONVERSATIONS * LINES,
        each.len(),
        mean.as_secs_f64() * 1e3
    );
    let on_disk: u64 = std::fs::read_dir(scratch.0.join("objects"))
        .expect("objects")
        .map(|file| file.expect("a file").metadata().expect("metadata").len())
        .sum();
    println!("  objects on disk: {on_disk} B");

    let objects = timed("read: load and rehash every object", || objects_of(&store));
    let all = timed("fold: every transcript, Stored and owed", || {
        let all = transcript::of_objects(objects, &key).expect("reads");
        for read in all.values() {
            let _ = read.owed();
        }
        all
    });
    assert_eq!(all.len(), CONVERSATIONS);
    let door: Vec<Entry> = entries
        .iter()
        .map(|entry| scrubbed(&ids[0], entry))
        .collect();
    assert_eq!(all[&ids[0]].entries(), door);
    let findings = timed("verify: the store's fsck", || store.verify());
    assert!(findings.is_empty(), "{findings:?}");
}
