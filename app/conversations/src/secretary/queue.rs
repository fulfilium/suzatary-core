//! The transcript is the queue: what is still undelivered is derived from it, never kept beside
//! it, so "a queued message is never sent twice" is a property of a derivation, not a lock. For
//! one writer it is the owed register (`transcript::Transcript::owed`), which a law holds equal.

use crate::secretary::model::Turn;
use std::collections::BTreeSet;

use crate::secretary::store::{Said, Scope, Stored};
use suzatary_inbox::secretary::stamp::Stamp;

/// How many `say`s one conversation may hold unanswered at once; past it `say` refuses rather
/// than grow the conversation forever.
pub const MAX_PENDING: usize = 16;

/// What waits to be answered: every live turn after the last one that closes a round, with its
/// index.
///
/// A round is closed exactly by a turn that [closes](Stored::closes) it, so a stopped question is
/// never owed again. A view, never a mark: once a round's own turn is appended the boundary moves
/// past what it read.
#[must_use]
pub fn delta(stored: &Stored) -> Vec<(usize, &Said)> {
    let live = stored.live();
    let boundary = live.iter().rposition(|(index, _)| stored.closes(*index));
    match boundary {
        Some(at) => live[at + 1..].to_vec(),
        None => live,
    }
}

/// Whether `said` starts a round: anything but an act line the gate did not stamp as the
/// owner's; the box's lines are sent with the next round, and wake no model alone.
#[must_use]
pub fn wakes(said: &Said) -> bool {
    match said.turn {
        Turn::Act { .. } => Stamp::owners(said.by.as_ref()),
        Turn::Person { .. } | Turn::Model { .. } | Turn::Tool { .. } | Turn::Erased { .. } => true,
    }
}

/// Whether a round is owed among `waiting`, a round's turns in order: some turn there [`wakes`].
#[must_use]
pub fn waits(waiting: &[(usize, &Said)]) -> bool {
    waiting.iter().any(|(_, said)| wakes(said))
}

/// How many of its own live turns before the round being answered a call is sent, at most. The
/// conversation page keeps its own count (`readings::DEFAULT_TURN_WINDOW`), which is not this.
pub const EARLIER_TURNS: usize = 60;

/// How many bytes of its own turns before the round being answered a call is sent, at most.
/// Beside the verbs (about 60 KB), the record (128 KiB) and the facts, under half the model's
/// 200,000 tokens; the round being answered has the rest.
pub const EARLIER_BYTES: usize = 65_536;

/// A verb's answer in an earlier round over this many stored bytes is sent and weighed as its
/// marker, so the round that read it stays in the window.
pub const LONG_ANSWER: usize = 4_096;

/// What a marker weighs: above the largest it prints. A long answer's is `VERB {verb}:\n[an
/// answer of {n} bytes given in an earlier round and not sent again: calling {verb} with {args}
/// answers it again]\n\n`: 108 bytes of its own words, two verb names of at most 24 bytes, a
/// count of at most 20 digits, and the arguments cut at 160 bytes with 18 more saying so: 354.
pub const MARKER_BYTES: usize = 384;

/// What a call is sent of its own conversation: the round being answered, whole, after the
/// earlier rounds [`Steps`] keeps, a function of the lines before the round's opener alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window<'s> {
    /// Whole rounds before the one being answered.
    pub earlier: Vec<(usize, &'s Said)>,
    /// The round being answered, whole, ending in the newest turn.
    pub round: Vec<(usize, &'s Said)>,
}

impl<'s> Window<'s> {
    /// Every turn of the window, oldest first.
    pub fn all(&self) -> impl Iterator<Item = &(usize, &'s Said)> {
        self.earlier.iter().chain(&self.round)
    }

    /// The index of its first turn; none for an empty conversation.
    #[must_use]
    pub fn first(&self) -> Option<usize> {
        self.all().next().map(|(index, _)| *index)
    }
}

/// The window's fold over the live rounds: each joins the earlier ones, and one that takes them
/// over [`EARLIER_TURNS`] or [`EARLIER_BYTES`] drops the oldest to half of both, never the round
/// just closed while it fits both alone, so the window's start moves in steps.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Steps {
    earlier: std::collections::VecDeque<Weighed>,
    turns: usize,
    bytes: usize,
    open: Option<Weighed>,
}

/// One round: the index of its first turn, and its turns' count and weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Weighed {
    from: usize,
    turns: usize,
    bytes: usize,
}

impl Steps {
    /// Turn `index`, weighing `weight` in the window, which opens a round or not.
    pub fn turn(&mut self, index: usize, opens: bool, weight: usize) {
        if opens && let Some(done) = self.open.take() {
            self.fold(done);
        }
        let open = self.open.get_or_insert(Weighed {
            from: index,
            turns: 0,
            bytes: 0,
        });
        open.turns += 1;
        open.bytes += weight;
    }

    fn fold(&mut self, round: Weighed) {
        self.earlier.push_back(round);
        self.turns += round.turns;
        self.bytes += round.bytes;
        if self.turns > EARLIER_TURNS || self.bytes > EARLIER_BYTES {
            while self.turns > EARLIER_TURNS / 2 || self.bytes > EARLIER_BYTES / 2 {
                // The round just closed stays while it fits the full bounds alone: a follow-up
                // to a long round is sent what it follows.
                let alone = self.earlier.len() == 1;
                if alone && self.turns <= EARLIER_TURNS && self.bytes <= EARLIER_BYTES {
                    break;
                }
                let Some(oldest) = self.earlier.pop_front() else {
                    break;
                };
                self.turns -= oldest.turns;
                self.bytes -= oldest.bytes;
            }
        }
    }

    /// The first turn of the window, and the first of the round being answered.
    fn starts(&self) -> Option<(usize, usize)> {
        let open = self.open?.from;
        Some((self.earlier.front().map_or(open, |first| first.from), open))
    }
}

/// The window of `stored`, less the turns `leaving_out` names (those no round has claimed).
#[must_use]
pub fn window<'s>(stored: &'s Stored, leaving_out: &BTreeSet<usize>) -> Window<'s> {
    let Some((from, round)) = stored.steps.starts() else {
        return Window {
            earlier: Vec::new(),
            round: Vec::new(),
        };
    };
    let live = |range: std::ops::Range<usize>| -> Vec<(usize, &'s Said)> {
        range
            .filter(|index| stored.is_live(*index) && !leaving_out.contains(index))
            .map(|index| (index, &stored.turns[index]))
            .collect()
    };
    Window {
        earlier: live(from..round),
        round: live(round..stored.turns.len()),
    }
}

/// What turn `index` of `stored` weighs in an earlier round.
#[must_use]
pub fn weight(stored: &Stored, index: usize) -> usize {
    if long_answer(stored, index) {
        MARKER_BYTES
    } else {
        stored.carried[index].bytes
    }
}

/// Whether turn `index` of `stored` is a verb's answer over [`LONG_ANSWER`] stored bytes, which
/// an earlier round sends as its marker: by its shape, so one whose words are gone is one too.
#[must_use]
pub fn long_answer(stored: &Stored, index: usize) -> bool {
    matches!(
        crate::secretary::turn_shape::shape(&stored.turns[index].turn),
        crate::secretary::turn_shape::TurnShape::Tool { .. }
    ) && stored.carried[index].bytes > LONG_ANSWER
}

/// Every turn a model asked as `stored` records holds now: every stored turn, or the window with
/// nothing left out, which holds what any ask of the round being answered was sent.
#[must_use]
pub fn held(stored: &Stored) -> Vec<(usize, &Said)> {
    match stored.scope {
        Scope::Transcript => stored.turns.iter().enumerate().collect(),
        Scope::Window => window(stored, &BTreeSet::new()).all().copied().collect(),
    }
}

/// What a turn with no stored line weighs: its words, and a file by its marker.
#[must_use]
pub fn bytes(turn: &Turn) -> usize {
    const MARKER: usize = 160;
    match turn {
        Turn::Person {
            text, attachments, ..
        } => {
            text.len()
                + attachments
                    .iter()
                    .map(|one| MARKER + one.private.as_ref().map_or(0, |copy| copy.text.len()))
                    .sum::<usize>()
        }
        Turn::Model { text, calls } => {
            text.len()
                + calls
                    .iter()
                    .map(|call| {
                        call.verb.as_str().len()
                            + serde_json::to_string(&call.args).map_or(0, |args| args.len())
                    })
                    .sum::<usize>()
        }
        Turn::Tool { answer, files, .. } => answer.len() + MARKER * files.len(),
        Turn::Act { text, .. } => text.len(),
        Turn::Erased { words, .. } => crate::secretary::turn_shape::prose(words).len(),
    }
}

#[must_use]
pub fn pending(stored: &Stored) -> usize {
    delta(stored).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::store::Entry;
    use suzatary_base::conversation_id::ConversationId;

    fn id() -> ConversationId {
        ConversationId::new("2026-09-13-1").expect("a slug")
    }

    fn opened() -> Entry {
        Entry::Opened {
            at: suzatary_base::capability::Datetime::new(2026, 9, 13, 9, 0, 0, 0)
                .expect("an instant"),
            title: suzatary_memory::secretary::words::Words::kept("Morning".to_owned()),
            model: "a-cli/small".to_owned(),
            kind: crate::secretary::store::Kind::Topic,
        }
    }

    fn said(at: &str, turn: Turn) -> Entry {
        Entry::Said {
            at: crate::secretary::store::instant(at),
            turn,
            by: None,
        }
    }

    fn person(text: &str) -> Turn {
        Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        }
    }

    fn finished_model(text: &str) -> Turn {
        Turn::Model {
            text: text.to_owned(),
            calls: Vec::new(),
        }
    }

    fn calling_model(text: &str, verb: &str) -> Turn {
        Turn::Model {
            text: text.to_owned(),
            calls: vec![crate::secretary::model::ToolCall {
                verb: verb.into(),
                args: std::collections::BTreeMap::new(),
            }],
        }
    }

    fn stored(entries: &[Entry]) -> Stored {
        Stored::of(&id(), entries).expect("a conversation")
    }

    fn turns<'a>(delta: &[(usize, &'a Said)]) -> Vec<&'a Turn> {
        delta.iter().map(|(_, said)| &said.turn).collect()
    }

    /// The box's own notes wake no model, and the owner's next turn is sent with them, in order.
    #[test]
    fn the_boxs_notes_wait_for_the_owners_turn() {
        let noted = |at: &str, text: &str| {
            Entry::noted(
                crate::secretary::store::instant(at),
                text.to_owned(),
                Vec::new(),
            )
        };
        let mut lines = vec![
            opened(),
            noted("2026-09-13T09:01:00", "The box asks: a key."),
            noted("2026-09-13T09:02:00", "The box asks: a link."),
        ];
        assert!(!waits(&delta(&stored(&lines))), "nothing wakes");
        let mut owner = said("2026-09-13T09:03:00", person("add my Work calendar"));
        if let Entry::Said { by, .. } = &mut owner {
            *by = Some(Stamp::Actor(
                suzatary_inbox::secretary::stamp::Speaker::device("ada"),
            ));
        }
        lines.push(owner);
        let waiting = stored(&lines);
        let owed = delta(&waiting);
        assert!(waits(&owed), "the owner's turn wakes");
        assert_eq!(owed.len(), 3, "the notes ride along");
        let dismissed = Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 13, 9, 4, 0, 0)
                .expect("an instant"),
            turn: Turn::Act {
                text: "Ada dismissed it: not now".to_owned(),
                references: Vec::new(),
            },
            by: Some(Stamp::Actor(
                suzatary_inbox::secretary::stamp::Speaker::device("ada"),
            )),
        };
        assert!(
            waits(&delta(&stored(&[opened(), dismissed]))),
            "the owner's own act still wakes"
        );
    }

    fn stop(at: &str) -> Entry {
        Entry::Stopped {
            at: crate::secretary::store::instant(at),
            by: "ada".to_owned(),
        }
    }

    /// A stop then an empty end reads as nothing queued, and the empty end opens no round of its
    /// own.
    #[test]
    fn a_stored_stop_with_its_empty_end_reads_as_it_did() {
        let lines = [
            opened(),
            said("t1", person("p1")),
            said("t2", finished_model("one")),
            said("t3", person("p2")),
            stop("t4"),
            said("t4", finished_model("")),
            said("t5", person("p3")),
            said("t6", finished_model("three")),
        ];
        let stored = stored(&lines);
        assert!(delta(&stored).is_empty());
        assert!(delta(&Stored::of(&id(), &lines[..6]).expect("a conversation")).is_empty());
        let window = window(&stored, &BTreeSet::new());
        assert_eq!(
            turns(&window.round),
            vec![&person("p3"), &finished_model("three")],
            "the empty end belongs to its stop, so the last round starts after it"
        );
        assert_eq!(window.earlier.len(), 5, "two whole rounds before it");
    }

    /// A round too long for any model ends in the box's words, and the next round is sent
    /// without it: the window keeps only whole rounds within its bytes.
    #[test]
    fn a_round_too_long_alone_is_not_sent_with_the_next() {
        let huge = Turn::Person {
            text: "x".repeat(EARLIER_BYTES * 4),
            references: Vec::new(),
            attachments: Vec::new(),
        };
        let mut lines = vec![
            opened(),
            said("t1", person("p1")),
            said("t2", finished_model("one")),
            said("t3", huge.clone()),
        ];
        let asked = stored(&lines);
        assert_eq!(
            turns(&window(&asked, &BTreeSet::new()).round),
            vec![&huge],
            "the round being answered is sent whole"
        );
        lines.extend([
            said(
                "t4",
                finished_model("I could not finish: this message is too long."),
            ),
            said("t5", person("p2")),
        ]);
        let next = stored(&lines);
        let window = window(&next, &BTreeSet::new());
        assert_eq!(turns(&window.round), vec![&person("p2")]);
        assert!(
            window.earlier.is_empty(),
            "the long round, and every round before it, is left to the record"
        );
    }

    /// The window's start moves in steps: it stays while the earlier rounds grow, and a round
    /// that takes them over their bounds drops the oldest to half of them.
    #[test]
    fn the_windows_start_moves_in_steps() {
        let mut lines = vec![opened()];
        let mut starts = Vec::new();
        for k in 0..80 {
            lines.push(said(&format!("t{}", 2 * k), person(&format!("q{k}"))));
            starts.push(window(&stored(&lines), &BTreeSet::new()).first());
            lines.push(said(&format!("t{}", 2 * k + 1), finished_model("a")));
        }
        // Two turns a round: the start moves once in every half window, sixteen rounds.
        let moves = starts.windows(2).filter(|pair| pair[0] != pair[1]).count();
        assert_eq!(moves, 4, "{starts:?}");
        let all = stored(&lines);
        assert!(window(&all, &BTreeSet::new()).earlier.len() <= EARLIER_TURNS);
    }

    /// A round that read a long mail stays in the window with the secretary's answer to it: the
    /// long answer weighs a marker's bytes.
    #[test]
    fn a_long_answer_weighs_its_marker() {
        let lines = vec![
            opened(),
            said("t1", person("read me the mail")),
            said("t2", calling_model("", "show")),
            said(
                "t3",
                Turn::Tool {
                    verb: "show".into(),
                    answer: "m".repeat(EARLIER_BYTES * 2),
                    outcome: crate::secretary::outcome::Outcome::Answered,
                    files: Vec::new(),
                },
            ),
            said("t4", finished_model("she asks whether we agree")),
            said("t5", person("reply yes to her")),
        ];
        let stored = stored(&lines);
        let window = window(&stored, &BTreeSet::new());
        assert_eq!(
            window.earlier.len(),
            4,
            "the round that read the mail stays"
        );
        assert!(long_answer(&stored, 2));
    }

    /// One step of a simulated conversation; a stop comes in one line, or as stored conversations
    /// hold it, the stop then an empty end.
    #[derive(Debug, Clone, Copy)]
    enum Step {
        Person,
        Act,
        Call,
        Complete,
        Stop { stored_shape: bool },
    }

    fn steps() -> impl proptest::strategy::Strategy<Value = Vec<Step>> {
        use proptest::prelude::*;
        proptest::collection::vec(
            prop_oneof![
                Just(Step::Person),
                Just(Step::Act),
                Just(Step::Call),
                Just(Step::Complete),
                any::<bool>().prop_map(|stored_shape| Step::Stop { stored_shape }),
            ],
            0..40,
        )
    }

    fn script_lines(script: &[Step]) -> Vec<Entry> {
        let mut entries = vec![opened()];
        let mut person_count = 0usize;
        for (at, step) in script.iter().enumerate() {
            let stamp = format!("t{at}");
            match step {
                Step::Person => {
                    entries.push(said(&stamp, person(&format!("p{person_count}"))));
                    person_count += 1;
                }
                Step::Act => entries.push(said(
                    &stamp,
                    Turn::Act {
                        text: format!("act{at}"),
                        references: Vec::new(),
                    },
                )),
                Step::Call => entries.push(said(&stamp, calling_model("", "todo_show"))),
                Step::Complete => entries.push(said(&stamp, finished_model("ok"))),
                Step::Stop { stored_shape } => {
                    entries.push(Entry::Stopped {
                        at: crate::secretary::store::instant(&stamp),
                        by: "ada".to_owned(),
                    });
                    if *stored_shape {
                        entries.push(said(&stamp, finished_model("")));
                    }
                }
            }
        }
        entries
    }

    proptest::proptest! {
        /// Every person turn appears in exactly one closed batch: the batch standing immediately
        /// before the turn that closes it (an answer or a stop), and whatever is still open at the
        /// end is the unclosed tail.
        #[test]
        fn every_person_turn_is_closed_exactly_once(script in steps()) {
            let lines = script_lines(&script);
            let person_count = lines
                .iter()
                .filter(|line| matches!(line, Entry::Said { turn: Turn::Person { .. }, .. }))
                .count();
            let mut delivered: Vec<usize> = Vec::new();
            for (at, line) in lines.iter().enumerate().skip(1) {
                let closing = matches!(line, Entry::Stopped { .. })
                    || matches!(line, Entry::Said { turn: Turn::Model { calls, .. }, .. } if calls.is_empty());
                if closing {
                    let before = Stored::of(&id(), &lines[..at]).expect("a conversation");
                    delivered.extend(
                        delta(&before).iter().filter_map(|(_, said)| person_index(&said.turn)),
                    );
                }
            }
            let entries = lines;

            let after = Stored::of(&id(), &entries).expect("a conversation");
            let still_queued: Vec<usize> = delta(&after).iter().filter_map(|(_, said)| person_index(&said.turn)).collect();

            let mut everywhere = delivered.clone();
            everywhere.extend(still_queued.iter().copied());
            everywhere.sort_unstable();

            let expected: Vec<usize> = (0..person_count).collect();
            proptest::prop_assert_eq!(everywhere, expected, "every person turn exactly once");

            // A delivered index never reappears in a later batch, being behind the boundary the
            // next round's own turn moves.
            let mut seen = std::collections::HashSet::new();
            for index in &delivered {
                proptest::prop_assert!(seen.insert(*index), "index {index} delivered twice");
            }
        }
    }

    proptest::proptest! {
        /// THE OWED LAW: a live turn is queued exactly while no live turn at or after it closes a
        /// round, an answer with no calls or a stop. So a stop needs no second write, and no
        /// reading between a stop's line and anything after it calls the stopped question queued.
        #[test]
        fn a_turn_is_owed_until_an_answer_or_a_stop_comes_after_it(script in steps()) {
            let lines = script_lines(&script);
            for upto in 1..=lines.len() {
                let stored = Stored::of(&id(), &lines[..upto]).expect("a conversation");
                let live = stored.live();
                let queued: Vec<usize> = delta(&stored).iter().map(|(index, _)| *index).collect();
                let owed: Vec<usize> = live
                    .iter()
                    .enumerate()
                    .filter(|(at, _)| !live[*at..].iter().any(|(index, _)| stored.closes(*index)))
                    .map(|(_, (index, _))| *index)
                    .collect();
                proptest::prop_assert_eq!(queued, owed);
            }
        }
    }

    /// A `Turn::Person`'s index, read back out of its own text (`"p<n>"`).
    fn person_index(turn: &Turn) -> Option<usize> {
        match turn {
            Turn::Person { text, .. } => text.strip_prefix('p').and_then(|n| n.parse().ok()),
            _ => None,
        }
    }
}
