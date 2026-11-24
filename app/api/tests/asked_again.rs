//! Two tabs asking, stopping and asking again over one store, as the box claims and closes. Each
//! tab seals over what it holds and hands its lines over through the lines' rule; the box claims
//! rounds by `Began`, joins, answers, and its runs die. The laws:
//! - every ask (a question, or a question a taken `Again` asks again) is covered by the rounds of
//!   at most two runs, and never by two runs whose processes both live;
//! - an `Again` handed over while an open round covers its question is refused, and so is one
//!   that would withdraw a round another live question was sent in;
//! - once both tabs hold every line, each reads the transcript the box reads.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU8, Ordering};

use prodrome_core::event::{Actor, Envelope, Hash, canonical_envelope};
use prodrome_core::literal::Datetime;
use prodrome_core::store::frontier::{Frontiers, KEPT, Since};
use suzatary_api::routes::{self, Books, Cut};
use suzatary_api::sync::Holding;
use suzatary_base::capability::Entropy;
use suzatary_base::conversation_id::DEFAULT;
use suzatary_conversations::secretary::model::Turn;
use suzatary_conversations::secretary::store::{Entry, Kind, RunId};
use suzatary_lines::secretary::held_replica::{
    AskedAgain, HeldReplica, NotAnAnswer, Question, Sealing, Whole,
};
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_lines::secretary::lines_rule::{LinesRule, SlotKeys};
use suzatary_lines::secretary::own_line::OwnLine;
use suzatary_lines::secretary::run::{Held, Runs as _};
use suzatary_lines::secretary::transcript::{self, Retry, Transcript};
use suzatary_memory::secretary::words::Words;
use suzatary_signatures::wire::signatures::RefusalKind;
use suzatary_store::admission::Beside as _;
use suzatary_store::replica::held::Node as _;
use suzatary_store::replica::writes::{Answer, Fit, Refusal};
use suzatary_store::store::{Prodrome as _, Reads as _};
use suzatary_stores::stores::conversations::InMemory;

/// Each key one more than the last: no two slots of any line share a key.
struct Counting(AtomicU8);

impl Entropy for Counting {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(self.0.fetch_add(1, Ordering::Relaxed));
        Ok(())
    }
}

fn at(second: u32) -> Datetime {
    Datetime::new(2026, 11, 17, 9, 0, second, 0).expect("an instant")
}

/// How a tab asks again: as it was, in new words, or in words the rule refuses (none), so the
/// box takes the edit's `Again` and refuses the question resting on it.
#[derive(Debug, Clone, Copy)]
enum How {
    Regenerate,
    Edit,
    EditRefused,
}

/// What another writer seals over a dangling edit's `Again` alone, at its instant: a plain
/// question (a second device whose clock repeats the edit's instant), a stop, an act (which the
/// box alone writes, so it is refused), an edit's own question by the `Again`'s writer (another
/// device of the same person), which takes the edit up by its kind, and one by another actor,
/// which the rule refuses: it would set the first actor's question aside on another's word.
#[derive(Debug, Clone, Copy)]
enum Craft {
    Question,
    Stop,
    Act,
    Edited,
    EditedByOther,
}

/// One step of the tabs or of the box.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Tab `n` seals a question over what it holds.
    Say(usize),
    /// Tab `n` seals a stop.
    Stop(usize),
    /// Tab `n` asks a live question again, the `k`th from the last (wrapping), over the lines it
    /// draws.
    Again(usize, How, usize),
    /// Tab `n` hands over what it holds unsent in one post, as the tab does.
    Post(usize),
    /// Tab `n` hands over only the first line the box can take, as a post cut by size does: an
    /// edit's pair may be split across posts with the box's claims and answers between.
    PostOne(usize),
    /// A line another writer seals over a dangling edit's `Again` alone, at its instant: what a
    /// crafted or wrong-clocked device can post.
    Reuse(Craft),
    /// Tab `n` lands what the box serves, with every print.
    Land(usize),
    /// The box's claim step, for a new run.
    Claim,
    /// The `n`th live round joins what is queued, and asks again.
    Join(usize),
    /// The `n`th live round answers and closes; its run ends.
    Answer(usize),
    /// The `n`th live run's process dies.
    Die(usize),
}

/// A question said, sent, answered and landed in both tabs, then any steps: so most cases reach
/// an `Again` the box takes (`most_cases_take_an_again`).
fn steps() -> impl proptest::strategy::Strategy<Value = Vec<Step>> {
    use proptest::prelude::*;
    let tab = 0usize..3;
    let how = prop_oneof![
        2 => Just(How::Regenerate),
        2 => Just(How::Edit),
        1 => Just(How::EditRefused),
    ];
    proptest::collection::vec(
        prop_oneof![
            3 => tab.clone().prop_map(Step::Say),
            1 => tab.clone().prop_map(Step::Stop),
            4 => (tab.clone(), how, 0usize..4)
                .prop_map(|(t, how, pick)| Step::Again(t, how, pick)),
            4 => tab.clone().prop_map(Step::Post),
            2 => tab.clone().prop_map(Step::PostOne),
            2 => prop_oneof![
                3 => Just(Craft::Question),
                1 => Just(Craft::Stop),
                1 => Just(Craft::Act),
                1 => Just(Craft::Edited),
                1 => Just(Craft::EditedByOther),
            ]
            .prop_map(Step::Reuse),
            2 => tab.prop_map(Step::Land),
            3 => Just(Step::Claim),
            1 => (0usize..3).prop_map(Step::Join),
            2 => (0usize..3).prop_map(Step::Answer),
            1 => (0usize..3).prop_map(Step::Die),
        ],
        1..40,
    )
    .prop_map(|steps| {
        let mut all = vec![
            Step::Say(0),
            Step::Post(0),
            Step::Claim,
            Step::Answer(0),
            Step::Land(0),
            Step::Land(1),
        ];
        all.extend(steps);
        all
    })
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(400))]

    #[test]
    fn two_tabs_asking_again_spend_at_most_two_calls_an_ask_and_read_as_the_box(
        steps in steps(),
    ) {
        asked(&steps)?;
    }
}

/// A regenerate taken reads on the tab, before the box holds it, as the box reads it after: its
/// answer set aside and its question owed, the ruling drawn on that question; and an edit taken
/// whole sets its question aside and owes the new one.
#[test]
fn a_tab_s_again_reads_as_the_box_reads_it_once_taken() {
    use Step::{Again, Answer, Claim, Land, Post, Say};
    asked(&[
        Say(0),
        Post(0),
        Claim,
        Answer(0),
        Land(0),
        Again(0, How::Regenerate, 0),
        Post(0),
        Claim,
        Answer(0),
        Land(0),
        Again(0, How::Edit, 0),
        Post(0),
    ])
    .expect("the laws hold");
}

/// An edit while a round covers its question is refused, and the tab's reading returns to the
/// box's: the answer under way is not set aside, and the new question rests on a refusal.
#[test]
fn an_again_over_a_working_claim_is_refused() {
    use Step::{Again, Answer, Claim, Join, Land, Post, Say};
    asked(&[
        Say(0),
        Post(0),
        Claim,
        Answer(0),
        Land(0),
        Say(1),
        Post(1),
        Claim,
        Land(0),
        Again(0, How::Edit, 0),
        Post(0),
        Join(0),
        Answer(0),
    ])
    .expect("the laws hold");
}

/// The box: its store, its live runs, and every run it ever began.
struct Boxed {
    lines: InMemory,
    live: Vec<Held>,
    begun: Vec<RunId>,
    /// Each ask (a question's line, and how many taken `Again` lines named it) with the runs that
    /// sent it to a model.
    calls: BTreeMap<(Hash, usize), BTreeSet<RunId>>,
}

impl Boxed {
    fn read(&self) -> std::sync::Arc<Transcript> {
        transcript::read(self.lines.conversations(), &DEFAULT).expect("the box reads")
    }

    /// The runs `run` sent its model, as asks, checked: at most two runs an ask, and no second run
    /// whose process lives.
    fn sent(&mut self, run: &RunId) -> Result<(), proptest::test_runner::TestCaseError> {
        let read = self.read();
        let names = read.turn_lines();
        let covered = |run: &RunId| -> BTreeSet<Hash> {
            read.sending(run)
                .into_iter()
                .filter(|(_, said)| matches!(said.turn, Turn::Person { .. }))
                .map(|(turn, _)| names[turn].clone())
                .collect()
        };
        for line in covered(run) {
            let asked_again = (0..read.lines().len())
                .filter(|&at| matches!(&read.lines()[at].entry, Entry::Again { question, .. } if *question == line) && active(&read, at))
                .count();
            let runs = self.calls.entry((line.clone(), asked_again)).or_default();
            runs.insert(run.clone());
            proptest::prop_assert!(
                runs.len() <= 2,
                "ask {line} (asked again {asked_again} times) went to {} runs",
                runs.len()
            );
            for other in &self.begun {
                proptest::prop_assert!(
                    other == run || self.lines.runs.gone(other) || !covered(other).contains(&line),
                    "two live runs send {line} at once"
                );
            }
        }
        Ok(())
    }

    /// Every question an open round covers, by its line.
    fn working(&self) -> BTreeSet<Hash> {
        let read = self.read();
        let names = read.turn_lines();
        self.begun
            .iter()
            .flat_map(|run| read.sending(run))
            .map(|(turn, _)| names[turn].clone())
            .collect()
    }
}

/// The box's close of a question it will not answer, as `Round::close` writes it.
fn closing(second: u32, text: &str) -> Entry {
    Entry::Said {
        at: at(second),
        turn: Turn::Model {
            text: text.to_owned(),
            calls: Vec::new(),
        },
        by: None,
    }
}

fn question(text: &str) -> Question {
    Question {
        text: text.to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
        seat: suzatary_signatures::signatures::Seat::Device,
    }
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

/// What the box holds landed in `tab`, every print held beside it.
fn land(lines: &InMemory, tab: &mut HeldReplica, fetched: u64) {
    let served = skeleton(lines);
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
        fetched,
    )
    .expect("a piece");
    let _ = tab.land();
    let names: Vec<Hash> = lines
        .lines
        .objects()
        .expect("held")
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let prints: Vec<_> = names
        .iter()
        .zip(lines.lines.literals(&names).expect("reads"))
        .filter_map(|(name, literal)| Some((name.clone(), literal?.into_bytes())))
        .collect();
    tab.lines.hold(prints, &[]);
}

/// `posted` handed over in one post, as a tab posts it, each line beside its signature by the
/// device and its slots' keys, through the lines' rule as the route takes it: the box's answer to
/// each line.
pub(crate) fn handed(lines: &InMemory, posted: &[OwnLine], ada: &Actor) -> Vec<(Hash, Answer)> {
    let mut literals = Vec::new();
    let mut keys: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for own in posted {
        let signed: Envelope<LineEvent> = Envelope::Signed(
            suzatary_connect::cast::keys::secret(suzatary_connect::cast::keys::PHONE)
                .sign(own.name()),
        );
        literals.push(own.print().to_owned());
        literals.push(canonical_envelope(&signed));
        keys.insert(
            own.name().as_str().to_owned(),
            own.keys().into_values().collect(),
        );
    }
    let history = lines.lines.objects().expect("held").into_iter().collect();
    let trusting = suzatary_signatures::signatures::Trusting::held(
        &suzatary_signatures::signatures::Keeping::stores(&[
            &suzatary_stores::stores::phone_trusted(&["ada", "bob"]),
        ]),
        &suzatary_connect::cast::keys::root(),
    )
    .expect("reads");
    let slot_keys = SlotKeys::parse(&keys).expect("the keys parse");
    let received = suzatary_api::sync::receive_changes::<LinesRule>(
        Holding {
            history: &history,
            world: &crate::own_lines::OnADevice
                as &dyn suzatary_lines::secretary::lines_rule::World,
        },
        &literals,
        &slot_keys,
        ada,
        &trusting,
    );
    for own in posted {
        if received
            .outcome
            .taken
            .iter()
            .any(|(name, _)| name == own.name())
        {
            for key in slot_keys.of(own.name()) {
                use suzatary_store::store::Doc as _;
                lines.keys.set(&key.name(), key).expect("kept");
            }
        }
    }
    lines.lines.accept(&received.outcome.taken).expect("taken");
    posted
        .iter()
        .map(|own| {
            let answer = match received
                .outcome
                .refused
                .iter()
                .find(|r| r.name.as_ref() == Some(own.name()))
            {
                Some(refused) => Answer::Refused {
                    kind: match refused.reason.kind() {
                        RefusalKind::Final | RefusalKind::Outdated => Refusal::Final,
                        RefusalKind::Trust | RefusalKind::Later => Refusal::Later,
                    },
                    reason: format!("{:?}", refused.reason),
                },
                None => Answer::Taken,
            };
            (own.name().clone(), answer)
        })
        .collect()
}

/// Whether the `Again` at `at` sets anything aside, as production reads it: a regenerate, or an
/// edit that `transcript::inert_edits` does not name.
fn active(read: &Transcript, at: usize) -> bool {
    use suzatary_conversations::secretary::store::Rewind;
    let lines = read.lines();
    match &lines[at].entry {
        Entry::Again {
            how: Rewind::Regenerate,
            ..
        } => true,
        Entry::Again {
            how: Rewind::Edit, ..
        } => {
            let parents: Vec<Vec<Hash>> = lines
                .iter()
                .map(|line| {
                    line.rests_on
                        .iter()
                        .map(|&p| lines[p].name.clone())
                        .collect()
                })
                .collect();
            let held: Vec<(&Hash, &LineEvent, &[Hash])> = lines
                .iter()
                .zip(&parents)
                .map(|(line, parents)| (&line.name, &line.line, parents.as_slice()))
                .collect();
            !transcript::inert_edits(&held).contains(&lines[at].name)
        }
        _ => false,
    }
}

/// Whether the question the `Again` named `again` asks again is a live turn of the box's fold.
fn question_live(read: &Transcript, again: &Hash) -> bool {
    let Some(Entry::Again { question, .. }) = read
        .lines()
        .iter()
        .find(|line| line.name == *again)
        .map(|line| &line.entry)
    else {
        return false;
    };
    read.turn_lines()
        .iter()
        .position(|turn| *turn == question)
        .is_some_and(|turn| read.stored().is_live(turn))
}

/// The box holds no half of an edit, and no edit completed by a question not sent as one: every
/// question an edit's `Again` names still stands, or an edit of it was taken up by its own question
/// (one of `edits`, which may itself have been edited since).
fn never_neither(
    read: &Transcript,
    edits: &BTreeSet<Hash>,
) -> Result<(), proptest::test_runner::TestCaseError> {
    use suzatary_conversations::secretary::store::Rewind;
    let names = read.turn_lines();
    let live = |name: &Hash| {
        names
            .iter()
            .position(|turn| *turn == name)
            .is_some_and(|turn| read.stored().is_live(turn))
    };
    let lines = read.lines();
    let edit_of = |at: usize| {
        if let Entry::Again {
            question,
            how: Rewind::Edit,
            ..
        } = &lines[at].entry
        {
            Some(question)
        } else {
            None
        }
    };
    for at in 0..lines.len() {
        let Some(question) = edit_of(at) else {
            continue;
        };
        let replaced = (0..lines.len()).any(|other| {
            edit_of(other) == Some(question)
                && lines
                    .iter()
                    .any(|later| later.rests_on.contains(&other) && edits.contains(&later.name))
        });
        proptest::prop_assert!(
            live(question) || replaced,
            "an edit set its question aside with nothing asked in its place"
        );
    }
    Ok(())
}

/// A live question of the conversation as `tab` reads it, the `k`th from the last (wrapping), by
/// its line, and the lines of every turn it draws.
fn a_question(tab: &mut HeldReplica, k: usize) -> Option<(Hash, Vec<Hash>)> {
    let Some(Whole::Read(Ok(read))) = tab.transcript(&DEFAULT) else {
        return None;
    };
    let names = read.turn_lines();
    let questions: Vec<usize> = read
        .stored()
        .live()
        .into_iter()
        .filter(|(_, said)| matches!(said.turn, Turn::Person { .. }))
        .map(|(turn, _)| turn)
        .collect();
    (!questions.is_empty()).then(|| {
        (
            names[questions[questions.len() - 1 - k % questions.len()]].clone(),
            names.into_iter().cloned().collect(),
        )
    })
}

/// A line `craft` over the box's latest edit `Again` that nothing takes up, resting on it alone
/// at its instant, sealed by the poster as no tab seals it; none where no such `Again` stands.
fn reusing(the_box: &Boxed, craft: Craft, n: u32) -> Option<(OwnLine, Actor)> {
    use suzatary_inbox::secretary::stamp::{Speaker, Stamp};
    let read = the_box.read();
    let lines = read.lines();
    let at = (0..lines.len()).rev().find(|&at| {
        matches!(
            lines[at].entry,
            Entry::Again {
                how: suzatary_conversations::secretary::store::Rewind::Edit,
                ..
            }
        ) && !active(&read, at)
    })?;
    let instant = lines[at].line.at();
    let poster = match craft {
        Craft::EditedByOther => "bob",
        Craft::Question | Craft::Stop | Craft::Act | Craft::Edited => "ada",
    };
    let by = Some(Stamp::Actor(Speaker::at(
        poster,
        suzatary_signatures::signatures::Seat::Device,
    )));
    let entry = match craft {
        Craft::Question | Craft::Edited | Craft::EditedByOther => Entry::Said {
            at: instant,
            turn: Turn::Person {
                text: format!("crafted {n}"),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by,
        },
        Craft::Stop => Entry::Stopped {
            at: instant,
            by: "ada".to_owned(),
        },
        Craft::Act => Entry::Said {
            at: instant,
            turn: Turn::Act {
                text: format!("Ada did {n}"),
                references: Vec::new(),
            },
            by,
        },
    };
    let genesis = the_box
        .lines
        .lines
        .objects()
        .expect("held")
        .into_iter()
        .find_map(|(name, object)| matches!(object, Envelope::Genesis(_)).then_some(name))?;
    let (actor, held) = (
        Actor::new(poster).expect("an actor"),
        u32::try_from(lines.len()).expect("few lines"),
    );
    let entropy = Counting(AtomicU8::new(
        200u8.wrapping_add(u8::try_from(n % 50).expect("small")),
    ));
    let write = match craft {
        Craft::Edited | Craft::EditedByOther => LineEvent::edited,
        Craft::Question | Craft::Stop | Craft::Act => LineEvent::write,
    };
    let (line, keys) = write(DEFAULT.clone(), actor.clone(), held, &entry, &entropy).ok()?;
    let change =
        prodrome_core::change::mk_change(genesis, vec![lines[at].name.clone()], line).ok()?;
    Some((
        OwnLine::sealed(&Envelope::Change(change), &keys).ok()?,
        actor,
    ))
}

/// What one case did: how many `Again` lines the box took, the box's answer to each crafted line,
/// each edit's `Again` the box took, by name, with the question it asks again, the box's answer to
/// each edit's question that arrived after its `Again` while that still read as nothing, and the
/// box's reading at the end.
#[derive(Debug, Default)]
struct Took {
    agains: usize,
    crafted: Vec<(Craft, Answer)>,
    edits: BTreeMap<Hash, Hash>,
    split: Vec<(Hash, Answer)>,
    read: Option<std::sync::Arc<Transcript>>,
}

/// Whether, in `read`, a live turn descends from the line `question` that the line `again` does
/// not rest on: the conversation moved on past what the `Again` saw.
fn moved_on(read: &Transcript, question: &Hash, again: &Hash) -> bool {
    let lines = read.lines();
    let at = |name: &Hash| lines.iter().position(|written| written.name == *name);
    let (Some(question), Some(again)) = (at(question), at(again)) else {
        return false;
    };
    let above = |from: usize| -> BTreeSet<usize> {
        let mut found = BTreeSet::new();
        let mut next = vec![from];
        while let Some(line) = next.pop() {
            if found.insert(line) {
                next.extend(&lines[line].rests_on);
            }
        }
        found
    };
    let seen = above(again);
    read.stored()
        .live()
        .into_iter()
        .filter_map(|(turn, _)| at(read.turn_lines()[turn]))
        .any(|turn| turn != question && !seen.contains(&turn) && above(turn).contains(&question))
}

#[expect(
    clippy::too_many_lines,
    reason = "one law: the tabs, the box, each step, and the readings after"
)]
fn asked(steps: &[Step]) -> Result<Took, proptest::test_runner::TestCaseError> {
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
    let mut tabs = [pulled(), pulled(), pulled()];
    // The names of the questions sealed as an edit's own: no other question sets anything aside.
    let mut edits: BTreeSet<Hash> = BTreeSet::new();
    let mut regenerates: BTreeSet<Hash> = BTreeSet::new();
    let ada = Actor::new("ada").expect("an actor");
    let entropy = Counting(AtomicU8::new(1));
    let fit = Fit {
        most: 1000,
        bytes: usize::MAX,
    };
    let mut the_box = Boxed {
        lines,
        live: Vec::new(),
        begun: Vec::new(),
        calls: BTreeMap::new(),
    };
    let mut took = Took::default();
    // Everything unsent in one post, as the tab hands it over; what refuses an `Again` over a
    // working claim is checked where no stop rides in the same post, which may end that claim.
    // `posted` handed over in one post, with every law of a post asserted; the box's answers.
    let post_lines = |the_box: &mut Boxed,
                      posted: Vec<OwnLine>,
                      took: &mut Took,
                      (edits, regenerates): (&BTreeSet<Hash>, &BTreeSet<Hash>),
                      poster: &Actor|
     -> Result<Vec<(Hash, Answer)>, proptest::test_runner::TestCaseError> {
        if posted.is_empty() {
            return Ok(Vec::new());
        }
        let working = the_box.working();
        let before = the_box.read();
        let settled: BTreeSet<Hash> = {
            let owed: BTreeSet<usize> = before.owed().iter().map(|(turn, _)| *turn).collect();
            before
                .stored()
                .live()
                .into_iter()
                .filter(|(turn, _)| !owed.contains(turn))
                .map(|(turn, _)| before.turn_lines()[turn].clone())
                .collect()
        };
        // A stop in the post ends the claim first only where the `Again` rests on it.
        let beneath = |own: &OwnLine| -> BTreeSet<Hash> {
            let mut found = BTreeSet::new();
            let mut next: Vec<Hash> = own.rests_on().to_vec();
            while let Some(name) = next.pop() {
                if found.insert(name.clone())
                    && let Some(parent) = posted.iter().find(|p| *p.name() == name)
                {
                    next.extend(parent.rests_on().iter().cloned());
                }
            }
            found
        };
        let stop = |own: &OwnLine| own.print().contains("event=Stopped(");
        let live_before: BTreeSet<Hash> = before
            .stored()
            .live()
            .into_iter()
            .map(|(turn, _)| before.turn_lines()[turn].clone())
            .collect();
        // The edits' `Again` lines the box took in an earlier post: their questions arrive split.
        let split = took.edits.clone();
        let answers = handed(&the_box.lines, &posted, poster);
        for (own, (_, answer)) in posted.iter().zip(&answers) {
            // An `Again` is drawn on the question it names; any other line on itself.
            let again = own.drawn_on() != own.name();
            if again && *answer == Answer::Taken {
                took.agains += 1;
                if !regenerates.contains(own.name()) {
                    took.edits
                        .insert(own.name().clone(), own.drawn_on().clone());
                }
            }
            let stopped = beneath(own)
                .iter()
                .any(|name| posted.iter().any(|p| p.name() == name && stop(p)));
            // The second half of an edit split across posts, while its `Again` still reads as
            // nothing (else it is a plain question), is refused while a round covers its question,
            // and once the conversation moved on past what the `Again` saw: a live turn descends
            // from the question that the `Again` does not rest on. A question ended since by a stop
            // that does not descend from it is not moved on from, and an edit of it is taken.
            let inert = |again: &Hash| {
                before
                    .lines()
                    .iter()
                    .position(|written| written.name == *again)
                    .is_some_and(|at| !active(&before, at))
            };
            if edits.contains(own.name())
                && let Some((again, question)) = own
                    .rests_on()
                    .iter()
                    .find_map(|parent| split.get_key_value(parent).filter(|_| inert(parent)))
            {
                let covered = !stopped && working.contains(question);
                let moved = live_before.contains(question) && moved_on(&before, question, again);
                took.split.push((own.name().clone(), answer.clone()));
                if covered || moved {
                    proptest::prop_assert!(
                        matches!(
                            answer,
                            Answer::Refused {
                                kind: Refusal::Final,
                                ..
                            }
                        ),
                        "an edit's question split from its Again, its question {}, was answered \
                         {answer:?}",
                        if covered { "covered" } else { "moved on from" }
                    );
                }
            }
            if again && !stopped && working.contains(own.drawn_on()) {
                proptest::prop_assert!(
                    matches!(
                        answer,
                        Answer::Refused {
                            kind: Refusal::Final,
                            ..
                        }
                    ),
                    "an Again over a working claim was answered {answer:?}"
                );
            }
        }
        // Asking again reopens no other question: what was settled before the post is owed after
        // only where an `Again` of the post taken by the box asks it again.
        let asked_again: BTreeSet<&Hash> = posted
            .iter()
            .zip(&answers)
            .filter(|(own, (_, answer))| own.drawn_on() != own.name() && *answer == Answer::Taken)
            .map(|(own, _)| own.drawn_on())
            .collect();
        let after = the_box.read();
        for (turn, _) in after.owed() {
            let name = &after.turn_lines()[turn];
            proptest::prop_assert!(
                !settled.contains(*name) || asked_again.contains(*name),
                "a post reopened a question it did not ask again"
            );
        }
        // A question the person did not send as an edit sets nothing aside: a turn live before the
        // post and set aside after it is set aside by a regenerate or an edit's own question.
        let asks_again = posted.iter().zip(&answers).any(|(own, (_, answer))| {
            *answer == Answer::Taken
                && (edits.contains(own.name()) || regenerates.contains(own.name()))
        });
        let live_after: BTreeSet<Hash> = after
            .stored()
            .live()
            .into_iter()
            .map(|(turn, _)| after.turn_lines()[turn].clone())
            .collect();
        proptest::prop_assert!(
            asks_again || live_before.is_subset(&live_after),
            "a post that asked nothing again set a turn aside"
        );
        never_neither(&after, edits)?;
        Ok(answers)
    };
    // What `tab` holds unsent, whole (as a hand-over posts it) or only its first line the box can
    // take (as a hand-over cut by size posts it), handed over and answered.
    let post = |the_box: &mut Boxed,
                tab: &mut HeldReplica,
                whole: bool,
                took: &mut Took,
                sets: (&BTreeSet<Hash>, &BTreeSet<Hash>)|
     -> Result<(), proptest::test_runner::TestCaseError> {
        let held: BTreeSet<Hash> = the_box
            .lines
            .lines
            .objects()
            .expect("held")
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        let unsent = tab
            .unsent(fit, |_| 1)
            .into_iter()
            .flatten()
            .filter(|own| matches!(own, OwnLine::Line { .. }))
            .cloned();
        let posted: Vec<OwnLine> = if whole {
            unsent.collect()
        } else {
            unsent
                .filter(|own| own.rests_on().iter().all(|parent| held.contains(parent)))
                .take(1)
                .collect()
        };
        let answers = post_lines(the_box, posted, took, sets, &ada)?;
        let _ = tab.answered(answers);
        Ok(())
    };
    for (n, step) in steps.iter().enumerate() {
        let second = 1 + u32::try_from(n).expect("few steps");
        match *step {
            Step::Say(t) => {
                let _ = tabs[t].seal(
                    Sealing::Say {
                        conversation: DEFAULT.clone(),
                        question: question(&format!("question {n}")),
                    },
                    at(second),
                    &ada,
                    &entropy,
                );
            }
            Step::Stop(t) => {
                let _ = tabs[t].seal(
                    Sealing::Stop {
                        conversation: DEFAULT.clone(),
                    },
                    at(second),
                    &ada,
                    &entropy,
                );
            }
            Step::Again(t, how, pick) => {
                let Some((asked, seen)) = a_question(&mut tabs[t], pick) else {
                    continue;
                };
                let how = match how {
                    How::Regenerate => AskedAgain::Regenerate,
                    How::Edit => AskedAgain::Edit(question(&format!("edited {n}"))),
                    How::EditRefused => AskedAgain::Edit(question("")),
                };
                let regenerate = matches!(how, AskedAgain::Regenerate);
                if let Ok((owned, name, _)) = tabs[t].seal(
                    Sealing::Again {
                        conversation: DEFAULT.clone(),
                        question: asked.clone(),
                        how,
                        seen,
                    },
                    at(second),
                    &ada,
                    &entropy,
                ) {
                    if regenerate {
                        regenerates.insert(name.clone());
                    }
                    edits.extend(
                        owned
                            .iter()
                            .filter(|own| own.node.rests_on().contains(&name))
                            .map(|own| own.node.name().clone()),
                    );
                    // Its ruling is drawn on the question it asks again.
                    let rulings = tabs[t].rulings(&DEFAULT);
                    let drawn = rulings
                        .iter()
                        .find(|(own, _)| *own.name() == name)
                        .map(|(own, _)| own.drawn_on().clone());
                    proptest::prop_assert_eq!(drawn, Some(asked));
                }
            }
            Step::Post(t) => post(
                &mut the_box,
                &mut tabs[t],
                true,
                &mut took,
                (&edits, &regenerates),
            )?,
            Step::PostOne(t) => post(
                &mut the_box,
                &mut tabs[t],
                false,
                &mut took,
                (&edits, &regenerates),
            )?,
            Step::Reuse(craft) => {
                let Some((crafted, poster)) = reusing(&the_box, craft, second) else {
                    continue;
                };
                let own_edit = matches!(craft, Craft::Edited);
                if own_edit {
                    // By its kind it is the edit's own question, sent by the edit's writer.
                    edits.insert(crafted.name().clone());
                }
                let edit = crafted.rests_on()[0].clone();
                let live_before = question_live(&the_box.read(), &edit);
                let answers = post_lines(
                    &mut the_box,
                    vec![crafted],
                    &mut took,
                    (&edits, &regenerates),
                    &poster,
                )?;
                took.crafted
                    .extend(answers.iter().map(|(_, answer)| (craft, answer.clone())));
                let taken = answers.iter().all(|(_, answer)| *answer == Answer::Taken);
                // As the box's fold reads it: the edit's question leaves the live turns exactly
                // when its writer's own `Edited` is taken.
                proptest::prop_assert_eq!(
                    question_live(&the_box.read(), &edit),
                    live_before && !(own_edit && taken),
                    "a {:?} at a dangling edit's instant, answered {:?}",
                    craft,
                    answers
                );
                if matches!(craft, Craft::Act | Craft::EditedByOther) {
                    proptest::prop_assert!(
                        answers.iter().all(|(_, answer)| matches!(
                            answer,
                            Answer::Refused {
                                kind: Refusal::Final,
                                ..
                            }
                        )),
                        "a {craft:?} was answered {answers:?}"
                    );
                }
            }
            Step::Land(t) => land(&the_box.lines, &mut tabs[t], u64::from(second)),
            Step::Claim => {
                let held = the_box.lines.runs.begin().expect("a run");
                let claimed = transcript::claim(
                    the_box.lines.conversations(),
                    &DEFAULT,
                    &held.run,
                    at(second),
                    Retry::Allowed,
                    &|_| true,
                    |_| vec![closing(second, "The box closed this unanswered.")],
                )
                .expect("claims");
                if claimed == transcript::Claimed::Began {
                    let run = held.run.clone();
                    the_box.begun.push(run.clone());
                    the_box.live.push(held);
                    the_box.sent(&run)?;
                }
            }
            Step::Join(k) if !the_box.live.is_empty() => {
                let run = the_box.live[k % the_box.live.len()].run.clone();
                transcript::join(
                    the_box.lines.conversations(),
                    &DEFAULT,
                    &run,
                    at(second),
                    &|_| true,
                )
                .expect("joins");
                the_box.sent(&run)?;
            }
            Step::Answer(k) if !the_box.live.is_empty() => {
                let held = the_box.live.remove(k % the_box.live.len());
                transcript::push_open(
                    the_box.lines.conversations(),
                    &DEFAULT,
                    &held.run,
                    &[closing(second, "Here it is.")],
                )
                .expect("answers");
            }
            Step::Die(k) if !the_box.live.is_empty() => {
                drop(the_box.live.remove(k % the_box.live.len()));
            }
            Step::Join(_) | Step::Answer(_) | Step::Die(_) => {}
        }
    }
    // Every tab hands over what it holds and lands what the box holds, until nothing moves.
    for round in 0..3u64 {
        for tab in &mut tabs {
            post(&mut the_box, tab, true, &mut took, (&edits, &regenerates))?;
        }
        for tab in &mut tabs {
            land(&the_box.lines, tab, 100 + round);
        }
    }
    let boxed = the_box.read();
    for tab in &mut tabs {
        let read = match tab.transcript(&DEFAULT) {
            Some(Whole::Read(Ok(read))) => read,
            other => {
                return Err(proptest::test_runner::TestCaseError::fail(format!(
                    "the tab reads {other:?}"
                )));
            }
        };
        proptest::prop_assert_eq!(read.stored(), boxed.stored());
        proptest::prop_assert_eq!(read.state(), boxed.state());
    }
    took.read = Some(boxed);
    Ok(took)
}

/// The generator's share: most cases reach an `Again` the box takes, so the laws above are drawn
/// over rewinds and not over questions alone. Printed, and held above one in two.
#[test]
fn most_cases_take_an_again() {
    use proptest::strategy::{Strategy as _, ValueTree as _};
    let mut runner = proptest::test_runner::TestRunner::deterministic();
    let cases = 200;
    let took = (0..cases)
        .filter(|_| {
            let steps = steps().new_tree(&mut runner).expect("a case").current();
            asked(&steps).expect("the laws hold").agains > 0
        })
        .count();
    println!("{took} of {cases} cases take an Again");
    assert!(took * 2 > cases, "{took} of {cases} cases take an Again");
}

/// An edit whose new question the box refuses leaves the old question and its answer standing:
/// the box takes the `Again` alone, which reads as nothing.
#[test]
fn an_edit_whose_question_is_refused_leaves_the_old_one_standing() {
    use Step::{Again, Answer, Claim, Land, Post, Say};
    asked(&[
        Say(0),
        Post(0),
        Claim,
        Answer(0),
        Land(0),
        Again(0, How::EditRefused, 0),
        Post(0),
    ])
    .expect("the laws hold");
}

/// The review's order: an edit whose question the box refuses leaves its `Again` dangling; the next
/// question, of the same tab or of another device that pulled the `Again`, rests on it and is a
/// plain question: the old question and its answer stand.
#[test]
fn a_question_over_a_dangling_edit_completes_nothing() {
    use Step::{Again, Answer, Claim, Land, Post, Say};
    asked(&[
        Say(0),
        Post(0),
        Claim,
        Answer(0),
        Land(0),
        Land(2),
        Again(0, How::EditRefused, 0),
        Post(0),
        Land(2),
        Say(2),
        Post(2),
        Land(0),
        Say(0),
        Post(0),
    ])
    .expect("the laws hold");
}

/// The same tab's next question after its edit was refused, with nothing landed between: it rests
/// on the dangling `Again` alone, at a later instant, and is a plain question.
#[test]
fn the_same_tab_s_next_question_over_its_refused_edit_is_plain() {
    use Step::{Again, Answer, Claim, Land, Post, Say};
    asked(&[
        Say(0),
        Post(0),
        Claim,
        Answer(0),
        Land(0),
        Again(0, How::EditRefused, 0),
        Post(0),
        Say(0),
        Post(0),
    ])
    .expect("the laws hold");
}

/// The review's order: a question queued, an edit of it whose question the box refused, then lines
/// another writer seals over the dangling `Again` alone, at its instant. Each is posted, and the
/// test names what answered it: a stop and a plain question (a second device whose clock repeats
/// the instant) are taken as what they are and take nothing up, an act is refused, since the box
/// alone writes a `Said` line other than a person's, and an edit's own question takes the edit up.
#[test]
fn a_line_at_a_dangling_edit_s_instant_is_judged_by_its_kind() {
    use Step::{Again, Land, Post, Reuse, Say};
    // The question still queued, so a stop over it has something to stop.
    for craft in [
        Craft::Stop,
        Craft::Act,
        Craft::Question,
        Craft::Edited,
        Craft::EditedByOther,
    ] {
        let took = asked(&[
            Say(1),
            Post(1),
            Land(0),
            Again(0, How::EditRefused, 0),
            Post(0),
            Reuse(craft),
        ])
        .expect("the laws hold");
        let [(posted, answer)] = &took.crafted[..] else {
            panic!("one crafted line posted for {craft:?}: {:?}", took.crafted);
        };
        let refused = |why: &str| matches!(answer, suzatary_store::replica::writes::Answer::Refused { kind: Refusal::Final, reason } if reason.contains(why));
        let read = took.read.as_ref().expect("the box's reading");
        let edit = &read
            .lines()
            .iter()
            .find(|written| matches!(written.entry, Entry::Again { .. }))
            .expect("the edit's Again")
            .name;
        match posted {
            Craft::Act => assert!(refused("the box alone writes a Said line"), "{answer:?}"),
            Craft::EditedByOther => {
                assert!(refused("this edit is someone else's"), "{answer:?}");
                assert!(
                    question_live(read, edit),
                    "another actor's question took the edit up"
                );
            }
            Craft::Stop | Craft::Question => {
                assert_eq!(*answer, Answer::Taken);
                assert!(question_live(read, edit), "a {posted:?} took the edit up");
            }
            Craft::Edited => {
                assert_eq!(*answer, Answer::Taken);
                assert!(
                    !question_live(read, edit),
                    "the edit's own question took it up"
                );
            }
        }
    }
}

/// The runners' order: tab 2 posts a question and edits it, while tab 0 seals a question and a
/// stop it has not posted. Tab 2's `Again` alone is taken, the box claims a round over the
/// question, and tab 0's post then lands its stop, which ends that round: the question is stopped,
/// never answered, and nothing descends from it that the `Again` did not see. So the edit's own
/// question, arriving last, is taken: it sets the stopped question aside and is owed in its place,
/// as an edit after "stop it first" is.
#[test]
fn an_edit_s_question_after_a_stop_ended_its_round_is_taken() {
    use Step::{Again, Answer, Claim, Land, Post, PostOne, Say, Stop};
    let took = asked(&[
        Say(0),
        Post(0),
        Claim,
        Answer(0),
        Land(0),
        Land(1),
        Land(2),
        Say(0),
        Say(2),
        Post(2),
        Again(2, How::Edit, 0),
        Stop(0),
        PostOne(2),
        Claim,
    ])
    .expect("the laws hold");
    let [(edited, answer)] = &took.split[..] else {
        panic!("one split edit's question: {:?}", took.split);
    };
    assert_eq!(*answer, suzatary_store::replica::writes::Answer::Taken);
    let [question] = &took.edits.values().collect::<Vec<_>>()[..] else {
        panic!("one edit taken: {:?}", took.edits);
    };
    let read = took.read.expect("the box's reading");
    let live: BTreeSet<&Hash> = read
        .stored()
        .live()
        .into_iter()
        .map(|(turn, _)| read.turn_lines()[turn])
        .collect();
    let owed: BTreeSet<&Hash> = read
        .owed()
        .iter()
        .map(|(turn, _)| read.turn_lines()[*turn])
        .collect();
    assert!(
        !live.contains(question),
        "the stopped question is set aside"
    );
    assert!(owed.contains(edited), "the edit's question is owed");
}

/// Two questions stopped before any round took them, then a regenerate of the second: setting the
/// stop aside would leave the first owed again, which nobody asked, so the rule refuses it.
#[test]
fn a_regenerate_that_would_reopen_a_stopped_question_is_refused() {
    use Step::{Again, Land, Post, Say, Stop};
    asked(&[
        Say(0),
        Say(0),
        Stop(0),
        Post(0),
        Land(0),
        Again(0, How::Regenerate, 0),
        Post(0),
    ])
    .expect("the laws hold");
}

/// The case the law found: a question sent by two runs (the first died, the second retried it),
/// then a second question and a stop that ends that round, then a regenerate of the second
/// question, which sets the stop aside. Withdrawing the stop's round would send the first question
/// a third time, so the rule refuses the regenerate.
#[test]
fn an_again_that_would_reopen_another_question_is_refused() {
    use How::Regenerate;
    use Step::{Again, Claim, Die, Land, Post, Say, Stop};
    asked(&[
        Say(1),
        Post(1),
        Land(1),
        Claim,
        Again(0, Regenerate, 0),
        Say(0),
        Die(0),
        Claim,
        Say(1),
        Stop(1),
        Again(1, Regenerate, 0),
        Post(1),
        Claim,
    ])
    .expect("the laws hold");
}

/// A session the box kept between an edit's `Again` and its question, which reach the box in two
/// posts, is dropped with the edit: the edit takes effect where its question stands, so no session
/// that saw the old question and its answer is resumed after it.
#[test]
fn a_session_kept_between_an_edit_s_two_posts_is_dropped_with_it() {
    let lines = InMemory::default();
    let opened = Entry::Opened {
        at: at(0),
        title: Words::kept("Secretary".to_owned()),
        model: "a-cli/small".to_owned(),
        kind: Kind::Default,
    };
    transcript::start(lines.conversations(), &DEFAULT, &opened).expect("opened");
    let ada = Actor::new("ada").expect("an actor");
    let entropy = Counting(AtomicU8::new(1));
    let mut tab =
        HeldReplica::pulled(|_| Ok::<_, NotAnAnswer>(skeleton(&lines))).expect("the tab pulled");
    let (asked, _, _) = tab
        .seal(
            Sealing::Say {
                conversation: DEFAULT.clone(),
                question: question("what changed"),
            },
            at(1),
            &ada,
            &entropy,
        )
        .expect("sealed");
    let _ = handed(&lines, &[asked[0].node.clone()], &ada);
    transcript::push(lines.conversations(), &DEFAULT, &closing(2, "Here it is."))
        .expect("answered");
    land(&lines, &mut tab, 1);
    let question_line = asked[0].node.name().clone();
    let seen = tab_lines(&mut tab);
    let (owned, again, _) = tab
        .seal(
            Sealing::Again {
                conversation: DEFAULT.clone(),
                question: question_line.clone(),
                how: AskedAgain::Edit(question("what changed today")),
                seen,
            },
            at(3),
            &ada,
            &entropy,
        )
        .expect("sealed");
    let (first, second): (Vec<_>, Vec<_>) = owned
        .iter()
        .map(|own| own.node.clone())
        .filter(|own| own.drawn_on() != own.name() || own.rests_on().contains(&again))
        .partition(|own| *own.name() == again);
    let _ = handed(&lines, &first, &ada);
    transcript::push(
        lines.conversations(),
        &DEFAULT,
        &Entry::Kept {
            at: at(4),
            session: suzatary_conversations::secretary::model::SessionId::minted("default/kept"),
            model: "a-cli/small".to_owned(),
            cwd: String::new(),
            surface: None,
            from: 0,
        },
    )
    .expect("kept");
    let _ = handed(&lines, &second, &ada);
    let read = transcript::read(lines.conversations(), &DEFAULT).expect("reads");
    assert!(
        read.stored().session.is_none(),
        "{:?}",
        read.stored().session
    );
}

/// The lines of every turn `tab` draws of the conversation.
fn tab_lines(tab: &mut HeldReplica) -> Vec<Hash> {
    let Some(Whole::Read(Ok(read))) = tab.transcript(&DEFAULT) else {
        return Vec::new();
    };
    read.turn_lines().into_iter().cloned().collect()
}

/// What `tab` holds unsent, handed over in one post and answered.
fn hand_over(lines: &InMemory, tab: &mut HeldReplica, ada: &Actor) -> Vec<(Hash, Answer)> {
    let fit = Fit {
        most: 1000,
        bytes: usize::MAX,
    };
    let posted: Vec<OwnLine> = tab
        .unsent(fit, |_| 1)
        .into_iter()
        .flatten()
        .filter(|own| matches!(own, OwnLine::Line { .. }))
        .cloned()
        .collect();
    let answers = handed(lines, &posted, ada);
    let _ = tab.answered(answers.clone());
    answers
}

/// `tab` edits the last live question as it reads it, at `second`, its entropy from `byte`: the
/// name of the edit's own question.
fn edit(tab: &mut HeldReplica, ada: &Actor, second: u32, byte: u8) -> Hash {
    let (asked, seen) = a_question(tab, 0).expect("a live question");
    let (owned, again, _) = tab
        .seal(
            Sealing::Again {
                conversation: DEFAULT.clone(),
                question: asked,
                how: AskedAgain::Edit(question(&format!("edited at {second}"))),
                seen,
            },
            at(second),
            ada,
            &Counting(AtomicU8::new(byte)),
        )
        .expect("sealed");
    owned
        .iter()
        .find(|own| own.node.rests_on().contains(&again))
        .map(|own| own.node.name().clone())
        .expect("the edit's own question")
}

/// The words `tab` gives back for its refused line `name`.
fn given_back(tab: &HeldReplica, name: &Hash) -> suzatary_lines::secretary::own_line::Words {
    use suzatary_lines::secretary::held_replica::Ruling;
    tab.rulings(&DEFAULT)
        .into_iter()
        .find_map(|(own, ruling)| match ruling {
            Ruling::Refused { words, .. } if own.name() == name => Some(words),
            Ruling::Refused { .. } | Ruling::Unsent | Ruling::Trust | Ruling::Later { .. } => None,
        })
        .expect("a refused line with words")
}

/// A question the box took whose row has not landed is not gone: an edit of it the box refuses,
/// since its answer is still coming, gives its words back as the same edit, so Send asks it again
/// in its place and does not add a second question beside it.
#[test]
fn a_refused_edit_of_a_question_whose_row_has_not_landed_comes_back_as_an_edit() {
    use suzatary_lines::secretary::own_line::Words as Given;
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
            Sealing::Say {
                conversation: DEFAULT.clone(),
                question: question("Is the zephyr permit due?"),
            },
            at(1),
            &ada,
            &Counting(AtomicU8::new(11)),
        )
        .expect("sealed");
    // The box takes the question; its answer has not reached the tab, which has landed nothing.
    let fit = Fit {
        most: 1000,
        bytes: usize::MAX,
    };
    let in_flight: Vec<OwnLine> = tab
        .unsent(fit, |_| 1)
        .into_iter()
        .flatten()
        .filter(|own| matches!(own, OwnLine::Line { .. }))
        .cloned()
        .collect();
    let first = handed(&lines, &in_flight, &ada);
    assert!(first.iter().all(|(_, answer)| *answer == Answer::Taken));
    // A round covers the question.
    let held = lines.runs.begin().expect("a run");
    let claimed = transcript::claim(
        lines.conversations(),
        &DEFAULT,
        &held.run,
        at(2),
        Retry::Allowed,
        &|_| true,
        |_| vec![closing(2, "The box closed this unanswered.")],
    )
    .expect("claims");
    assert_eq!(claimed, transcript::Claimed::Began);
    // The page drew the question as the tab sealed it, and the person edits it.
    let (owned, again, _) = tab
        .seal(
            Sealing::Again {
                conversation: DEFAULT.clone(),
                question: asked.clone(),
                how: AskedAgain::Edit(question("Is the zephyr permit due this week?")),
                seen: vec![asked.clone()],
            },
            at(3),
            &ada,
            &Counting(AtomicU8::new(41)),
        )
        .expect("sealed");
    let edited = owned
        .iter()
        .find(|own| own.node.rests_on().contains(&again))
        .map(|own| own.node.name().clone())
        .expect("the edit's own question");
    let _ = tab.answered(first);
    let answers = hand_over(&lines, &mut tab, &ada);
    assert!(!tab.holds(&asked), "the question's row has not landed");
    assert!(
        answers.iter().all(|(_, answer)| matches!(
            answer,
            Answer::Refused {
                kind: Refusal::Final,
                ..
            }
        )),
        "{answers:?}"
    );
    assert!(
        matches!(given_back(&tab, &edited), Given::Edit { question, .. } if question == asked),
        "{:?}",
        given_back(&tab, &edited)
    );
    drop(held);
}

/// A question the rows hold that an edit elsewhere set aside is gone: this tab's own edit of it,
/// refused, gives its words back as the edit while the tab still reads the question in a round,
/// and as a plain question once it lands the rows that set it aside.
#[test]
fn a_refused_edit_of_a_question_set_aside_since_comes_back_plain() {
    use suzatary_lines::secretary::own_line::Words as Given;
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
    let (mut tab, mut other) = (pulled(), pulled());
    let ada = Actor::new("ada").expect("an actor");
    let (_, asked, _) = tab
        .seal(
            Sealing::Say {
                conversation: DEFAULT.clone(),
                question: question("Is the zephyr permit due?"),
            },
            at(1),
            &ada,
            &Counting(AtomicU8::new(11)),
        )
        .expect("sealed");
    hand_over(&lines, &mut tab, &ada);
    land(&lines, &mut tab, 1);
    land(&lines, &mut other, 1);
    let theirs = edit(&mut other, &ada, 2, 23);
    assert!(
        hand_over(&lines, &mut other, &ada)
            .iter()
            .any(|(name, answer)| *name == theirs && *answer == Answer::Taken)
    );
    let mine = edit(&mut tab, &ada, 3, 41);
    let answers = hand_over(&lines, &mut tab, &ada);
    assert!(
        answers.iter().all(|(_, answer)| matches!(
            answer,
            Answer::Refused {
                kind: Refusal::Final,
                ..
            }
        )),
        "{answers:?}"
    );
    assert!(
        matches!(given_back(&tab, &mine), Given::Edit { question, .. } if question == asked),
        "before the rows that set it aside land: {:?}",
        given_back(&tab, &mine)
    );
    land(&lines, &mut tab, 2);
    assert!(tab.holds(&asked));
    assert!(
        matches!(given_back(&tab, &mine), Given::Question { .. }),
        "after: {:?}",
        given_back(&tab, &mine)
    );
}
