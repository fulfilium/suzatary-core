//! What a round has read and who opened it, the two facts the binding gate reads.
//!
//! Deny by default: a verb is [`Taint::Clean`] only where `VerbName` says so, and a turn is
//! clean only when it is the model's own, a clean verb's answer, or stamped as the owner's.

use crate::secretary::model::Turn;
use crate::secretary::queue;
use crate::secretary::store::{Entry, Said, Stored};
use crate::secretary::turn_shape::{TurnShape, shape};
use crate::secretary::verb_name::VerbName;
use suzatary_base::conversation_id::ConversationId;
use suzatary_inbox::secretary::stamp::Stamp;
use suzatary_memory::secretary::memory::Origin;

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
    pub const ALL: [Taint; 2] = [Taint::Clean, Taint::Tainted];

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

prodrome_data::word_literal!(Taint: Taint::ALL => |taint| taint.as_str());

/// The one printed form, `as_str`'s, in JSON too.
impl serde::Serialize for Taint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> serde::Deserialize<'de> for Taint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Taint::parse(&text)
            .ok_or_else(|| serde::de::Error::unknown_variant(&text, &["clean", "tainted"]))
    }
}

/// One turn's own class as a reader reads it off its shape, with no word: a model's turn is
/// clean here, and what it carries is what its line stores its call was sent
/// ([`Carried`](crate::secretary::store::Carried)). A verb answer is its verb's class, and a
/// `zoom` answer, whose class only its words name, reads tainted: the box stores every verb
/// answer's class when it writes it ([`written`]), so this reads only an older line. A verb
/// answer under a name the surface lacks is tainted, and so are a person's photos and PDFs.
#[must_use]
pub fn turn(said: &Said) -> Taint {
    of_turn(&said.turn, said.by.as_ref(), Taint::Tainted)
}

/// A turn's class as its writer stores it: [`turn`]'s, but a `zoom` answer's from its words, the
/// `taint` it wrote for the node it opened.
#[must_use]
pub fn written(said: &Said) -> Taint {
    let zoom = match &said.turn {
        Turn::Tool { answer, .. } => zoomed(answer),
        _ => Taint::Tainted,
    };
    of_turn(&said.turn, said.by.as_ref(), zoom)
}

/// A turn's class off its shape, a `zoom` answer's being `zoom`.
fn of_turn(turn: &Turn, stamp: Option<&Stamp>, zoom: Taint) -> Taint {
    match shape(turn) {
        TurnShape::Model { .. } => Taint::Clean,
        TurnShape::Tool { verb, outcome, .. } => match verb.verb() {
            Some(VerbName::Zoom) if !outcome.refused() => zoom,
            Some(verb) => verb.powers().taint,
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

/// A `zoom` answer's class: the `taint` it wrote for the node it opened; anything else reads
/// tainted.
fn zoomed(answer: &str) -> Taint {
    serde_json::from_str::<serde_json::Value>(answer)
        .ok()
        .and_then(|answer| Taint::parse(answer.get("taint")?.as_str()?))
        .unwrap_or(Taint::Tainted)
}

/// One stored line's class: a said line as its turn's; a line of the box's own bookkeeping,
/// which holds only ids, counts and names the box chose, clean; and a line holding free text a
/// model or a review wrote, tainted.
#[must_use]
pub fn line(entry: &Entry) -> Taint {
    match entry {
        Entry::Said { turn, by, .. } => of_turn(turn, by.as_ref(), Taint::Tainted),
        Entry::Proposed { .. }
        | Entry::Stopped { .. }
        | Entry::Rewound { .. }
        | Entry::Retired { .. }
        | Entry::Scoped { .. }
        | Entry::Erased { .. }
        | Entry::Resealed { .. }
        | Entry::Began { .. }
        | Entry::Again { .. } => Taint::Clean,
        Entry::Opened { .. }
        | Entry::SetUp { .. }
        | Entry::Titled { .. }
        | Entry::Reviewed { .. }
        | Entry::ReviewFailed { .. }
        | Entry::Coding { .. } => Taint::Tainted,
    }
}

/// Each line's taint in its conversation `id`, in the order its fold takes them, each beside the
/// turns it sets aside (the transcript's `folding`): a person's or an act's turn keeps its own
/// class, a model's turn what its fold reads it carried (its line's stored carry, else every live
/// turn before it), and any other line joins its round so far. No window is read: a label is a
/// fact of the lines, whatever bound or weight a call is sent by.
#[must_use]
pub fn rounds<'e>(
    id: &ConversationId,
    lines: impl IntoIterator<Item = crate::secretary::store::Folded<'e>>,
) -> Vec<Taint> {
    let mut so_far: Option<Stored> = None;
    let mut round = Taint::Clean;
    lines
        .into_iter()
        .map(|folded| {
            let entry = folded.entry;
            let so_far = so_far.get_or_insert_with(|| {
                Stored::empty(id, crate::secretary::store::Kind::Topic, entry.at())
            });
            let turn = so_far.turns.len();
            // A line the conversation's own fold refuses moves nothing.
            let _ = so_far.fold_read(folded);
            // A turn's own class is what its fold reads it carries, stored where the box stored
            // one, and tainted for a turn the fold refused, which no read can vouch for; any
            // other line's is its kind's.
            let own = match entry {
                Entry::Said { .. } | Entry::Stopped { .. } => so_far
                    .carried
                    .get(turn)
                    .map_or(Taint::Tainted, |carried| carried.taint),
                _ => line(entry),
            };
            let taint = match entry {
                _ if opens(entry) => own,
                Entry::Said { turn: said, .. }
                    if matches!(shape(said), TurnShape::Model { .. }) =>
                {
                    own
                }
                _ => round.join(own),
            };
            round = taint;
            taint
        })
        .collect()
}

/// What a call sent `turns` of `stored` carries: the join of each one's
/// [`Carried::taint`](crate::secretary::store::Carried), a marker's included, since an answer
/// carries what its writer held whether it is sent whole or not.
#[must_use]
pub fn sent(stored: &Stored, turns: impl IntoIterator<Item = usize>) -> Taint {
    turns.into_iter().fold(Taint::Clean, |held, index| {
        held.join(stored.carried[index].taint)
    })
}

/// What an ask now would carry: [`sent`] over what it would send ([`queue::held`]). The gate
/// does not read it: it reads what the round's asks WERE sent, which the round hands its verbs.
#[must_use]
pub fn held(stored: &Stored) -> Taint {
    sent(
        stored,
        queue::held(stored).into_iter().map(|(index, _)| index),
    )
}

fn opens(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Said { turn, .. }
            if matches!(shape(turn), TurnShape::Person { .. } | TurnShape::Act { .. })
    )
}

/// Who opened this round, if the owner did: the person or act turns before the model first spoke,
/// provided every one carries the gate's stamp.
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
    use crate::secretary::model::ToolCall;
    use crate::secretary::store::Entry;
    use suzatary_base::conversation_id::ConversationId;

    fn said(turn: Turn, by: Option<&str>) -> Entry {
        Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0)
                .expect("an instant"),
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

    fn tainted(stored: &Stored) -> bool {
        held(stored) == Taint::Tainted
    }

    fn stored(lines: Vec<Entry>) -> Stored {
        let mut entries = vec![Entry::Opened {
            at: suzatary_base::capability::Datetime::new(2026, 9, 25, 8, 0, 0, 0)
                .expect("an instant"),
            title: suzatary_memory::secretary::words::Words::kept("t".to_owned()),
            model: "m".to_owned(),
            kind: crate::secretary::store::Kind::Topic,
        }];
        entries.extend(lines);
        Stored::of(&ConversationId::new("default").expect("a slug"), &entries)
            .expect("a conversation")
    }

    /// `lines` folded as this box writes them: each model's turn storing what an ask then would
    /// have been sent ([`held`]), each verb's answer its class ([`written`]).
    fn written_now(lines: &[Entry]) -> Stored {
        let mut stored = stored(Vec::new());
        for entry in lines {
            let carries = match entry {
                Entry::Said {
                    turn: Turn::Model { .. },
                    ..
                } => Some(held(&stored)),
                _ => Said::of(entry)
                    .filter(|said| matches!(said.turn, Turn::Tool { .. }))
                    .map(|said| written(&said)),
            };
            stored
                .fold_read(crate::secretary::store::Folded {
                    entry,
                    aside: Vec::new(),
                    weight: crate::secretary::store::Folded::bare(entry).weight,
                    carries,
                })
                .expect("folds");
        }
        stored
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
            rounds(
                &ConversationId::new("default").expect("a slug"),
                lines.iter().map(crate::secretary::store::Folded::bare)
            ),
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
            at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0)
                .expect("an instant"),
            by: "ada".to_owned(),
        };
        assert_eq!(line(&stopped), Taint::Clean);
        let failed = Entry::ReviewFailed {
            at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0)
                .expect("an instant"),
            reason: suzatary_memory::secretary::words::Words::kept("the batch said so".to_owned()),
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
    fn only_eight_verbs_are_clean() {
        let clean: Vec<&str> = VerbName::ALL
            .iter()
            .filter(|verb| verb.powers().taint == Taint::Clean)
            .map(|verb| verb.name())
            .collect();
        assert_eq!(
            clean,
            vec![
                "context",
                "remember",
                "forget",
                "consolidate",
                "propose_act",
                "weather",
                "add_topics",
                "revise_topics"
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

    /// The box's own notes carry nobody's words: a clean round the owner opens after them still
    /// writes the memory, and none of them opens a round as his.
    #[test]
    fn the_boxs_own_notes_leave_his_round_clean() {
        let noted = |text: &str| {
            Entry::noted(
                suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0)
                    .expect("an instant"),
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

    fn ask() -> Vec<Entry> {
        vec![
            said(person("remember I moved"), Some("ada")),
            said(calling("remember"), None),
        ]
    }

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

    /// What a model holds is what it is sent, less what a rewind set aside, and every answer
    /// carries what its writer held: a read outlives the window in the answers written after it.
    #[test]
    fn a_read_counts_while_the_model_holds_it() {
        let within: Vec<Entry> = read().into_iter().chain(chat(20)).chain(ask()).collect();
        assert!(tainted(&stored(within)), "in the window: a card");
        let aged: Vec<Entry> = read().into_iter().chain(chat(40)).chain(ask()).collect();
        let aged = stored(aged);
        assert!(
            !queue::held(&aged)
                .iter()
                .any(|(_, said)| turn(said) == Taint::Tainted),
            "the read itself is out of the window"
        );
        assert!(
            tainted(&aged),
            "the answers written after it carry it: a card"
        );
        let rewound: Vec<Entry> = read()
            .into_iter()
            .chain([Entry::Rewound {
                to: 0,
                at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0)
                    .expect("an instant"),
                by: "ada".to_owned(),
            }])
            .chain(ask())
            .collect();
        assert!(!tainted(&written_now(&rewound)), "set aside: written");
        assert!(
            tainted(&stored(rewound)),
            "an older answer, which stores nothing, carries every turn before it, set aside or not"
        );
    }

    /// An owner's act on a todo the agent wrote and nobody confirmed taints what its model holds
    /// even when nothing loaded the todo.
    #[test]
    fn an_act_on_an_unconfirmed_todo_makes_remember_a_card() {
        let act = |by: Stamp| Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0)
                .expect("an instant"),
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
        let pointing = after(Stamp::Unconfirmed(
            suzatary_inbox::secretary::stamp::Speaker::device("ada"),
        ));
        assert!(tainted(&pointing));
        assert!(opener(&pointing).is_some(), "his round, so a card");
        assert!(!tainted(&after(Stamp::Actor(
            suzatary_inbox::secretary::stamp::Speaker::device("ada")
        ))));
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

    /// A message said during a round's last ask lands before the round's answer, so the answer
    /// closes it too; the round that claims it next opens on the model's own turn, has no opener,
    /// and its `remember` is refused: it fails closed. `opener` reads `queue::delta`, as on master.
    #[test]
    fn a_message_said_before_the_last_answer_leaves_the_next_round_no_opener() {
        let now = stored(vec![
            said(person("what did the bank say"), Some("ada")),
            said(calling("weather"), None),
            said(answered("weather"), None),
            said(person("and remember I moved"), Some("ada")),
            said(
                Turn::Model {
                    text: "it said so".to_owned(),
                    calls: Vec::new(),
                },
                None,
            ),
            said(calling("remember"), None),
        ]);
        assert_eq!(opener(&now), None);
    }
}
