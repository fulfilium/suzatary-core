//! What `taint` says a model holds against what a model was sent: a simulated model records every
//! turn it is sent (a stateless one its window, a session its opening window and each delta
//! resumed), over generated conversations of the owner's turns, mail read and weather looked up,
//! sessions opened (some with a turn said while they open, some on a line from before the first
//! turn was recorded), reopened when the manual changes, lost and answered by a stateless call
//! instead, transcript asks (the sweep's), stops and rewinds. A stateless answer while a session
//! is kept ends that session ([`Entry::Unheard`]), as the round writes it. Whenever a model holds a tainted
//! turn, `remember` is a card and `taint::rounds` labels its answer tainted (law 9's second half:
//! nothing the model read reaches the memory unasked).

use std::collections::BTreeSet;

use proptest::prelude::*;
use suzatary_core::conversation_id::ConversationId;
use suzatary_core::secretary::model::{SessionId, ToolCall, Turn};
use suzatary_core::secretary::queue;
use suzatary_core::secretary::store::{Entry, Scope, Stored};
use suzatary_core::secretary::taint::{self, Taint};

#[derive(Debug, Clone)]
enum Step {
    /// The owner says something.
    Owner,
    /// The model calls a verb that reads mail, and its answer is stored.
    Read,
    /// The model calls a clean verb, and its answer is stored.
    Weather,
    /// The model is asked and answers, closing the round.
    Ask(Asking),
    /// The owner stops the round.
    Stop,
    /// The owner rewinds to a turn, as a share of the conversation.
    Rewind(u8),
}

#[derive(Debug, Clone, Copy)]
enum Asking {
    /// A model with no session, sent its window.
    Window,
    /// A session, resumed or opened; when opened, the owner may say up to four turns before its
    /// line is written, and the line may be one written before its first turn was recorded.
    Session { concurrent: usize, legacy: bool },
    /// The manual or the verbs changed, so a fresh session opens whether one is kept or not.
    Reopen,
    /// The kept session is lost and will not reopen: one stateless call, sent its window, answers
    /// instead, and the session it did not hear ends.
    Lost,
    /// A job told no memory, sent every stored turn.
    Transcript,
}

/// A step, drawn so a session often outlives its window: a stop or a rewind about once in
/// twenty-five steps, a read about once in thirty.
fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        40 => Just(Step::Owner),
        3 => Just(Step::Read),
        6 => Just(Step::Weather),
        4 => Just(Step::Ask(Asking::Window)),
        40 => (0..5usize, proptest::bool::weighted(0.1))
            .prop_map(|(concurrent, legacy)| Step::Ask(Asking::Session { concurrent, legacy })),
        1 => Just(Step::Ask(Asking::Reopen)),
        1 => Just(Step::Ask(Asking::Lost)),
        1 => Just(Step::Ask(Asking::Transcript)),
        2 => Just(Step::Stop),
        2 => any::<u8>().prop_map(Step::Rewind),
    ]
}

fn id() -> ConversationId {
    ConversationId::new("default").expect("a slug")
}

fn at() -> suzatary_core::capability::Datetime {
    suzatary_core::capability::Datetime::new(2026, 10, 7, 9, 0, 0, 0).expect("an instant")
}

fn said(turn: Turn) -> Entry {
    let by = matches!(turn, Turn::Person { .. }).then(|| "ada".into());
    Entry::Said { at: at(), turn, by }
}

fn calling(verb: &str) -> Entry {
    said(Turn::Model {
        text: String::new(),
        calls: vec![ToolCall {
            verb: verb.into(),
            args: std::collections::BTreeMap::new(),
        }],
    })
}

fn answered(verb: &str) -> Entry {
    said(Turn::Tool {
        verb: verb.into(),
        answer: "{}".to_owned(),
        outcome: suzatary_core::secretary::outcome::Outcome::Answered,
        files: Vec::new(),
    })
}

fn owner(n: usize) -> Entry {
    said(Turn::Person {
        text: format!("the owner says {n}"),
        references: Vec::new(),
        attachments: Vec::new(),
    })
}

/// The conversation as the box folds it.
struct Conversation {
    lines: Vec<Entry>,
}

impl Conversation {
    fn new() -> Self {
        Self {
            lines: vec![Entry::Opened {
                at: at(),
                title: suzatary_core::secretary::words::Words::kept("t".to_owned()),
                model: "m".to_owned(),
                kind: suzatary_core::secretary::store::Kind::Topic,
            }],
        }
    }

    fn stored(&self) -> Stored {
        Stored::of(&id(), &self.lines).expect("a conversation")
    }

    fn push(&mut self, line: Entry) {
        self.lines.push(line);
    }
}

fn kept(sessions: usize, from: usize) -> Entry {
    Entry::Kept {
        at: at(),
        session: SessionId::minted(&format!("default/{sessions}")),
        model: "m".to_owned(),
        cwd: String::new(),
        surface: None,
        from,
    }
}

fn indices(sent: &[(usize, &suzatary_core::secretary::store::Said)]) -> BTreeSet<usize> {
    sent.iter().map(|(at, _)| *at).collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// Whatever tainted turn a model was sent and still holds, `remember` is a card.
    #[test]
    fn a_model_holding_a_tainted_turn_makes_remember_a_card(
        script in proptest::collection::vec(step(), 1..180),
    ) {
        let mut conversation = Conversation::new();
        // What the live session was sent since it opened; `None` while no live session is kept.
        let mut session: Option<BTreeSet<usize>> = None;
        let mut sessions = 0;
        for (n, step) in script.iter().enumerate() {
            match *step {
                Step::Owner => conversation.push(owner(n)),
                Step::Read => {
                    conversation.push(calling("show"));
                    conversation.push(answered("show"));
                }
                Step::Weather => {
                    conversation.push(calling("weather"));
                    conversation.push(answered("weather"));
                }
                Step::Stop => {
                    conversation.push(Entry::Stopped { at: at(), by: "ada".to_owned() });
                    session = None;
                }
                Step::Rewind(share) => {
                    let turns = conversation.stored().turns.len();
                    conversation.push(Entry::Rewound {
                        to: turns * usize::from(share) / 255,
                        at: at(),
                        by: "ada".to_owned(),
                    });
                    session = None;
                }
                Step::Ask(asking) => {
                    let stored = conversation.stored();
                    prop_assert_eq!(stored.session.is_some(), session.is_some());
                    // A stateless answer is unheard by the kept session, which ends.
                    let unheard = |conversation: &mut Conversation, session: &mut Option<BTreeSet<usize>>| {
                        if let Some(kept) = &stored.session {
                            conversation.push(Entry::Unheard {
                                at: at(),
                                session: kept.session.id.clone(),
                                model: "a stateless model".to_owned(),
                            });
                        }
                        *session = None;
                    };
                    let window = indices(&queue::for_model(&stored));
                    let first = queue::for_model(&stored)
                        .first()
                        .map_or(stored.turns.len(), |(at, _)| *at);
                    let mut open = |conversation: &mut Conversation, concurrent: usize, legacy: bool| {
                        for _ in 0..concurrent {
                            conversation.push(owner(n));
                        }
                        sessions += 1;
                        conversation.push(if legacy {
                            Entry::Session {
                                at: at(),
                                session: SessionId::minted(&format!("default/{sessions}")),
                                model: "m".to_owned(),
                                cwd: String::new(),
                                surface: None,
                            }
                        } else {
                            kept(sessions, first)
                        });
                    };
                    let (holds, scope): (BTreeSet<usize>, Scope) = match asking {
                        Asking::Window => {
                            unheard(&mut conversation, &mut session);
                            (window, Scope::Window)
                        }
                        Asking::Transcript => {
                            unheard(&mut conversation, &mut session);
                            ((0..stored.turns.len()).collect(), Scope::Transcript)
                        }
                        Asking::Lost if session.is_some() => {
                            unheard(&mut conversation, &mut session);
                            (window, Scope::Window)
                        }
                        Asking::Session { .. } if session.is_some() => {
                            let sent = session.as_mut().expect("a live session");
                            sent.extend(indices(&queue::delta(&stored)));
                            (sent.clone(), Scope::Window)
                        }
                        Asking::Session { concurrent, legacy } => {
                            open(&mut conversation, concurrent, legacy);
                            session = Some(window.clone());
                            (window, Scope::Window)
                        }
                        Asking::Lost | Asking::Reopen => {
                            open(&mut conversation, 0, false);
                            session = Some(window.clone());
                            (window, Scope::Window)
                        }
                    };
                    if conversation.stored().scope != scope {
                        conversation.push(Entry::Scoped { at: at(), scope });
                    }
                    let now = conversation.stored();
                    let read = holds
                        .iter()
                        .find(|at| taint::turn(&now.turns[**at]) == Taint::Tainted);
                    if let Some(read) = read {
                        prop_assert!(
                            taint::tainted(&now),
                            "the model holds a read at {}, and remember would write", read
                        );
                    }
                    conversation.push(said(Turn::Model {
                        text: "done".to_owned(),
                        calls: Vec::new(),
                    }));
                    if read.is_some() {
                        prop_assert_eq!(
                            taint::rounds(&id(), &conversation.lines).last().copied(),
                            Some(Taint::Tainted),
                            "the answer of a model holding a read is labelled tainted"
                        );
                    }
                }
            }
        }
    }
}
