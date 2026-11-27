//! The transcript's laws over the conversations held in core's memory: what a restart answers,
//! the order lines read back in, and what an erasure destroys.

use prodrome_data::event::Actor;
use prodrome_data::literal::Datetime;
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::Entry;
use suzatary_lines::secretary::conversations::Conversations;
use suzatary_lines::secretary::spoken::Plain;
use suzatary_lines::secretary::spoken::Spoken;
use suzatary_lines::secretary::transcript::Retry;
use suzatary_lines::secretary::transcript::*;
use suzatary_memory::secretary::erasure::Erasure;
use suzatary_memory::secretary::locked_text::KeyName;
use suzatary_memory::secretary::words::{Gone, Words};
use suzatary_store::store::Reads as _;
use suzatary_store::store::StoreError;

fn id(text: &str) -> ConversationId {
    ConversationId::new(text).expect("a slug")
}

fn at(second: u32) -> Datetime {
    Datetime::new(2026, 9, 30, 9, 0, second, 0).expect("an instant")
}

fn opened<T>() -> Entry<T> {
    Entry::Opened {
        at: at(0),
        title: suzatary_memory::secretary::words::Words::kept("Morning".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: suzatary_conversations::secretary::store::Kind::Topic,
    }
}

fn person<T: From<Spoken>>(second: u32, text: &str) -> Entry<T> {
    Entry::Said {
        at: at(second),
        turn: Spoken::person(text.to_owned(), Vec::new(), Vec::new()).into(),
        by: Some("ada".into()),
    }
}

fn model(second: u32, text: &str) -> Entry {
    Entry::Said {
        at: at(second),
        turn: Turn::Model {
            text: text.to_owned(),
            calls: Vec::new(),
        },
        by: None,
    }
}

fn asked() -> Erasure {
    Erasure::Asked {
        by: Actor::new("ada").expect("a name"),
    }
}

use proptest::prelude::*;

/// A cache that cannot forget.
struct Stuck;

impl suzatary_todo::render_cache::RenderCache for Stuck {
    fn forget(&self, _: &suzatary_base::answer::Renderable) -> Result<(), String> {
        Err("the disk said no".to_owned())
    }
}

/// A start resumes a person's question asked within the hour, whatever the model's health; an
/// acknowledgement, a model's turn with no question owed, and a question from an earlier deploy
/// wait for their conversation's next write.
#[test]
fn a_start_resumes_only_what_it_cut_off() {
    let held = suzatary_stores::stores::conversations::InMemory::default();
    let store = held.conversations();
    let hours_ago =
        |hours: u32| Datetime::new(2026, 9, 30, 9 - hours, 0, 0, 0).expect("an instant");
    let lines: [(&str, Entry); 4] = [
        ("2026-09-30-1", person(1, "when is the permit due")),
        (
            "2026-09-30-2",
            Entry::Said {
                at: at(1),
                turn: Turn::Model {
                    text: String::new(),
                    calls: vec![suzatary_conversations::secretary::model::ToolCall {
                        verb: "weather".into(),
                        args: std::collections::BTreeMap::new(),
                    }],
                },
                by: None,
            },
        ),
        (
            "2026-09-30-3",
            Entry::Said {
                at: at(1),
                turn: Turn::Act {
                    text: "Checked Sent".to_owned(),
                    references: Vec::new(),
                },
                by: Some("ada".into()),
            },
        ),
        (
            "2026-09-30-4",
            Entry::Said {
                at: hours_ago(2),
                turn: Turn::Person {
                    text: "from the deploy before".to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: Some("ada".into()),
            },
        ),
    ];
    for (conversation, line) in &lines {
        start(store, &id(conversation), &opened()).expect("started");
        push_older(store, &id(conversation), line).expect("written");
    }
    for retry in [Retry::Allowed, Retry::Held] {
        assert_eq!(
            resumed(store, at(30), retry).expect("reads"),
            [id("2026-09-30-1")]
        );
    }
}

/// A question is claimed by one run at a time, only the round that claimed it closes it, and a
/// question claimed and closed is owed nothing more.
#[test]
fn a_question_is_answered_once_by_the_round_that_claimed_it() {
    use suzatary_lines::secretary::run::Runs as _;
    let held = suzatary_stores::stores::conversations::InMemory::default();
    let store = held.conversations();
    let morning = id("2026-09-30-1");
    start(store, &morning, &opened()).expect("started");
    push_older(store, &morning, &person(1, "when is the permit due")).expect("asked");
    assert_eq!(
        resumed(store, at(2), Retry::Allowed).expect("reads"),
        std::slice::from_ref(&morning)
    );
    let (first, second) = (
        held.runs.begin().expect("a run"),
        held.runs.begin().expect("a run"),
    );
    let claim = |run: &suzatary_lines::secretary::run::Held| {
        suzatary_lines::secretary::transcript::claim(
            store,
            &morning,
            &run.run,
            at(2),
            Retry::Allowed,
            &|_| true,
            |_| panic!("nothing is closed by the box"),
        )
        .expect("reads")
    };
    assert_eq!(claim(&first), Claimed::Began);
    assert_eq!(
        claim(&second),
        Claimed::Nothing,
        "the first run's round is running"
    );
    let answer = |second: u32| {
        suzatary_lines::secretary::answer::BoxLine::Answer(
            suzatary_lines::secretary::answer::Answer::replied(
                at(second),
                "On Friday.".to_owned(),
                Vec::new(),
                suzatary_conversations::secretary::taint::Taint::Clean,
            ),
        )
    };
    assert_eq!(
        push_open(store, &morning, &second.run, &[answer(2)]).expect("read"),
        Answering::Answered
    );
    assert_eq!(
        push_open(store, &morning, &first.run, &[answer(2)]).expect("written"),
        Answering::Written
    );
    let lines = held.lines.objects().expect("reads").len();
    assert_eq!(
        push_open(store, &morning, &first.run, &[answer(3)]).expect("read"),
        Answering::Answered
    );
    assert_eq!(held.lines.objects().expect("reads").len(), lines);
    assert!(read(store, &morning).expect("reads").owed().is_empty());
    assert_eq!(claim(&second), Claimed::Nothing);
}

#[test]
fn lines_read_back_in_the_order_written() {
    let held = suzatary_stores::stores::conversations::InMemory::default();
    let store = held.conversations();
    let morning = id("2026-09-30-1");
    start(store, &morning, &opened()).expect("started");
    for (second, text) in [
        (5, "first"),
        (3, "second, on a clock gone back"),
        (3, "ok"),
        (3, "ok"),
    ] {
        push_older(store, &morning, &person(second, text)).expect("appended");
    }
    let transcript = read(store, &morning).expect("a transcript");
    let entries = transcript.entries();
    assert_eq!(entries.len(), 5, "the same words twice are two lines");
    assert_eq!(entries[1], person(5, "first"));
    assert_eq!(entries[2], person(3, "second, on a clock gone back"));
    let held: Vec<u32> = transcript.lines().iter().map(|w| w.line.held()).collect();
    assert_eq!(held, [0, 1, 2, 3, 4]);
    assert_eq!(transcript.lines()[4].rests_on, [3]);
    assert_eq!(transcript.beneath(4), [0, 1, 2, 3].into());
}

#[test]
fn a_conversation_starts_once_and_is_pushed_to_after() {
    let held = suzatary_stores::stores::conversations::InMemory::default();
    let store = held.conversations();
    let morning = id("2026-09-30-1");
    assert!(matches!(
        push_older(store, &morning, &person(1, "hi")),
        Err(StoreError::Absent { .. })
    ));
    start(store, &morning, &opened()).expect("started");
    assert!(matches!(
        start(store, &morning, &opened()),
        Err(StoreError::Taken { .. })
    ));
    start(store, &id("2026-09-30-2"), &opened()).expect("another");
    assert_eq!(keys(store).expect("keys"), [morning, id("2026-09-30-2")]);
}

/// The door is the store's: a credential pushed is gone from every object.
#[test]
fn a_pushed_line_holds_no_credential() {
    let held = suzatary_stores::stores::conversations::InMemory::default();
    let store = held.conversations();
    let morning = id("2026-09-30-1");
    start(store, &morning, &opened()).expect("started");
    push(
        store,
        &morning,
        &person(1, "reset at https://example.test/r?token=s3cret"),
    )
    .expect("appended");
    for (_, object) in held.lines.objects().expect("objects") {
        let print = prodrome_data::event::canonical_envelope(&object);
        assert!(!print.contains("s3cret"), "{print}");
    }
    assert!(matches!(
        &read(store, &morning).expect("read").entries()[1],
        Entry::Said { turn: Turn::Person { text, .. }, .. }
            if text == "reset at https://example.test/r?token=[redacted]"
    ));
}

/// An erasure takes the words it names, reads as erased with the line's shape whole, forgets
/// what was rendered of them, and destroys their key once: a second sweep finds nothing.
#[test]
fn an_erasure_destroys_its_keys_and_their_renders() {
    let held = suzatary_stores::stores::conversations::InMemory::default();
    let store = held.conversations();
    let morning = id("2026-09-30-1");
    start(store, &morning, &opened()).expect("started");
    push_older(store, &morning, &person(1, "the private thing")).expect("said");
    push_older(store, &morning, &model(2, "First block.\n\nSecond block.")).expect("answered");
    let lines = read(store, &morning).expect("read");
    let answer: std::collections::BTreeSet<KeyName> =
        lines.lines()[2].line.keys().cloned().collect();
    let before = held.keys.names().len();
    assert!(
        answer.iter().all(|key| held.opened.holds(key)),
        "read, so kept"
    );
    let destroyed = erase(store, &morning, answer.clone(), asked(), at(3)).expect("erased");
    assert!(
        answer.iter().all(|key| !held.opened.holds(key)),
        "forgotten with the key"
    );
    assert_eq!(destroyed, 1);
    assert_eq!(held.keys.names().len(), before - 1);
    assert!(answer.iter().all(|key| !held.keys.names().contains(key)));
    assert_eq!(held.forgot.sources(), ["First block.\n\n", "Second block."]);
    let entries = read(store, &morning).expect("read").entries();
    assert_eq!(entries[1], person(1, "the private thing"));
    assert!(matches!(
        &entries[2],
        Entry::Said { turn: Turn::Erased { words, .. }, .. }
            if words.iter().all(|w| matches!(w, Words::Gone { gone: Gone::Erased { .. } }))
    ));
    assert_eq!(sweep(store).expect("swept"), 0);
}

/// An erasure names only words of its own conversation, and some.
#[test]
fn an_erasure_of_words_elsewhere_is_refused() {
    let held = suzatary_stores::stores::conversations::InMemory::default();
    let store = held.conversations();
    let (morning, evening) = (id("2026-09-30-1"), id("2026-09-30-2"));
    start(store, &morning, &opened()).expect("started");
    start(store, &evening, &opened()).expect("started");
    push_older(store, &evening, &person(1, "elsewhere")).expect("said");
    let theirs: std::collections::BTreeSet<KeyName> = read(store, &evening).expect("read").lines()
        [1]
    .line
    .keys()
    .cloned()
    .collect();
    for keys in [theirs, std::collections::BTreeSet::new()] {
        assert!(matches!(
            erase(store, &morning, keys, asked(), at(2)),
            Err(StoreError::NotAppended { .. })
        ));
    }
    assert_eq!(
        read(store, &evening).expect("read").entries()[1],
        person(1, "elsewhere")
    );
}

/// A block that may still be on disk keeps its key, so a later sweep finds it again.
#[test]
fn a_render_not_forgotten_keeps_its_key() {
    let held = suzatary_stores::stores::conversations::InMemory::default();
    let morning = id("2026-09-30-1");
    start(held.conversations(), &morning, &opened()).expect("started");
    push_older(held.conversations(), &morning, &model(1, "An answer.")).expect("answered");
    let stuck = Conversations {
        renders: &Stuck,
        sessions: &suzatary_base::capability::Nothing,
        ..held.conversations()
    };
    let keys: std::collections::BTreeSet<KeyName> = read(stuck, &morning).expect("read").lines()[1]
        .line
        .keys()
        .cloned()
        .collect();
    assert_eq!(erase(stuck, &morning, keys.clone(), asked(), at(2)), Ok(0));
    assert!(matches!(sweep(stuck), Err(StoreError::Unwritable { .. })));
    assert!(keys.iter().all(|key| held.keys.names().contains(key)));
    assert_eq!(sweep(held.conversations()).expect("swept"), 1);
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig { cases: 24, ..Default::default() })]

    /// The lines' laws over any run of turns and erasures: no object holds a turn's words,
    /// after each sweep the keys held are exactly those no erasure names, and what is
    /// destroyed only grows.
    #[test]
    fn the_keys_held_are_the_live_ones_and_the_dead_only_grow(
        steps in proptest::collection::vec(
            proptest::prop_oneof![
                "[a-z]{6,12}".prop_map(Ok),
                proptest::collection::vec(0usize..16, 1..3).prop_map(Err),
            ],
            1..16,
        )
    ) {
        let held = suzatary_stores::stores::conversations::InMemory::default();
        let store = held.conversations();
        let morning = id("2026-09-30-1");
        start(store, &morning, &opened()).expect("started");
        let mut said = Vec::new();
        let mut dead = std::collections::BTreeSet::new();
        for (second, step) in (1u32..).zip(steps) {
            match step {
                Ok(words) => {
                    push_older(store, &morning, &person(second, &format!("<{words}>"))).expect("said");
                    said.push(format!("<{words}>"));
                }
                Err(lines) => {
                    let read = read(store, &morning).expect("read");
                    let keys: std::collections::BTreeSet<KeyName> = lines
                        .iter()
                        .filter_map(|&line| read.lines().get(line))
                        .flat_map(|written| written.line.keys().cloned())
                        .collect();
                    if !keys.is_empty() {
                        erase(store, &morning, keys, asked(), at(second)).expect("erased");
                    }
                }
            }
            let objects = held.lines.objects().expect("objects");
            for (_, object) in &objects {
                let print = prodrome_data::event::canonical_envelope(object);
                for words in &said {
                    prop_assert!(!print.contains(words.as_str()));
                }
            }
            let erased = erased(objects.iter());
            let named: std::collections::BTreeSet<KeyName> = objects
                .iter()
                .filter_map(|(_, object)| object.event())
                .flat_map(|line| line.keys().cloned())
                .collect();
            let live: std::collections::BTreeSet<KeyName> = named
                .iter()
                .filter(|key| !erased.contains_key(*key))
                .cloned()
                .collect();
            let kept: std::collections::BTreeSet<KeyName> =
                held.keys.names().into_iter().collect();
            prop_assert_eq!(&kept, &live);
            let now: std::collections::BTreeSet<KeyName> =
                named.difference(&kept).cloned().collect();
            prop_assert!(dead.is_subset(&now));
            dead = now;
        }
    }
}

/// One conversation's lines as a tab and the box wrote them, each over the last: an opening by
/// the box, then each `(writer, entry)` in turn, read with the keys they were sealed under.
fn written(lines: &[(&str, Plain)]) -> Transcript {
    let (held, opens) = sealed_lines(lines);
    let writes: Vec<Write<'_>> = held
        .iter()
        .map(|(name, line, parents)| Write {
            name,
            line,
            parents,
        })
        .collect();
    Transcript::of(&id("morning"), &writes, &opens).expect("a conversation")
}

/// The digest's reading of the same lines: the taint of the leaf of the line `[writer, …]`
/// places second, the first after the opening.
fn digest_taint(lines: &[(&str, Plain)]) -> suzatary_conversations::secretary::taint::Taint {
    let (held, opens) = sealed_lines(lines);
    let opened: std::collections::BTreeMap<prodrome_data::event::Hash, Entry> = held
        .iter()
        .map(|(name, line, _)| (name.clone(), line.entry(&opens)))
        .collect();
    let placed: Vec<suzatary_digest::secretary::digest::Placed<'_>> = held
        .iter()
        .map(|(name, line, parents)| (name, line, parents.as_slice()))
        .collect();
    let shape = suzatary_digest::secretary::digest::Shape::of(&placed, &opened).expect("a shape");
    shape
        .node(&held[1].0)
        .expect("the line's leaf")
        .measure()
        .taint
}

/// The lines [`written`] reads, each over the last, and what opens their words.
#[expect(
    clippy::type_complexity,
    reason = "the lines and their opener, as a reader takes them"
)]
fn sealed_lines(
    lines: &[(&str, Plain)],
) -> (
    Vec<(
        prodrome_data::event::Hash,
        suzatary_lines::secretary::line_event::LineEvent,
        Vec<prodrome_data::event::Hash>,
    )>,
    impl Fn(&KeyName) -> Words<suzatary_memory::secretary::locked_text::TextKey>,
) {
    let over: Vec<(&str, Plain, Vec<usize>)> = lines
        .iter()
        .enumerate()
        .map(|(at, (writer, entry))| (*writer, entry.clone(), vec![at]))
        .collect();
    sealed_over(&over)
}

/// The lines `(writer, entry, parents)`, each over the lines at `parents` (the opening is 0, the
/// first of `lines` 1), and what opens their words.
#[expect(
    clippy::type_complexity,
    reason = "the lines and their opener, as a reader takes them"
)]
fn sealed_over(
    lines: &[(&str, Plain, Vec<usize>)],
) -> (
    Vec<(
        prodrome_data::event::Hash,
        suzatary_lines::secretary::line_event::LineEvent,
        Vec<prodrome_data::event::Hash>,
    )>,
    impl Fn(&KeyName) -> Words<suzatary_memory::secretary::locked_text::TextKey> + use<>,
) {
    use prodrome_data::event::Hash;
    use suzatary_lines::secretary::line_event::LineEvent;
    struct Fresh(std::sync::atomic::AtomicU8);
    impl suzatary_base::capability::Entropy for Fresh {
        fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
            bytes.fill(self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
            Ok(())
        }
    }
    let entropy = Fresh(std::sync::atomic::AtomicU8::new(1));
    let mut held: Vec<(Hash, LineEvent, Vec<Hash>)> = Vec::new();
    let mut keys = Vec::new();
    let first = [("assistant", opened(), Vec::new())];
    for (writer, entry, over) in first.iter().chain(lines) {
        let (line, sealed) = LineEvent::write(
            id("morning"),
            Actor::new(*writer).expect("an actor"),
            0,
            entry,
            &entropy,
        )
        .expect("a line");
        let parents: Vec<Hash> = over.iter().map(|&at| held[at].0.clone()).collect();
        let name = Hash::new(format!("{:064x}", held.len() + 1)).expect("a name");
        keys.extend(sealed);
        held.push((name, line, parents));
    }
    let opens = move |name: &KeyName| -> Words<suzatary_memory::secretary::locked_text::TextKey> {
        keys.iter()
            .find(|key| key.name() == *name)
            .and_then(|key| {
                suzatary_memory::secretary::locked_text::TextKey::parse(name, key.bytes())
            })
            .map_or_else(|| Gone::Missing.into(), Words::kept)
    };
    (held, opens)
}

/// Law 10: a turn another writer wrote reads tainted to every reader until a claim judges it,
/// and as the join of every judgment after: tainted when any claim judged it so. The transcript
/// and the digest's leaf read it alike.
#[test]
fn a_tab_s_turn_reads_tainted_until_every_claim_judged_it_clean() {
    use suzatary_conversations::secretary::store::{Covered, RunId};
    use suzatary_conversations::secretary::taint::{Taint, turn};
    use suzatary_inbox::secretary::stamp::{Speaker, Stamp};
    let asked = Entry::Said {
        at: at(1),
        turn: Spoken::person("Is the permit due?".to_owned(), Vec::new(), Vec::new()),
        by: Some(Stamp::Actor(Speaker::device("device0"))),
    };
    let question = prodrome_data::event::Hash::new(format!("{:064x}", 2)).expect("a name");
    let judged = |taint: Taint, second: u32| {
        (
            "assistant",
            Entry::Began {
                at: at(second),
                covers: vec![Covered {
                    line: question.clone(),
                    taint,
                }],
                run: RunId::of([u8::try_from(second).expect("a byte"); 16]),
            },
        )
    };
    let taint_of = |read: &Transcript| turn(&read.stored().turns[0]);

    let unjudged = written(&[("device0", asked.clone())]);
    assert_eq!(
        taint_of(&unjudged),
        Taint::Tainted,
        "no claim judged it yet"
    );
    assert_eq!(
        digest_taint(&[("device0", asked.clone())]),
        Taint::Tainted,
        "the digest's leaf too"
    );

    let clean = written(&[("device0", asked.clone()), judged(Taint::Clean, 2)]);
    assert_eq!(taint_of(&clean), Taint::Clean);
    assert_eq!(
        digest_taint(&[("device0", asked.clone()), judged(Taint::Clean, 2)]),
        Taint::Clean
    );

    let both = written(&[
        ("device0", asked.clone()),
        judged(Taint::Clean, 2),
        judged(Taint::Tainted, 3),
    ]);
    assert_eq!(taint_of(&both), Taint::Tainted, "the join of the judgments");
    assert_eq!(
        digest_taint(&[
            ("device0", asked.clone()),
            judged(Taint::Clean, 2),
            judged(Taint::Tainted, 3),
        ]),
        Taint::Tainted,
        "the digest's leaf reads the join"
    );

    let boxed = written(&[("assistant", asked)]);
    assert_eq!(
        taint_of(&boxed),
        Taint::Clean,
        "the box's own turn keeps its stamp"
    );
}

/// A stop ends exactly the rounds open before it in the transcript: the round whose claim was
/// written before the stop was taken, whether or not the tab saw that claim, and never a round
/// whose claim rests on the stop, which is what a round begun after the take writes. So the halt
/// a stop pulls is a reading of its line, never "the round running now".
#[test]
fn a_stop_ends_the_rounds_before_it_and_none_begun_after_its_take() {
    use suzatary_conversations::secretary::store::{Covered, RunId};
    use suzatary_conversations::secretary::taint::Taint;
    use suzatary_inbox::secretary::stamp::{Speaker, Stamp};
    let name =
        |at: usize| prodrome_data::event::Hash::new(format!("{:064x}", at + 1)).expect("a name");
    let asked = |second: u32| Entry::Said {
        at: at(second),
        turn: Spoken::person(format!("question {second}"), Vec::new(), Vec::new()),
        by: Some(Stamp::Actor(Speaker::device("device0"))),
    };
    let began = |second: u32, covers: usize, run: u8| Entry::Began {
        at: at(second),
        covers: vec![Covered {
            line: name(covers),
            taint: Taint::Clean,
        }],
        run: RunId::of([run; 16]),
    };
    let stopped = |second: u32| Entry::Stopped {
        at: at(second),
        by: "device0".to_owned(),
    };
    // 1: a question; 2: the first round's claim; 3: the tab's stop, over `saw`; 4: a question
    // said after it; 5: the second round's claim, written over every line held, the stop among them.
    // The stop's clock reads before the claim's and after it: no clock decides which rounds it ends.
    let cases = [vec![1], vec![2]]
        .into_iter()
        .flat_map(|saw| [(saw.clone(), 1), (saw, 3)]);
    for (saw, clock) in cases {
        let lines = [
            ("device0", asked(1), vec![0]),
            ("assistant", began(2, 1, 1), vec![1]),
            ("device0", stopped(clock), saw.clone()),
            ("device0", asked(4), vec![3]),
            ("assistant", began(5, 4, 2), vec![2, 4]),
        ];
        let (held, opens) = sealed_over(&lines);
        let writes: Vec<Write<'_>> = held
            .iter()
            .map(|(name, line, parents)| Write {
                name,
                line,
                parents,
            })
            .collect();
        let read = Transcript::of(&id("morning"), &writes, &opens).expect("a conversation");
        assert_eq!(
            read.ended_by(&name(3)),
            std::collections::BTreeSet::from([RunId::of([1; 16])]),
            "a stop over {saw:?} at {clock} ends the first round alone"
        );
        assert!(
            read.ended_by(&name(4)).is_empty(),
            "a question ends no round"
        );
    }
}

/// A line resting on two lines one of which rests on the other reads as the same line resting on
/// the upper one alone: every reading of a conversation reads what a line rests on through its
/// ancestry, which the dep beneath the other does not change. A store may hold such a line from
/// before the door refused one; `verify` names it ("change … depends on …, which its dep …
/// already rests on"), and it stays.
#[test]
fn a_dep_beneath_another_dep_reads_as_if_it_were_not_there() {
    let read = |last: Vec<usize>| {
        let (held, opens) = sealed_over(&[
            ("ada", person(1, "Is the permit due?"), vec![0]),
            ("ada", person(2, "And the fee?"), vec![1]),
            ("ada", person(3, "Thanks."), last),
        ]);
        let writes: Vec<Write<'_>> = held
            .iter()
            .map(|(name, line, parents)| Write {
                name,
                line,
                parents,
            })
            .collect();
        Transcript::of(&id("morning"), &writes, &opens).expect("a conversation")
    };
    // The opening is 0: the last question rests on the second, and on the opening beneath it.
    let (redundant, alone) = (read(vec![2, 0]), read(vec![2]));
    assert_eq!(redundant.entries(), alone.entries());
    assert_eq!(redundant.stored(), alone.stored());
    assert_eq!(redundant.turn_lines(), alone.turn_lines());
    assert_eq!(redundant.owed(), alone.owed());
    assert_eq!(redundant.queued(), alone.queued());
    assert_eq!(redundant.state(), alone.state());
    let names =
        |read: &Transcript| -> Vec<_> { read.lines().iter().map(|w| w.name.clone()).collect() };
    assert_eq!(names(&redundant), names(&alone), "one causal order");
    for line in 0..redundant.lines().len() {
        assert_eq!(redundant.beneath(line), alone.beneath(line));
    }
}
