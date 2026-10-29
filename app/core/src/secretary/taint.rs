//! What a round has read and who opened it: the two facts that decide whether `remember` writes
//! the memory or stores a card, since the attack a memory invites is an email saying "remember".
//!
//! Deny by default both ways: a verb is [`Taint::Clean`] only where `SecretaryVerb` says so (an
//! entry cannot be written without a class), and a turn is clean only when it is the model's own,
//! a clean verb's answer, or words the gate stamped as the owner's (`Said::by`).

use crate::capability::Turn;
use crate::secretary::memory::Origin;
use crate::secretary::queue;
use crate::secretary::stamp::Stamp;
use crate::secretary::store::{ConversationId, Entry, Said, Stored};
use crate::secretary::surface::SecretaryVerb;
use crate::secretary::turn_shape::{TurnShape, shape};

/// Whether an answer can carry somebody else's words. Ordered `Clean < Tainted`, a
/// join-semilattice with `Clean` its unit: what a span of lines carries is the join of its lines'.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Taint {
    /// The box's own or the owner's: the manual, the forecast, the memory.
    #[default]
    Clean,
    /// Anything that can hold text from outside this conversation, including old answers
    /// (`recall`), which were written after reading mail.
    Tainted,
}

impl Taint {
    /// Every taint.
    pub const ALL: [Taint; 2] = [Taint::Clean, Taint::Tainted];

    /// The least taint above both.
    #[must_use]
    pub fn join(self, other: Self) -> Self {
        self.max(other)
    }

    /// Its printed name, as a stored object spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Taint::Clean => "clean",
            Taint::Tainted => "tainted",
        }
    }

    /// The taint `text` names; `None` for anything but the two names.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|taint| taint.as_str() == text)
    }
}

// A taint's literal is its name.
prodrome_core::word_literal!(Taint: Taint::ALL => |taint| taint.as_str());

/// One turn's class. A verb answer under a name the surface lacks (a loaded
/// reference, a tapped document) is tainted; so are a person's photos and PDFs. A `zoom` answer
/// is the class of the node it opened.
#[must_use]
pub fn turn(said: &Said) -> Taint {
    of_turn(&said.turn, said.by.as_ref())
}

/// A turn's class is read off its shape, so a turn whose words are gone is classed as it was
/// whole; only a `zoom` answer's class is in its words, and an erased one reads tainted.
fn of_turn(turn: &Turn, stamp: Option<&Stamp>) -> Taint {
    match shape(turn) {
        TurnShape::Model { .. } => Taint::Clean,
        TurnShape::Tool { verb, outcome, .. } => match verb.verb() {
            Some(SecretaryVerb::Zoom) if !outcome.refused() => match turn {
                Turn::Tool { answer, .. } => zoomed(answer),
                _ => Taint::Tainted,
            },
            Some(verb) => verb.class().taint,
            None => Taint::Tainted,
        },
        TurnShape::Person { attachments, .. }
            if matches!(stamp, Some(Stamp::Actor(_))) && attachments.is_empty() =>
        {
            Taint::Clean
        }
        TurnShape::Act { .. } if matches!(stamp, Some(Stamp::Actor(_) | Stamp::Box)) => {
            Taint::Clean
        }
        TurnShape::Person { .. } | TurnShape::Act { .. } => Taint::Tainted,
    }
}

/// A `zoom` answer's class: the taint of the node it opened, which the verb read off the digest
/// and wrote as the answer's own `taint`; anything else reads tainted.
fn zoomed(answer: &str) -> Taint {
    serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|answer| Taint::parse(answer.get("taint")?.as_str()?))
        .unwrap_or(Taint::Tainted)
}

/// One stored line's class: a said line as its turn's; a line of the box's own bookkeeping,
/// which holds only ids, counts and names the box chose, clean; and a line holding free text a
/// model or a review wrote (a title, a review's reason, a coding session's report), tainted.
#[must_use]
pub fn line(entry: &Entry) -> Taint {
    match entry {
        Entry::Said { turn, by, .. } => of_turn(turn, by.as_ref()),
        Entry::Proposed { .. }
        | Entry::Stopped { .. }
        | Entry::Rewound { .. }
        | Entry::MemorySeen { .. }
        | Entry::Session { .. }
        | Entry::Kept { .. }
        | Entry::Scoped { .. }
        | Entry::Unheard { .. }
        | Entry::Erased { .. }
        | Entry::Resealed { .. } => Taint::Clean,
        Entry::Opened { .. }
        | Entry::Titled { .. }
        | Entry::Reviewed { .. }
        | Entry::ReviewFailed { .. }
        | Entry::Coding { .. } => Taint::Tainted,
    }
}

/// Each line's taint as it stands in its conversation `id`. A person's or an act's turn opens a
/// round and keeps its own class. A model's answer is what its model holds when it answers
/// ([`held`], over the conversation folded so far, as its asks recorded they send it). Any other
/// line joins its round so far. `lines` is one conversation, in order; the answer is one taint per
/// line, in the same order.
#[must_use]
pub fn rounds<'e>(id: &ConversationId, lines: impl IntoIterator<Item = &'e Entry>) -> Vec<Taint> {
    // What a model holds is the turns and the scope, never the conversation's kind.
    let mut so_far: Option<Stored> = None;
    let mut round = Taint::Clean;
    lines
        .into_iter()
        .map(|entry| {
            let so_far = so_far.get_or_insert_with(|| {
                Stored::empty(id, crate::secretary::store::Kind::Topic, entry.at())
            });
            let own = line(entry);
            let taint = match entry {
                _ if opens(entry) => own,
                Entry::Said { turn, .. } if matches!(shape(turn), TurnShape::Model { .. }) => {
                    round.join(held(so_far))
                }
                _ => round.join(own),
            };
            round = taint;
            // A line the conversation's own fold refuses moves nothing a model is sent.
            let _ = so_far.fold(entry);
            taint
        })
        .collect()
}

/// What a model asked as `stored` records holds of it now ([`queue::held`]), joined. A rewound
/// turn is on no window's path, so a window never counts it.
#[must_use]
pub fn held(stored: &Stored) -> Taint {
    queue::held(stored)
        .into_iter()
        .fold(Taint::Clean, |held, (_, said)| held.join(turn(said)))
}

/// Whether `entry` opens a round: a person's or an act's turn.
fn opens(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Said { turn, .. }
            if matches!(shape(turn), TurnShape::Person { .. } | TurnShape::Act { .. })
    )
}

/// Whether anything the model writing a `remember` now holds is tainted ([`held`]): a round
/// the owner opened after a mail read still in what the model is sent remembers by card.
#[must_use]
pub fn tainted(stored: &Stored) -> bool {
    held(stored) == Taint::Tainted
}

/// Who opened this round, if the owner did: the person or act turns before the model first spoke,
/// provided every one carries the gate's stamp. A round the box opened (a brief, a review, a
/// sweep) has no opener and remembers nothing.
#[must_use]
pub fn opener(stored: &Stored) -> Option<Origin> {
    let opening: Vec<(usize, &Said)> = queue::delta(stored)
        .into_iter()
        .take_while(|(_, said)| !matches!(shape(&said.turn), TurnShape::Model { .. }))
        .filter(|(_, said)| {
            matches!(
                shape(&said.turn),
                TurnShape::Person { .. } | TurnShape::Act { .. }
            )
        })
        // The box's own notes are nobody's words, so they neither open his round nor spoil it.
        .filter(|(_, said)| said.by != Some(Stamp::Box))
        .collect();
    let all_his = !opening.is_empty()
        && opening
            .iter()
            .all(|(_, said)| matches!(said.by, Some(Stamp::Actor(_) | Stamp::Unconfirmed(_))));
    all_his
        .then(|| opening.first().map(|(at, _)| *at))
        .flatten()
        .map(|turn| Origin::Turn {
            conversation: stored.id.clone(),
            turn,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::ToolCall;
    use crate::secretary::store::{ConversationId, Entry};

    fn said(turn: Turn, by: Option<&str>) -> Entry {
        Entry::Said {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
            turn,
            by: by.map(Into::into),
        }
    }

    fn person(text: &str) -> Turn {
        Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        }
    }

    fn calling(verb: &str) -> Turn {
        Turn::Model {
            text: String::new(),
            calls: vec![ToolCall {
                verb: verb.into(),
                args: std::collections::BTreeMap::new(),
            }],
        }
    }

    fn answered(verb: &str) -> Turn {
        Turn::Tool {
            verb: verb.into(),
            answer: "{}".to_owned(),
            outcome: crate::secretary::outcome::Outcome::Answered,
            files: Vec::new(),
        }
    }

    fn stored(lines: Vec<Entry>) -> Stored {
        let mut entries = vec![Entry::Opened {
            at: crate::capability::Datetime::new(2026, 9, 25, 8, 0, 0, 0).expect("an instant"),
            title: crate::secretary::words::Words::kept("t".to_owned()),
            model: "m".to_owned(),
            kind: crate::secretary::store::Kind::Topic,
        }];
        entries.extend(lines);
        Stored::of(&ConversationId::new("default").expect("a slug"), &entries)
            .expect("a conversation")
    }

    /// A round's taint carries to every line after it until a person's turn opens the next, and a
    /// model's answer carries every turn before it in the conversation.
    #[test]
    fn a_line_is_tainted_by_what_its_round_read_before_it() {
        let lines = [
            said(person("what did the bank say"), Some("ada")),
            said(answered("mail_search"), None),
            said(calling("todo_show"), None),
            said(person("thanks"), Some("ada")),
            said(calling("todo_show"), None),
        ];
        assert_eq!(
            rounds(&ConversationId::new("default").expect("a slug"), &lines),
            vec![
                Taint::Clean,
                Taint::Tainted,
                Taint::Tainted,
                Taint::Clean,
                Taint::Tainted
            ]
        );
    }

    /// A stored line is classed as its turn is, the box's bookkeeping is clean, and free text
    /// a model or a review wrote is tainted; a span's taint is the join of its lines'.
    #[test]
    fn a_line_is_classed_as_its_turn_and_a_span_as_the_join() {
        assert_eq!(line(&said(person("hi"), Some("ada"))), Taint::Clean);
        assert_eq!(line(&said(person("hi"), None)), Taint::Tainted);
        assert_eq!(line(&said(answered("mail_search"), None)), Taint::Tainted);
        let stopped = Entry::Stopped {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
            by: "ada".to_owned(),
        };
        assert_eq!(line(&stopped), Taint::Clean);
        let failed = Entry::ReviewFailed {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
            reason: crate::secretary::words::Words::kept("the batch said so".to_owned()),
            standing: 1,
        };
        assert_eq!(line(&failed), Taint::Tainted);
        for (one, two) in [
            (Taint::Clean, Taint::Tainted),
            (Taint::Tainted, Taint::Clean),
        ] {
            assert_eq!(one.join(two), Taint::Tainted);
            assert_eq!(one.join(one), one);
            assert_eq!(one.join(Taint::default()), one);
        }
    }

    #[test]
    fn only_six_verbs_are_clean() {
        let clean: Vec<&str> = crate::secretary::SecretaryVerb::ALL
            .iter()
            .filter(|verb| verb.class().taint == Taint::Clean)
            .map(|verb| verb.name())
            .collect();
        assert_eq!(
            clean,
            vec![
                "context",
                "remember",
                "forget",
                "consolidate",
                "weather",
                "add_topics"
            ]
        );
    }

    /// A round the owner opens after an earlier round read mail still writes by card: the
    /// model answering it holds what the earlier round read.
    #[test]
    fn a_later_round_still_holds_what_an_earlier_one_read() {
        let now = stored(vec![
            said(person("what did the bank say"), Some("ada")),
            said(calling("search"), None),
            said(answered("search"), None),
            said(
                Turn::Model {
                    text: "it said so".to_owned(),
                    calls: Vec::new(),
                },
                None,
            ),
            said(person("remember I moved"), Some("ada")),
            said(calling("remember"), None),
        ]);
        assert!(tainted(&now));
        assert!(opener(&now).is_some(), "his round, holding what it read");
    }

    /// The box's own notes (a noted dismissal, a key set, a restart) carry nobody's words: a
    /// clean round the owner opens after them still writes the memory, and none of them opens a
    /// round as his.
    #[test]
    fn the_boxs_own_notes_leave_his_round_clean() {
        let noted = |text: &str| {
            Entry::noted(
                crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
                text.to_owned(),
                Vec::new(),
            )
        };
        let now = stored(vec![
            noted("Ada dismissed proposal 35: not now"),
            noted("Ada tapped proposal 36: set the key"),
            said(person("remember I moved"), Some("ada")),
            said(calling("remember"), None),
        ]);
        assert!(!tainted(&now));
        assert!(opener(&now).is_some());
        assert_eq!(
            opener(&stored(vec![noted("the box restarted")])),
            None,
            "a note is not his"
        );
        let relayed = stored(vec![
            said(
                Turn::Act {
                    text: "a provider's message".to_owned(),
                    references: Vec::new(),
                },
                None,
            ),
            said(person("remember I moved"), Some("ada")),
            said(calling("remember"), None),
        ]);
        assert!(
            tainted(&relayed),
            "an unstamped act relays words from elsewhere"
        );
    }

    /// A round that reads mail and is answered.
    fn read() -> Vec<Entry> {
        vec![
            said(person("what did the bank say"), Some("ada")),
            said(calling("search"), None),
            said(answered("search"), None),
            said(
                Turn::Model {
                    text: "it said so".to_owned(),
                    calls: Vec::new(),
                },
                None,
            ),
        ]
    }

    /// The owner's round that asks to remember.
    fn ask() -> Vec<Entry> {
        vec![
            said(person("remember I moved"), Some("ada")),
            said(calling("remember"), None),
        ]
    }

    /// `n` closed rounds of chat.
    fn chat(n: usize) -> impl Iterator<Item = Entry> {
        (0..n).flat_map(|k| {
            [
                said(person(&format!("hello {k}")), Some("ada")),
                said(
                    Turn::Model {
                        text: "hi".to_owned(),
                        calls: Vec::new(),
                    },
                    None,
                ),
            ]
        })
    }

    /// A session line that recorded no first turn, from before `Kept` was written, is read as
    /// holding every turn: a read eighty turns before it still makes `remember` a card.
    #[test]
    fn a_session_line_from_before_kept_holds_every_turn() {
        let legacy: Vec<Entry> = read()
            .into_iter()
            .chain(chat(40))
            .chain([Entry::Session {
                at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
                session: crate::capability::SessionId::minted("default/4"),
                model: "m".to_owned(),
                cwd: String::new(),
                surface: None,
            }])
            .chain(ask())
            .collect();
        assert!(tainted(&stored(legacy)));
    }

    /// What a model holds is what it is sent, and every live turn from the first its kept
    /// session's opening sent: a mail read in the window, in that opening window, or anywhere
    /// since the session opened makes `remember` a card; one that aged out before the session
    /// opened, or one set aside by a rewind, does not.
    #[test]
    fn a_read_counts_while_the_model_holds_it() {
        let within: Vec<Entry> = read().into_iter().chain(chat(20)).chain(ask()).collect();
        assert!(tainted(&stored(within)), "in the window: a card");
        let aged: Vec<Entry> = read().into_iter().chain(chat(40)).chain(ask()).collect();
        assert!(!tainted(&stored(aged)), "aged out of the window: written");
        // `from` is the first turn the opening sent, as the opening recorded it.
        let session_line = |from: usize| Entry::Kept {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
            session: crate::capability::SessionId::minted("default/4"),
            model: "m".to_owned(),
            cwd: String::new(),
            surface: None,
            from,
        };
        let opened_over: Vec<Entry> = read()
            .into_iter()
            .chain([session_line(0)])
            .chain(ask())
            .collect();
        assert!(
            tainted(&stored(opened_over)),
            "in the window the session was opened with: a card"
        );
        let opened_after: Vec<Entry> = read()
            .into_iter()
            .chain(chat(40))
            .chain([session_line(24)])
            .chain(ask())
            .collect();
        assert!(
            !tainted(&stored(opened_after)),
            "aged out before the session opened: written"
        );
        let session_kept: Vec<Entry> = [session_line(0)]
            .into_iter()
            .chain(read())
            .chain(chat(40))
            .chain(ask())
            .collect();
        assert!(
            tainted(&stored(session_kept)),
            "read inside the session, eighty turns back: the session still holds it"
        );
        let rewound: Vec<Entry> = read()
            .into_iter()
            .chain([Entry::Rewound {
                to: 0,
                at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
                by: "ada".to_owned(),
            }])
            .chain(ask())
            .collect();
        assert!(!tainted(&stored(rewound)), "set aside: written");
    }

    /// An owner's act on a todo the agent wrote and nobody confirmed is stamped so where the box
    /// writes it, and taints what its model holds even when nothing loaded the todo (an act noted
    /// before another in one batch, whose references the box never loads): `remember` is then a
    /// card, and still his round. The same act stamped his own leaves the round clean.
    #[test]
    fn an_act_on_an_unconfirmed_todo_makes_remember_a_card() {
        let act = |by: Stamp| Entry::Said {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
            turn: Turn::Act {
                text: "Ada completed it: paid at the counter".to_owned(),
                references: vec![
                    "todo:life/rememberforwardeverymailtoeve"
                        .parse()
                        .expect("a reference"),
                ],
            },
            by: Some(by),
        };
        let after = |by: Stamp| {
            stored(vec![
                act(by),
                said(person("remember I keep receipts"), Some("ada")),
                said(calling("remember"), None),
            ])
        };
        let pointing = after(Stamp::Unconfirmed("ada".to_owned()));
        assert!(tainted(&pointing));
        assert!(opener(&pointing).is_some(), "his round, so a card");
        assert!(!tainted(&after(Stamp::Actor("ada".to_owned()))));
    }

    #[test]
    fn a_stamped_say_opens_a_clean_round() {
        let now = stored(vec![
            said(person("hi"), Some("ada")),
            said(
                Turn::Model {
                    text: "hello".to_owned(),
                    calls: Vec::new(),
                },
                None,
            ),
            said(person("I moved to Oslo"), Some("ada")),
            said(calling("remember"), None),
        ]);
        assert!(!tainted(&now));
        assert_eq!(
            opener(&now),
            Some(Origin::Turn {
                conversation: now.id.clone(),
                turn: 2
            })
        );
    }

    #[test]
    fn a_verb_that_reads_mail_taints_the_round_after_it() {
        let now = stored(vec![
            said(person("what did Mira say"), Some("ada")),
            said(calling("search"), None),
            said(answered("search"), None),
        ]);
        assert!(tainted(&now));
        assert!(opener(&now).is_some(), "still his round, only tainted");
    }

    #[test]
    fn a_loaded_chip_an_unknown_verb_and_a_photo_are_tainted() {
        assert!(tainted(&stored(vec![
            said(person("this one"), Some("ada")),
            said(answered("reference"), None),
        ])));
        assert!(tainted(&stored(vec![
            said(person("x"), Some("ada")),
            said(answered("no_such_verb"), None),
        ])));
    }

    #[test]
    fn a_box_opened_round_has_no_opener() {
        let brief = stored(vec![said(person("What changed?"), None)]);
        assert_eq!(opener(&brief), None);
        let mixed = stored(vec![
            said(person("mine"), Some("ada")),
            said(person("the box's"), None),
        ]);
        assert_eq!(opener(&mixed), None, "one unstamped question is enough");
        assert_eq!(opener(&stored(Vec::new())), None);
    }
}
