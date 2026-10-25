use super::*;
use crate::capability::{Datetime, SessionId, Turn};
use crate::reference::Reference;
use crate::secretary::responded::{Done, Responded, Tried, Withdrawal};

fn id() -> ConversationId {
    ConversationId::new("2026-09-09-1").expect("a slug")
}

fn draft() -> Draft {
    Draft {
        account: "ada@example.com".to_owned(),
        to: vec!["counsel@example.test".to_owned()],
        cc: Vec::new(),
        subject: "The permit".to_owned(),
        body: "Sending the documents today.".to_owned(),
        attachments: Vec::new(),
    }
}

fn reasons() -> Reasons {
    Reasons::new(
        "He asked for the documents on Tuesday and the last scan is done.",
        vec![
            "mail:a1b2c3d4e5f6".parse().expect("a reference"),
            "todo:life/permitfiling".parse().expect("a reference"),
        ],
    )
    .expect("reasons")
}

fn object() -> crate::secretary::inbox::ProposalId {
    crate::secretary::inbox::ProposalId::new("p0123456789abcdef0123456789abcdef").expect("an id")
}

fn proposed(at: &str) -> Entry {
    Entry::Proposed {
        at: instant(at),
        object: object(),
        revises: None,
    }
}

/// An inbox holding [`object`], asked as an email, answered `responses`.
fn inbox_of(responses: &[Responded]) -> crate::secretary::inbox::Fold {
    let asked = crate::secretary::inbox::Asked {
        proposer: crate::secretary::proposer::Proposer::Secretary,
        proposal: Proposal::Email(draft()),
        reasons: reasons(),
        asked: "proposal:2026-09-09-1/0".parse().expect("a reference"),
        price: crate::secretary::inbox::Price::unpriced(),
    };
    let by = prodrome_core::event::Actor::new("ada").expect("an actor");
    let mut events = vec![crate::secretary::inbox_event::InboxEvent {
        proposal: object(),
        at: now(),
        actor: prodrome_core::event::Actor::new("assistant").expect("an actor"),
        said: crate::secretary::inbox_event::Said::Asked(Box::new(asked)),
    }];
    events.extend(
        responses
            .iter()
            .map(|responded| crate::secretary::inbox_event::InboxEvent {
                proposal: object(),
                at: now(),
                actor: by.clone(),
                said: crate::secretary::inbox_event::Said::Responded(responded.clone()),
            }),
    );
    crate::secretary::inbox::Fold::of_chain(&events)
}

fn changeset() -> crate::secretary::changes::Changeset {
    crate::secretary::changes::Changeset::new(
        "life",
        vec![crate::secretary::changes::Act::Complete {
            todo: "permitfiling".to_owned(),
            note: "filed".to_owned(),
        }],
    )
    .expect("one act")
}

fn opened() -> Entry {
    Entry::Opened {
        at: crate::capability::Datetime::new(2026, 9, 9, 9, 0, 0, 0).expect("an instant"),
        title: crate::secretary::words::Words::kept("Morning".to_owned()),
        model: "a-cli".to_owned(),
        kind: crate::secretary::store::Kind::Topic,
    }
}

/// Some instant on the same morning as [`opened`], for `unanswered`'s `now`.
fn now() -> Datetime {
    Datetime::new(2026, 9, 9, 9, 5, 0, 0).expect("a real instant")
}

fn act() -> crate::secretary::acts::Act {
    crate::secretary::acts::Act::Auth {
        account: crate::mail::address::EmailAddress::new("ada@example.com")
            .expect("a shaped address"),
    }
}

fn fired(id: &str) -> crate::coding::Session {
    crate::coding::Session::Fired {
        id: id.parse().expect("a session id"),
        url: format!("https://claude.ai/code/{id}")
            .parse()
            .expect("a url"),
        at: "2026-09-09T09:06:00".to_owned(),
    }
}

/// A coding line folds by its proposal's index, the last winning, and one naming a proposal
/// nothing made is refused.
#[test]
fn a_coding_line_folds_last_wins_by_index_and_refuses_a_phantom() {
    let coding = |proposal, session| Entry::Coding {
        at: crate::capability::Datetime::new(2026, 9, 9, 9, 6, 0, 0).expect("an instant"),
        proposal,
        session,
    };
    let dropped = fired("session_2")
        .step(crate::coding::Step::Drop {
            reason: "not now".to_owned(),
        })
        .expect("drops");
    let stored = Stored::of(
        &id(),
        &[
            opened(),
            proposed("2026-09-09T09:05:00"),
            coding(0, fired("session_1")),
            coding(0, dropped.clone()),
        ],
    )
    .expect("folds");
    assert_eq!(stored.coding.get(&0), Some(&dropped));
    assert_eq!(
        Stored::of(&id(), &[opened(), coding(0, fired("session_1"))]),
        Err(NotAConversation::Phantom { id: id(), index: 0 })
    );
}

/// A name is a path segment and a file stem.
#[test]
fn a_conversation_id_is_a_slug() {
    for good in ["2026-09-09-1", "brief", "a", "9", &"a".repeat(64)] {
        assert!(ConversationId::new(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "..",
        "-lead",
        "Brief",
        "has space",
        "has/slash",
        "has_underscore",
        "%2e%2e",
        "memory",
        &"a".repeat(65),
    ] {
        assert!(ConversationId::new(bad).is_err(), "{bad:?}");
    }
}

/// A file without an opening line was not written by this program.
#[test]
fn a_conversation_without_an_opening_line_is_refused() {
    assert_eq!(
        Stored::of(&id(), &[]).unwrap_err(),
        NotAConversation::Unopened(id())
    );
}

/// An email proposal names itself, and its sources are their text form.
#[test]
fn an_email_proposal_round_trips_and_names_itself() {
    let proposal = Proposal::Email(draft());
    let printed = serde_json::to_string(&proposal).expect("serialises");
    assert!(printed.contains(r#""act":"email""#), "{printed}");
    let read: Proposal = serde_json::from_str(&printed).expect("and reads back");
    assert_eq!(read, proposal);
    let printed = serde_json::to_string(&reasons()).expect("serialises");
    assert!(printed.contains(r#""sources":["mail:a1b2c3d4e5f6","todo:life/permitfiling"]"#));
}

/// A line names its object, and the standing is the two joined: the act, reasons and responses
/// from the inbox, the position and replacement from the conversation.
#[test]
fn a_standing_joins_the_line_with_its_object() {
    let sent = Done::Sent {
        recipients: vec!["counsel@example.test".to_owned()],
        message_id: "<x@box.example>".to_owned(),
    };
    let stored =
        Stored::of(&id(), &[opened(), proposed("2026-09-09T09:02:00")]).expect("a conversation");
    let standing = stored
        .standing(0, &inbox_of(&[Responded::Done(sent.clone())]))
        .expect("proposal 0");
    assert_eq!(standing.object.asked.proposal, Proposal::Email(draft()));
    assert_eq!(standing.object.asked.reasons, reasons());
    assert_eq!(
        standing
            .object
            .reading
            .shown()
            .map(|answer| &answer.response),
        Some(&Responded::Done(sent))
    );
    assert_eq!(standing.asking.after_turn, 0);
    assert_eq!(
        stored
            .standing(1, &inbox_of(&[]))
            .expect_err("nothing proposed it")
            .status(),
        crate::refusal::Status::NotFound
    );
    assert_eq!(
        stored
            .standing(0, &crate::secretary::inbox::Fold::default())
            .expect_err("no object")
            .status(),
        crate::refusal::Status::Conflict
    );
}

/// A changeset proposal round trips, and names itself as `"changes"`.
#[test]
fn a_changeset_proposal_round_trips_and_names_itself() {
    let proposal = Proposal::Changes(changeset());
    let printed = serde_json::to_string(&proposal).expect("serialises");
    assert!(printed.contains(r#""act":"changes""#), "{printed}");
    assert!(printed.contains(r#""chain":"life""#), "{printed}");
    assert!(printed.contains(r#""act":"complete""#), "{printed}");
    let read: Proposal = serde_json::from_str(&printed).expect("and reads back");
    assert_eq!(read, proposal);
}

/// An act proposal round trips, and names itself as `"act"`.
#[test]
fn an_act_proposal_round_trips_and_names_itself() {
    let proposal = Proposal::Act(act());
    let printed = serde_json::to_string(&proposal).expect("serialises");
    assert!(printed.contains(r#""act":"act""#), "{printed}");
    assert!(printed.contains(r#""kind":"auth""#), "{printed}");
    assert!(
        printed.contains(r#""account":"ada@example.com""#),
        "{printed}"
    );
    let read: Proposal = serde_json::from_str(&printed).expect("and reads back");
    assert_eq!(read, proposal);
}

/// A proposal cannot be built without its reasons and sources.
#[test]
fn a_proposal_with_no_why_or_no_sources_is_not_a_proposal() {
    let source: Reference = "mail:a1b2c3d4e5f6".parse().expect("a reference");
    assert_eq!(
        Reasons::new("  ", vec![source.clone()]).unwrap_err(),
        NoReasons::Silent
    );
    assert_eq!(
        Reasons::new("because", Vec::new()).unwrap_err(),
        NoReasons::Groundless
    );
    assert!(Reasons::new("because", vec![source]).is_ok());
}

/// A hand-edited line goes through the same constructor: `serde` parses into [`Reasons`] rather
/// than filling its fields.
#[test]
fn reasons_are_parsed_when_they_are_deserialised() {
    assert!(serde_json::from_str::<Reasons>(r#"{"why":"x","sources":[]}"#).is_err());
    assert!(
        serde_json::from_str::<Reasons>(r#"{"why":"","sources":["mail:a1b2c3d4e5f6"]}"#).is_err()
    );
    assert!(serde_json::from_str::<Reasons>(r#"{"why":"x","sources":["nowhere:1"]}"#).is_err());
    let read: Reasons =
        serde_json::from_str(r#"{"why":"x","sources":["mail:a1b2c3d4e5f6"]}"#).expect("reasons");
    assert_eq!(read.why(), "x");
    assert_eq!(read.sources().len(), 1);
}

/// An unheard turn ends the session it names and no other: a session opened after it heard
/// the turn, so a line about the one before changes nothing.
#[test]
fn an_unheard_turn_ends_only_the_session_it_names() {
    let first = SessionId::new("96f709a3-43d7-4453-bf59-6fb5291b5a3c").expect("a uuid");
    let second = SessionId::new("11111111-2222-4333-8444-555555555555").expect("a uuid");
    let kept = |at: &str, id: &SessionId| Entry::Kept {
        at: instant(at),
        session: id.clone(),
        model: "a-cli/small".to_owned(),
        cwd: "/home/ada/Documents/Suzatary".to_owned(),
        surface: None,
        from: 0,
    };
    let unheard = |id: &SessionId| Entry::Unheard {
        at: crate::capability::Datetime::new(2026, 9, 13, 9, 3, 0, 0).expect("an instant"),
        session: id.clone(),
        model: "openai-compatible/deepseek-v4-flash".to_owned(),
    };
    let ended = Stored::of(
        &id(),
        &[
            opened(),
            kept("2026-09-13T09:01:00", &first),
            unheard(&first),
        ],
    )
    .expect("a conversation");
    assert!(ended.session.is_none());
    let standing = Stored::of(
        &id(),
        &[
            opened(),
            kept("2026-09-13T09:01:00", &first),
            kept("2026-09-13T09:05:00", &second),
            unheard(&first),
        ],
    )
    .expect("a conversation");
    assert_eq!(standing.session.map(|kept| kept.session.id), Some(second));
}

/// The last session line wins, first ever or minted after cold recovery.
#[test]
fn the_last_session_line_wins() {
    let first = SessionId::new("96f709a3-43d7-4453-bf59-6fb5291b5a3c").expect("a uuid");
    let second = SessionId::new("11111111-2222-4333-8444-555555555555").expect("a uuid");
    let entries = vec![
        opened(),
        Entry::Session {
            at: crate::capability::Datetime::new(2026, 9, 13, 9, 1, 0, 0).expect("an instant"),
            session: first.clone(),
            model: "a-cli/small".to_owned(),
            cwd: "/home/ada/Documents/Suzatary".to_owned(),
            surface: None,
        },
        Entry::Session {
            at: crate::capability::Datetime::new(2026, 9, 13, 9, 5, 0, 0).expect("an instant"),
            session: second.clone(),
            model: "a-cli/small".to_owned(),
            cwd: "/home/ada/Documents/Suzatary".to_owned(),
            surface: None,
        },
    ];
    let stored = Stored::of(&id(), &entries).expect("a conversation");
    let session = stored.session.expect("a session");
    assert_eq!(session.session.id, second);
    assert_ne!(session.session.id, first);
    assert_eq!(session.session.created, "2026-09-13T09:05:00");
}

/// A stop reads as the owner's act and ends the session: the killed process holds a question it
/// never answered, so nothing resumes it.
#[test]
fn a_stop_is_an_act_and_ends_the_session() {
    let entries = vec![
        opened(),
        Entry::Session {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 30, 0).expect("an instant"),
            session: SessionId::new("96f709a3-43d7-4453-bf59-6fb5291b5a3c").expect("a uuid"),
            model: "a-cli/small".to_owned(),
            cwd: "/srv".to_owned(),
            surface: None,
        },
        Entry::Said {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 1, 0, 0).expect("an instant"),
            turn: Turn::Person {
                text: "read everything".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        },
        Entry::Stopped {
            at: crate::capability::Datetime::new(2026, 9, 25, 9, 1, 30, 0).expect("an instant"),
            by: "ada".to_owned(),
        },
    ];
    let stored = Stored::of(&id(), &entries).expect("a conversation");
    assert!(stored.session.is_none(), "a stopped session is not resumed");
    let last = stored.turns.last().expect("the act");
    assert_eq!(
        last.turn,
        Turn::Act {
            text: stopped_by("ada"),
            references: Vec::new(),
        }
    );
    assert_eq!(last.by, Some("ada".into()));
    assert_eq!(state(&stored), ConversationState::Idle);
}

fn said_by(at: &str, turn: Turn) -> Entry {
    Entry::Said {
        at: instant(at),
        turn,
        by: Some("ada".into()),
    }
}

fn question(text: &str) -> Turn {
    Turn::Person {
        text: text.to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
    }
}

fn answer(text: &str) -> Turn {
    Turn::Model {
        text: text.to_owned(),
        calls: Vec::new(),
    }
}

fn calling(verb: &str) -> Turn {
    Turn::Model {
        text: String::new(),
        calls: vec![crate::capability::ToolCall {
            verb: verb.into(),
            args: std::collections::BTreeMap::new(),
        }],
    }
}

fn answered_by(verb: &str, refused: bool) -> Turn {
    Turn::Tool {
        verb: verb.into(),
        answer: "{}".to_owned(),
        outcome: if refused {
            crate::secretary::outcome::Outcome::Refused
        } else {
            crate::secretary::outcome::Outcome::Answered
        },
        files: Vec::new(),
    }
}

fn rewound(to: usize) -> Entry {
    Entry::Rewound {
        to,
        at: crate::capability::Datetime::new(2026, 9, 25, 10, 0, 0, 0).expect("an instant"),
        by: "ada".to_owned(),
    }
}

/// A round that only read: asked, searched, answered.
fn one_round() -> Vec<Entry> {
    vec![
        opened(),
        said_by("t1", question("what changed")),
        said_by("t2", calling("search")),
        said_by("t3", answered_by("search", false)),
        said_by("t4", answer("nothing much")),
    ]
}

/// A regenerate sets the trace and answer aside: the question is pending again, every index
/// still names its turn, and the session is gone.
#[test]
fn a_regenerate_leaves_the_question_pending_on_the_live_path() {
    let stored = Stored::of(&id(), &one_round()).expect("a conversation");
    let to = stored
        .rewind(Rewind::Regenerate)
        .expect("a round that only read");
    assert_eq!(to, 1);
    let mut entries = one_round();
    entries.push(rewound(to));
    let after = Stored::of(&id(), &entries).expect("a conversation");
    assert_eq!(after.turns.len(), 4, "nothing leaves the file");
    assert_eq!(
        after
            .live()
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(state(&after), ConversationState::Queued);
    assert_eq!(
        crate::secretary::queue::delta(&after)
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>(),
        vec![0],
        "the question is asked again"
    );
    assert!(after.session.is_none());
}

/// An edit sets the question aside too, and the new one is all the queue holds.
#[test]
fn an_edit_sets_the_question_aside_and_queues_the_new_one() {
    let stored = Stored::of(&id(), &one_round()).expect("a conversation");
    let to = stored.rewind(Rewind::Edit).expect("a question of his");
    assert_eq!(to, 0);
    let mut entries = one_round();
    entries.push(rewound(to));
    entries.push(said_by("t5", question("what changed today")));
    let after = Stored::of(&id(), &entries).expect("a conversation");
    let delta = crate::secretary::queue::delta(&after);
    assert_eq!(delta.len(), 1);
    assert_eq!(delta[0].0, 4);
    assert!(!after.is_live(0) && !after.is_live(3) && after.is_live(4));
}

/// Only a finished last round is asked again, and a second rewind of it starts from its live
/// path.
#[test]
fn a_rewind_waits_for_the_answer_and_follows_the_live_path() {
    let mut running = one_round();
    running.push(said_by("t5", question("and the permit?")));
    let stored = Stored::of(&id(), &running).expect("a conversation");
    assert!(stored.rewind(Rewind::Regenerate).is_err(), "still queued");
    assert!(
        Stored::of(&id(), &[opened()])
            .expect("empty")
            .rewind(Rewind::Edit)
            .is_err()
    );

    let mut twice = one_round();
    twice.push(rewound(1));
    twice.push(said_by("t6", answer("still nothing")));
    let stored = Stored::of(&id(), &twice).expect("a conversation");
    assert_eq!(stored.rewind(Rewind::Regenerate), Ok(4));
    assert_eq!(stored.rewind(Rewind::Edit), Ok(0));
}

/// A round stopped by the stop's line alone may be asked again, as one stored with an empty
/// end after its stop is: the stop is not a write, and it closes the round.
#[test]
fn a_round_stopped_in_one_line_may_be_asked_again() {
    let entries = vec![
        opened(),
        said_by("t1", question("read every thread")),
        Entry::Stopped {
            at: instant("t2"),
            by: "ada".to_owned(),
        },
    ];
    let stored = Stored::of(&id(), &entries).expect("a conversation");
    assert_eq!(stored.rewind(Rewind::Regenerate), Ok(1));
    assert_eq!(stored.rewind(Rewind::Edit), Ok(0));
    let mut again = entries.clone();
    again.push(rewound(1));
    let again = Stored::of(&id(), &again).expect("a conversation");
    assert_eq!(
        state(&again),
        ConversationState::Queued,
        "the question is asked again"
    );
}

/// A stopped round may be asked again: the stop is not a write.
#[test]
fn a_stopped_round_may_be_asked_again() {
    let entries = vec![
        opened(),
        said_by("t1", question("read every thread")),
        Entry::Stopped {
            at: instant("t2"),
            by: "ada".to_owned(),
        },
        said_by("t3", answer("")),
    ];
    let stored = Stored::of(&id(), &entries).expect("a conversation");
    assert_eq!(stored.rewind(Rewind::Regenerate), Ok(1));
}

/// A stop is known by its line, never its words: stopped by another persona it is still no
/// write, and an act that merely reads like a stop still is one.
#[test]
fn a_stop_is_known_by_its_line_not_its_words() {
    let entries = vec![
        opened(),
        said_by("t1", question("read every thread")),
        Entry::Stopped {
            at: instant("t2"),
            by: "diego".to_owned(),
        },
        said_by("t3", answer("")),
    ];
    let mut stored = Stored::of(&id(), &entries).expect("a conversation");
    assert_eq!(stored.stops, vec![1]);
    stored.turns[1].turn = Turn::Act {
        text: "Diego stopped the answer".to_owned(),
        references: Vec::new(),
    };
    assert_eq!(stored.rewind(Rewind::Regenerate), Ok(1));

    let posing = vec![
        opened(),
        said_by("t1", question("read every thread")),
        said_by(
            "t2",
            Turn::Act {
                text: stopped_by("ada"),
                references: Vec::new(),
            },
        ),
        said_by("t3", answer("")),
    ];
    let stored = Stored::of(&id(), &posing).expect("a conversation");
    assert!(stored.rewind(Rewind::Regenerate).is_err());
}

/// A rewind past the end of the file is not a conversation.
#[test]
fn a_rewind_to_a_turn_never_said_is_refused() {
    let mut entries = one_round();
    entries.push(rewound(9));
    assert!(matches!(
        Stored::of(&id(), &entries),
        Err(NotAConversation::Unsaid { to: 9, .. })
    ));
}

/// One step of a random conversation, for the rewind law.
#[derive(Debug, Clone, Copy)]
enum Step {
    Ask,
    Read { refused: bool },
    Write { verb: usize, refused: bool },
    Propose,
    Answer,
    Stop { stored_shape: bool },
    Again(Rewind),
}

const WRITES: [&str; 5] = [
    "todo_add",
    "record",
    "remember",
    "propose_email",
    "triage_withdraw",
];

fn steps() -> impl proptest::strategy::Strategy<Value = Vec<Step>> {
    use proptest::prelude::*;
    proptest::collection::vec(
        prop_oneof![
            3 => Just(Step::Ask),
            2 => any::<bool>().prop_map(|refused| Step::Read { refused }),
            2 => (0..WRITES.len(), any::<bool>())
                .prop_map(|(verb, refused)| Step::Write { verb, refused }),
            1 => Just(Step::Propose),
            3 => Just(Step::Answer),
            1 => any::<bool>().prop_map(|stored_shape| Step::Stop { stored_shape }),
            2 => Just(Step::Again(Rewind::Regenerate)),
            2 => Just(Step::Again(Rewind::Edit)),
        ],
        0..48,
    )
}

proptest::proptest! {
    /// A rewind never hides a write: over any interleaving, every verb that wrote and every
    /// proposal stays on the live path, and every allowed rewind leaves a question pending.
    #[test]
    fn a_rewind_never_hides_a_write(script in steps()) {
        let mut entries = vec![opened()];
        for (at, step) in script.into_iter().enumerate() {
            let at = format!("t{at}");
            match step {
                Step::Ask => entries.push(said_by(&at, question("q"))),
                Step::Read { refused } => {
                    entries.push(said_by(&at, calling("search")));
                    entries.push(said_by(&at, answered_by("search", refused)));
                }
                Step::Write { verb, refused } => {
                    entries.push(said_by(&at, calling(WRITES[verb])));
                    entries.push(said_by(&at, answered_by(WRITES[verb], refused)));
                }
                Step::Propose => {
                    entries.push(said_by(&at, calling("propose_email")));
                    entries.push(proposed(&at));
                    entries.push(said_by(&at, answered_by("propose_email", false)));
                }
                Step::Answer => entries.push(said_by(&at, answer("a"))),
                Step::Stop { stored_shape } => {
                    entries.push(Entry::Stopped { at: instant(&at), by: "ada".to_owned() });
                    if stored_shape {
                        entries.push(said_by(&at, answer("")));
                    }
                }
                Step::Again(how) => {
                    let stored = Stored::of(&id(), &entries).expect("a conversation");
                    if let Ok(to) = stored.rewind(how) {
                        entries.push(rewound(to));
                        if how == Rewind::Edit {
                            entries.push(said_by(&at, question("q again")));
                        }
                        let after = Stored::of(&id(), &entries).expect("a conversation");
                        proptest::prop_assert_eq!(state(&after), ConversationState::Queued);
                    }
                }
            }
        }
        let stored = Stored::of(&id(), &entries).expect("a conversation");
        for (index, said) in stored.turns.iter().enumerate() {
            if let Turn::Tool { verb, outcome, .. } = &said.turn
                && !outcome.refused()
                && WRITES.contains(&verb.as_str())
            {
                proptest::prop_assert!(stored.is_live(index), "the write at {} was hidden", index);
            }
        }
        for standing in &stored.proposals {
            proptest::prop_assert!(
                stored.is_live(standing.after_turn - 1),
                "a proposal after turn {} was hidden",
                standing.after_turn - 1
            );
        }
    }
}

/// An id is parsed on the way in, wherever it comes from, including a stored line somebody
/// could have edited.
#[test]
fn an_id_is_parsed_when_it_is_deserialised() {
    assert!(serde_json::from_str::<ConversationId>(r#""../etc""#).is_err());
    assert_eq!(
        serde_json::from_str::<ConversationId>(r#""brief-1""#)
            .expect("a slug")
            .as_str(),
        "brief-1"
    );
}

/// The last name wins, so a rename is an append; an unnamed topic heads its row "New topic".
#[test]
fn the_last_title_wins() {
    let untitled = Entry::Opened {
        at: crate::capability::Datetime::new(2026, 9, 25, 9, 0, 0, 0).expect("an instant"),
        title: crate::secretary::words::Words::kept(String::new()),
        model: "m".to_owned(),
        kind: Kind::Topic,
    };
    let titled = |title: &str, by: Namer| Entry::Titled {
        at: crate::capability::Datetime::new(2026, 9, 25, 9, 5, 0, 0).expect("an instant"),
        title: crate::secretary::words::Words::kept(Title::new(title).expect("a title")),
        by,
    };
    let bare = Stored::of(&id(), std::slice::from_ref(&untitled)).expect("a conversation");
    assert_eq!(bare.heading(), "New topic");
    let entries = vec![
        untitled,
        titled(
            "Permit filing",
            Namer::Job {
                model: "m".to_owned(),
            },
        ),
        titled(
            "The permit",
            Namer::Person {
                actor: "ada".to_owned(),
            },
        ),
    ];
    let stored = Stored::of(&id(), &entries).expect("a conversation");
    assert_eq!(stored.title.get().map(String::as_str), Some("The permit"));
    assert_eq!(stored.heading(), "The permit");
}

// --- the responses and the one reading of them -------------------------

/// One response of each arm; [`arm`] is an exhaustive match, so a new arm fails to compile
/// until it is listed here.
fn every_response() -> Vec<Responded> {
    vec![
        Responded::Done(Done::Applied {
            objects: vec!["a1b2".to_owned()],
            chain: "life".to_owned(),
        }),
        Responded::Done(Done::Sent {
            recipients: vec!["counsel@example.test".to_owned()],
            message_id: "<m@box.example>".to_owned(),
        }),
        Responded::Tried {
            attempt: 1,
            tried: Tried::NotSent {
                reason: "auth: 535".to_owned(),
            },
        },
        Responded::Done(Done::MayHaveGone {
            reason: "the connection closed after the final dot".to_owned(),
        }),
        Responded::Dismissed {
            reason: "wrong person".to_owned(),
        },
        Responded::Tried {
            attempt: 1,
            tried: Tried::Running {
                began: "2026-09-27T09:00:00".to_owned(),
                expires_at: "2026-09-27T09:15:00".to_owned(),
            },
        },
        Responded::Done(Done::Acted {
            what: "the grant is stored".to_owned(),
        }),
        Responded::Checked(()),
        Responded::Withdrawn(Withdrawal::Superseded),
        Responded::Withdrawn(Withdrawal::NoLongerNeeded {
            why: "she replied first".to_owned(),
        }),
    ]
}

fn arm(response: &Responded) -> usize {
    match response {
        Responded::Done(Done::Applied { .. }) => 0,
        Responded::Done(Done::Sent { .. }) => 1,
        Responded::Tried {
            tried: Tried::NotSent { .. },
            ..
        } => 2,
        Responded::Done(Done::MayHaveGone { .. }) => 3,
        Responded::Dismissed { .. } => 4,
        Responded::Tried {
            tried: Tried::Running { .. },
            ..
        } => 5,
        Responded::Done(Done::Acted { .. }) => 6,
        Responded::Checked(()) => 7,
        Responded::Withdrawn(Withdrawal::Superseded) => 8,
        Responded::Withdrawn(Withdrawal::NoLongerNeeded { .. }) => 9,
    }
}

#[test]
fn every_response_is_every_arm() {
    let arms: std::collections::BTreeSet<usize> = every_response().iter().map(arm).collect();
    assert_eq!(arms, (0..10).collect());
}

/// A proposal waits exactly until the inbox records a settled response, except a send that
/// may have gone, which waits for the owner's check; while it waits there is something to
/// press, its own flow, or his check.
#[test]
fn a_proposal_waits_exactly_until_its_completion_is_recorded() {
    let standing = inbox_of(&[]);
    let reading = &standing.get(&object()).expect("held").reading;
    assert!(reading.open() && reading.performs());
    let uncertain = Responded::Done(Done::MayHaveGone {
        reason: "the connection closed after the final dot".to_owned(),
    });
    let mut logs: Vec<Vec<Responded>> = every_response()
        .into_iter()
        .filter(|response| !matches!(response, Responded::Checked(())))
        .map(|response| vec![response])
        .collect();
    logs.push(vec![uncertain, Responded::Checked(())]);
    for log in logs {
        let fold = inbox_of(&log);
        let reading = &fold.get(&object()).expect("held").reading;
        let ends = log.iter().any(|response| response.phase().settled());
        let shown = reading.shown().map(|answer| &answer.response);
        let unchecked = matches!(shown, Some(Responded::Done(Done::MayHaveGone { .. })));
        let running = matches!(
            shown,
            Some(Responded::Tried {
                tried: Tried::Running { .. },
                ..
            })
        );
        assert_eq!(reading.open(), !ends || unchecked, "{log:?}");
        assert!(
            !reading.open() || reading.performs() || unchecked || running,
            "a waiting proposal with nothing to press: {log:?}"
        );
    }
}

/// A check reads as the send that may have gone beneath it, and above anything else it is no
/// reading at all.
#[test]
fn a_check_reads_as_the_send_it_checks() {
    for response in every_response() {
        let log = [response.clone(), Responded::Checked(())];
        match &response {
            Responded::Done(Done::MayHaveGone { reason }) => {
                let fold = inbox_of(&log);
                let reading = &fold.get(&object()).expect("held").reading;
                assert!(matches!(
                    reading.shown().map(|answer| &answer.response),
                    Some(Responded::Checked(check)) if check.reason() == reason
                ));
                assert!(!reading.open() && !reading.performs());
            }
            _ => assert_eq!(
                inbox_of(&log).get(&object()).map(|object| &object.reading),
                Some(&crate::secretary::inbox_reading::Reading::Unreadable(
                    crate::secretary::inbox::Unchecked
                )),
                "{response:?}"
            ),
        }
    }
}

/// A failure with nothing sent is performed again; one that may have gone is a completion.
#[test]
fn a_failed_proposal_is_retried_only_when_nothing_went() {
    let stored =
        Stored::of(&id(), &[opened(), proposed("2026-09-27T09:00:00")]).expect("a conversation");
    let tried = |response: Responded| {
        stored
            .standing(0, &inbox_of(&[response]))
            .expect("proposal 0")
    };
    let not_sent = tried(Responded::Tried {
        attempt: 1,
        tried: Tried::NotSent {
            reason: "auth: 535".to_owned(),
        },
    });
    assert!(not_sent.object.reading.performs());
    let unknown = tried(Responded::Done(Done::MayHaveGone {
        reason: "the connection closed after the final dot".to_owned(),
    }));
    assert!(!unknown.object.reading.performs());
    assert!(
        unknown.object.reading.open(),
        "it waits on the owner's check"
    );
    let refusal = refused(0, &unknown.object.reading, now());
    assert!(
        refusal.message().contains("may have gone through"),
        "{}",
        refusal.message()
    );
}
