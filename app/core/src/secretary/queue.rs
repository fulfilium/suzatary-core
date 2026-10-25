//! The transcript is the queue: what is still undelivered is derived from it, never kept beside
//! it, so "a queued message is never sent twice" is a property of a derivation, not a lock. For
//! one writer it is the owed register (`transcript::Transcript::owed`), which a law holds equal.

use crate::capability::Turn;
use crate::secretary::stamp::Stamp;
use crate::secretary::store::{Said, Scope, Stored};
use crate::secretary::tool_name::ToolName;

/// How many `say`s one conversation may hold unanswered at once; past it `say` refuses rather
/// than grow the conversation forever.
pub const MAX_PENDING: usize = 16;

/// What the next `resume` sends: every live turn after the last one that closes a round, with its
/// index.
///
/// A round is closed exactly by a turn that [closes](Stored::closes) it: a `Turn::Model` with no
/// calls, or a stop, so a stopped question is never owed again, not even between two writes. A
/// view, never a mark: once a round's own turn is appended the boundary moves past what it read. A
/// turn a rewind set aside is on no path, so a regenerate's question is pending again.
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
/// owner's. The box's lines (a card it filed, a restart, a sign-in's link) are sent with the next
/// round, and wake no model alone.
#[must_use]
pub fn wakes(said: &Said) -> bool {
    match said.turn {
        Turn::Act { .. } => Stamp::owners(said.by.as_ref()),
        Turn::Person { .. } | Turn::Model { .. } | Turn::Tool { .. } | Turn::Erased { .. } => true,
    }
}

/// What a round answers: [`delta`], never empty and holding a turn that [`wakes`], so a resumed
/// session is always sent something and a nudge that added no turn asks no model.
#[derive(Debug, Clone)]
pub struct Pending<'a> {
    first: &'a Said,
    rest: Vec<&'a Said>,
}

impl<'a> Pending<'a> {
    /// What waits in `stored`, or `None` when nothing that [`wakes`] does.
    #[must_use]
    pub fn of(stored: &'a Stored) -> Option<Self> {
        let waiting = delta(stored);
        if !waiting.iter().any(|(_, said)| wakes(said)) {
            return None;
        }
        let ((_, first), rest) = waiting.split_first()?;
        Some(Self {
            first,
            rest: rest.iter().map(|(_, said)| *said).collect(),
        })
    }

    /// What a resumed session is sent: the memory's heading, if anything moved, then the waiting
    /// turns.
    #[must_use]
    pub fn delta(&self, heading: Option<Turn>) -> crate::capability::Delta {
        let first = self.first.turn.clone();
        let rest = self.rest.iter().map(|said| said.turn.clone());
        match heading {
            Some(heading) => {
                crate::capability::Delta::new(heading, std::iter::once(first).chain(rest))
            }
            None => crate::capability::Delta::new(first, rest),
        }
    }
}

/// The turn heading a batch with what the model is told it remembers. Never stored: the
/// `MemorySeen` line written after the answer is what makes it once.
#[must_use]
pub fn heading(told: String) -> Turn {
    Turn::Tool {
        verb: ToolName::Memory,
        answer: told,
        outcome: crate::secretary::outcome::Outcome::Answered,
        files: Vec::new(),
    }
}

/// A batch: the heading, if any, then the turns.
#[must_use]
pub fn headed(heading: Option<Turn>, turns: &[(usize, &Said)]) -> Vec<Turn> {
    heading
        .into_iter()
        .chain(turns.iter().map(|(_, said)| said.turn.clone()))
        .collect()
}

/// How many turns a model without a session is sent: the conversation page's
/// own default, so what the owner sees and what the model reads are one window.
pub const MODEL_WINDOW: usize = 60;

/// What a model without a session is sent: the last [`MODEL_WINDOW`] live turns, starting at the
/// first round boundary inside them.
///
/// A window cut mid-round is malformed (a tool answer without its call), so it starts where
/// [`delta`] cuts; a round longer than the window is sent whole, from its own start.
#[must_use]
pub fn for_model(stored: &Stored) -> Vec<(usize, &Said)> {
    window_of(stored, &stored.live(), MODEL_WINDOW).to_vec()
}

/// [`for_model`] over any window size, so a test can use six turns, not sixty.
fn window_of<'a, 'b>(
    stored: &Stored,
    turns: &'a [(usize, &'b Said)],
    limit: usize,
) -> &'a [(usize, &'b Said)] {
    let floor = turns.len().saturating_sub(limit);
    let starts_a_round = |index: usize| stored.opens(turns, index);
    let start = (floor..turns.len())
        .find(|&index| starts_a_round(index))
        .or_else(|| (0..floor).rev().find(|&index| starts_a_round(index)))
        .unwrap_or(0);
    &turns[start..]
}

/// Every turn a model asked as `stored` records ([`Stored::scope`]) holds now ([`sent`]).
#[must_use]
pub fn held(stored: &Stored) -> Vec<(usize, &Said)> {
    sent(stored, stored.scope)
}

/// Every turn a model asked over `scope` holds: for a transcript, every stored turn, which is what
/// it is sent; for a window, what [`for_model`] sends and, when a session is kept, every live turn
/// from the first one its opening sent, since a resumed session is sent only what it has not read
/// and keeps all it has.
#[must_use]
pub fn sent(stored: &Stored, scope: Scope) -> Vec<(usize, &Said)> {
    match scope {
        Scope::Transcript => stored.turns.iter().enumerate().collect(),
        Scope::Window => {
            let window = for_model(stored);
            let Some(kept) = &stored.session else {
                return window;
            };
            let from = window
                .first()
                .map_or(stored.turns.len(), |(at, _)| *at)
                .min(kept.from);
            stored
                .live()
                .into_iter()
                .filter(|(at, _)| *at >= from)
                .collect()
        }
    }
}

/// How many lines are still queued: [`delta`]'s length, for the `MAX_PENDING` refusal and a badge.
#[must_use]
pub fn pending(stored: &Stored) -> usize {
    delta(stored).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secretary::store::{ConversationId, Entry};

    fn id() -> ConversationId {
        ConversationId::new("2026-09-13-1").expect("a slug")
    }

    fn opened() -> Entry {
        Entry::Opened {
            at: crate::capability::Datetime::new(2026, 9, 13, 9, 0, 0, 0).expect("an instant"),
            title: crate::secretary::words::Words::kept("Morning".to_owned()),
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
            calls: vec![crate::capability::ToolCall {
                verb: verb.into(),
                args: std::collections::BTreeMap::new(),
            }],
        }
    }

    fn stored(entries: &[Entry]) -> Stored {
        Stored::of(&id(), entries).expect("a conversation")
    }

    /// `delta`'s bare turns, without each `Said`'s `at`.
    fn turns<'a>(delta: &[(usize, &'a Said)]) -> Vec<&'a Turn> {
        delta.iter().map(|(_, said)| &said.turn).collect()
    }

    /// The box's own notes wake no model: a conversation holding only them waits for nothing,
    /// and the owner's next turn is sent with them, in order.
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
        assert!(Pending::of(&stored(&lines)).is_none(), "nothing wakes");
        let mut owner = said("2026-09-13T09:03:00", person("add my Work calendar"));
        if let Entry::Said { by, .. } = &mut owner {
            *by = Some(Stamp::Actor("ada".to_owned()));
        }
        lines.push(owner);
        let waiting = stored(&lines);
        let pending = Pending::of(&waiting).expect("the owner's turn wakes");
        assert_eq!(pending.delta(None).turns().len(), 3, "the notes ride along");
        let dismissed = Entry::Said {
            at: crate::capability::Datetime::new(2026, 9, 13, 9, 4, 0, 0).expect("an instant"),
            turn: Turn::Act {
                text: "Ada dismissed it: not now".to_owned(),
                references: Vec::new(),
            },
            by: Some(Stamp::Actor("ada".to_owned())),
        };
        assert!(
            Pending::of(&stored(&[opened(), dismissed])).is_some(),
            "the owner's own act still wakes"
        );
    }

    fn stop(at: &str) -> Entry {
        Entry::Stopped {
            at: crate::secretary::store::instant(at),
            by: "ada".to_owned(),
        }
    }

    /// A conversation stored before a stop closed by itself holds the stop then an empty end; it
    /// reads exactly as it did: nothing queued, and the empty end opens no round of its own.
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
        let live = stored.live();
        let window = window_of(&stored, &live, 3);
        assert_eq!(
            turns(window),
            vec![&person("p3"), &finished_model("three")],
            "the empty end belongs to its stop, so the window starts after it"
        );
    }

    // --- The property: every person turn is delivered exactly once. -------

    /// One step of a simulated conversation: the owner says something, dismisses a proposal, a
    /// round asks for a verb, a round completes, delivering whatever is queued, or the owner stops
    /// the round, closing whatever is queued unanswered: in one line, or as stored conversations
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

    /// The lines a script comes to.
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
                    // The batch as it stood right before the closing line joined it.
                    let before = Stored::of(&id(), &lines[..at]).expect("a conversation");
                    delivered.extend(
                        delta(&before).iter().filter_map(|(_, said)| person_index(&said.turn)),
                    );
                }
            }
            let entries = lines;

            // Whatever is still queued at the end is the current, not yet answered delta.
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
