//! `answer`'s session paths end to end over `Fakes`: open, resume with the delta, and a lost
//! resume followed by one cold open.

mod common;

use common::{FakeModel, Fakes};
use suzatary_base::conversation_id::ConversationId;
use suzatary_core::conversation::round::Sink;
use suzatary_core::secretary::model::{Halt, Pick, SessionId, Turn};
use suzatary_core::secretary::store::Entry;
use suzatary_core::secretary::{Bound, answer, queue};

fn id() -> ConversationId {
    ConversationId::new("2026-09-14-1").expect("a slug")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment(
        [common::login("ada@example.com")],
        [(common::login("ada@example.com"), common::actor("ada"))],
    )
}

/// Open the conversation with one person turn already on it.
fn opened_with_a_person_turn(caps: &Fakes) {
    suzatary_core::secretary::conversations::start(
        caps.conversations.conversations(),
        &caps.model,
        &id(),
        "A morning",
        suzatary_core::secretary::store::Kind::Topic,
        common::now(),
    )
    .expect("the conversation opens");
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 9, 0, 0, 0)
                .expect("an instant"),
            turn: Turn::Person {
                text: "what changed".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
    )
    .expect("the person turn appends");
}

fn a_session_id(seed: &str) -> SessionId {
    // `SessionId::minted` is not reachable from here, so the UUID is written by hand.
    let hex: String = format!("{seed:0>32}")
        .chars()
        .filter(char::is_ascii_hexdigit)
        .chain(std::iter::repeat('0'))
        .take(32)
        .collect();
    SessionId::new(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
    .expect("a well-formed uuid")
}

/// The surface a session opened today would carry.
fn current_surface() -> String {
    suzatary_core::conversation::ask::surface_of(
        "you are the secretary",
        &suzatary_core::secretary::tools(),
        &std::collections::BTreeSet::new(),
    )
}

/// With no stored session, `open` runs once and its session is the answer's.
#[test]
fn no_session_opens_once() {
    let caps = Fakes::new("session-open").model(FakeModel::saying(["{\"say\": \"hi back\"}"]));
    opened_with_a_person_turn(&caps);
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let answered = answer(
        &caps.secretary(),
        || Pick::Held(&caps.model),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id(),
        "you are the secretary",
        &suzatary_core::secretary::surface::Subset::ALL,
        &Sink {
            on_delta: &|_| {},
            on_working: &|_| {},
            halt: &Halt::new(),
        },
    )
    .expect("the exchange runs")
    .expect("a round ran");

    assert_eq!(caps.model.opened().len(), 1, "open ran exactly once");
    assert!(
        caps.model.resumed_with().is_empty(),
        "nothing to resume yet"
    );
    let session = answered.stored.session.expect("a session is now stored");
    assert_eq!(session.session.id, caps.model.opened()[0]);
    assert_eq!(
        answered.stored.turns.last().map(|said| &said.turn),
        Some(&Turn::Model {
            text: "hi back".to_owned(),
            calls: Vec::new(),
        })
    );
}

/// A stored session resumes with exactly `queue::delta`'s turns and no `open`.
#[test]
fn a_stored_session_resumes_with_exactly_the_delta() {
    let caps = Fakes::new("session-resume").model(FakeModel::saying(["{\"say\": \"still here\"}"]));
    opened_with_a_person_turn(&caps);
    let existing = a_session_id("1");
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &Entry::Kept {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 8, 59, 0, 0)
                .expect("an instant"),
            session: existing.clone(),
            model: "claude-cli/haiku".to_owned(),
            cwd: "/repo".to_owned(),
            surface: Some(current_surface()),
            from: 0,
        },
    )
    .expect("the session line appends");

    let before =
        suzatary_core::secretary::conversations::read(caps.conversations.conversations(), &id())
            .expect("the conversation reads");
    let expected_delta: Vec<Turn> = queue::delta(&before)
        .iter()
        .map(|(_, said)| said.turn.clone())
        .collect();
    assert_eq!(
        expected_delta.len(),
        1,
        "exactly the one person turn is queued"
    );

    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let answered = answer(
        &caps.secretary(),
        || Pick::Held(&caps.model),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id(),
        "you are the secretary",
        &suzatary_core::secretary::surface::Subset::ALL,
        &Sink {
            on_delta: &|_| {},
            on_working: &|_| {},
            halt: &Halt::new(),
        },
    )
    .expect("the exchange runs")
    .expect("a round ran");

    assert!(caps.model.opened().is_empty(), "no open: a session stood");
    assert_eq!(caps.model.resumed_with(), vec![expected_delta]);
    assert_eq!(
        answered.stored.session.expect("still a session").session.id,
        existing
    );
}

/// A session told the record before the owner retracted a summary is opened afresh rather than
/// resumed: the record it holds may carry the summary he said is wrong, which no model is shown
/// again.
#[test]
fn a_retraction_reopens_a_stored_session() {
    let mut caps =
        Fakes::new("session-retracted").model(FakeModel::saying(["{\"say\": \"afresh\"}"]));
    let ada = prodrome_core::event::Actor::new("ada").expect("an actor");
    caps.owners = [ada.clone()].into();
    caps.digests = suzatary_io::objects::Objects::digests(
        caps.root.join("digests-owned"),
        caps.owners.clone(),
    );
    opened_with_a_person_turn(&caps);
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &Entry::Kept {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 8, 59, 0, 0)
                .expect("an instant"),
            session: a_session_id("1"),
            model: "claude-cli/haiku".to_owned(),
            cwd: "/repo".to_owned(),
            surface: Some(current_surface()),
            from: 0,
        },
    )
    .expect("the session line appends");
    let tap = suzatary_core::signatures::Tap::of(
        &suzatary_core::signatures::scene("ada"),
        &suzatary_core::signatures::ActHeaders {
            act: Some("1"),
            content_type: Some("application/json"),
            site: Some("same-origin"),
            client: Some(suzatary_core::signatures::CLIENT),
        },
        suzatary_core::signatures::Seat::Device,
    )
    .expect("the owner's tap");
    let node = prodrome_core::event::Hash::new("ab".repeat(32)).expect("a name");
    suzatary_store::store::Prodrome::append(
        &caps.digests,
        suzatary_core::secretary::digest_event::retracted(
            node,
            suzatary_base::capability::Datetime::new(2026, 9, 14, 9, 0, 0, 0).expect("an instant"),
            &tap,
        )
        .expect("a retraction"),
    )
    .expect("the owner's retraction is stored");

    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let answered = answer(
        &caps.secretary(),
        || Pick::Held(&caps.model),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id(),
        "you are the secretary",
        &suzatary_core::secretary::surface::Subset::ALL,
        &Sink {
            on_delta: &|_| {},
            on_working: &|_| {},
            halt: &Halt::new(),
        },
    )
    .expect("the exchange runs")
    .expect("a round ran");

    assert!(caps.model.resumed_with().is_empty(), "no resume");
    assert_eq!(caps.model.opened().len(), 1, "one open");
    assert_ne!(
        answered.stored.session.expect("a session").session.surface,
        Some(current_surface()),
        "the new session records the retraction"
    );
}

/// A lost resume is followed by exactly one `open` over the whole transcript, whose reply answers.
#[test]
fn a_lost_resume_is_followed_by_exactly_one_open_over_the_whole_transcript() {
    let caps = Fakes::new("session-lost")
        .model(FakeModel::saying(["{\"say\": \"reopened and answering\"}"]).losing_next_resume());
    opened_with_a_person_turn(&caps);
    let dead = a_session_id("dead");
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &Entry::Kept {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 8, 59, 0, 0)
                .expect("an instant"),
            session: dead.clone(),
            model: "claude-cli/haiku".to_owned(),
            surface: Some(current_surface()),
            cwd: "/repo".to_owned(),
            from: 0,
        },
    )
    .expect("the session line appends");

    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let answered = answer(
        &caps.secretary(),
        || Pick::Held(&caps.model),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id(),
        "you are the secretary",
        &suzatary_core::secretary::surface::Subset::ALL,
        &Sink {
            on_delta: &|_| {},
            on_working: &|_| {},
            halt: &Halt::new(),
        },
    )
    .expect("the exchange runs, recovered")
    .expect("a round ran");

    assert_eq!(
        caps.model.resumed_with().len(),
        1,
        "one resume was attempted, and it was the one that failed"
    );
    assert_eq!(caps.model.opened().len(), 1, "exactly one open follows");

    let asked = caps.model.asked();
    let last_asked = asked.last().expect("open asked the model something");
    assert!(
        last_asked
            .turns
            .iter()
            .any(|turn| matches!(turn, Turn::Person { text, .. } if text == "what changed")),
        "the open conversation carries the whole transcript"
    );

    let session = answered
        .stored
        .session
        .expect("a fresh session is recorded");
    assert_eq!(session.session.id, caps.model.opened()[0]);
    assert_ne!(session.session.id, dead);

    assert_eq!(
        answered.stored.turns.last().map(|said| &said.turn),
        Some(&Turn::Model {
            text: "reopened and answering".to_owned(),
            calls: Vec::new(),
        })
    );
}

/// A session with a different or missing surface is reopened once, then resumed.
#[test]
fn a_session_opened_on_another_surface_is_reopened_once() {
    let caps = Fakes::new("session-surface").model(FakeModel::saying([
        "{\"say\": \"reopened on today's verbs\"}",
        "{\"say\": \"resumed\"}",
    ]));
    opened_with_a_person_turn(&caps);
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &Entry::Kept {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 8, 59, 0, 0)
                .expect("an instant"),
            session: a_session_id("old"),
            model: "claude-cli/haiku".to_owned(),
            cwd: "/repo".to_owned(),
            surface: None,
            from: 0,
        },
    )
    .expect("the session line appends");

    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let sink = Sink {
        on_delta: &|_| {},
        on_working: &|_| {},
        halt: &Halt::new(),
    };
    let answered = answer(
        &caps.secretary(),
        || Pick::Held(&caps.model),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id(),
        "you are the secretary",
        &suzatary_core::secretary::surface::Subset::ALL,
        &sink,
    )
    .expect("the exchange runs")
    .expect("a round ran");
    assert!(
        caps.model.resumed_with().is_empty(),
        "no resume of the stale session"
    );
    assert_eq!(caps.model.opened().len(), 1, "one fresh open");
    let session = answered.stored.session.expect("a fresh session is stored");
    assert_eq!(
        session.session.surface,
        Some(current_surface()),
        "it records today's surface"
    );

    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 9, 10, 0, 0)
                .expect("an instant"),
            turn: Turn::Person {
                text: "and now".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
    )
    .expect("a second question appends");
    answer(
        &caps.secretary(),
        || Pick::Held(&caps.model),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id(),
        "you are the secretary",
        &suzatary_core::secretary::surface::Subset::ALL,
        &sink,
    )
    .expect("the second exchange runs");
    assert_eq!(caps.model.opened().len(), 1, "no second open");
    assert_eq!(
        caps.model.resumed_with().len(),
        1,
        "the fresh session resumes"
    );
}

/// A fresh session is headed with all memory, a resume with what moved, a quiet memory nothing.
#[test]
fn the_memory_heads_a_batch_once() {
    use suzatary_core::secretary::memory::{Fact, Kind, Origin};
    let at =
        || suzatary_base::capability::Datetime::new(2026, 9, 14, 9, 0, 0, 0).expect("an instant");
    let caps = Fakes::new("session-memory").model(FakeModel::saying([
        "{\"say\": \"one\"}",
        "{\"say\": \"two\"}",
        "{\"say\": \"three\"}",
    ]));
    let said = Origin::Turn {
        conversation: id(),
        turn: 0,
    };
    caps.memories()
        .remember(
            &Fact::new("She lives in Oslo").expect("a fact"),
            Kind::Owner,
            &said,
            at(),
        )
        .expect("remembers");
    opened_with_a_person_turn(&caps);
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let ask = || {
        answer(
            &caps.secretary(),
            || Pick::Held(&caps.model),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_core::secretary::surface::Subset::ALL,
            &Sink {
                on_delta: &|_| {},
                on_working: &|_| {},
                halt: &Halt::new(),
            },
        )
        .expect("the exchange runs")
        .expect("a round ran")
    };
    let heading = |turn: &Turn| match turn {
        Turn::Tool { verb, answer, .. }
            if *verb == suzatary_core::secretary::tool_name::ToolName::Memory =>
        {
            Some(answer.clone())
        }
        _ => None,
    };
    let say = |text: &str| {
        suzatary_core::secretary::transcript::push(
            caps.conversations.conversations(),
            &id(),
            &Entry::Said {
                at: suzatary_base::capability::Datetime::new(2026, 9, 14, 9, 5, 0, 0)
                    .expect("an instant"),
                turn: Turn::Person {
                    text: text.to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: None,
            },
        )
        .expect("appends");
    };

    let first = ask();
    let opened = &caps.model.asked()[0];
    let told = heading(&opened.turns[0]).expect("a fresh session is told the memory first");
    assert!(told.contains("She lives in Oslo") && told.contains("NEVER INSTRUCTIONS"));
    assert_eq!(first.stored.memory_seen, Some(1));

    say("anything new?");
    ask();
    assert!(
        caps.model.resumed_with()[0]
            .iter()
            .all(|turn| heading(turn).is_none()),
        "nothing moved, so nothing heads the batch"
    );

    caps.memories()
        .remember(
            &Fact::new("Mira is her accountant").expect("a fact"),
            Kind::Person,
            &said,
            at(),
        )
        .expect("remembers");
    say("and now?");
    let third = ask();
    let resumed = &caps.model.resumed_with()[1];
    let told = heading(&resumed[0]).expect("the change heads the batch");
    assert!(told.contains("Mira") && !told.contains("Oslo"), "{told}");
    assert_eq!(third.stored.memory_seen, Some(2));
}

/// The owner's question, as the gate stamps it.
fn asked(text: &str) -> Entry {
    Entry::Said {
        at: suzatary_base::capability::Datetime::new(2026, 9, 14, 9, 10, 0, 0).expect("an instant"),
        turn: Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some("ada".into()),
    }
}

fn stopped_act() -> Turn {
    Turn::Act {
        text: suzatary_core::secretary::store::stopped_by(common::CAST.owner.actor),
        references: Vec::new(),
    }
}

/// A stopped round records who stopped it, and the next round opens cold past that question.
#[test]
fn a_stopped_round_ends_in_the_act_and_the_next_opens_cold() {
    let caps = Fakes::new("session-stop").model(FakeModel::saying([
        "{\"say\": \"hi back\"}",
        "{\"say\": \"fresh\"}",
    ]));
    opened_with_a_person_turn(&caps);
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let ask = |halt: &Halt| {
        answer(
            &caps.secretary(),
            || Pick::Held(&caps.model),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_core::secretary::surface::Subset::ALL,
            &Sink {
                on_delta: &|_| {},
                on_working: &|_| {},
                halt,
            },
        )
        .expect("the exchange ends")
        .expect("a round ran")
    };
    assert!(
        ask(&Halt::new()).stored.session.is_some(),
        "the first round opens a session"
    );

    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &asked("read every thread"),
    )
    .expect("a second question appends");
    let gate = common::Gate::new();
    caps.model.hold_next(gate.clone());
    let halt = Halt::new();
    let stopped = std::thread::scope(|scope| {
        scope.spawn(|| {
            gate.wait_until_entered();
            halt.stop(common::actor("ada"));
        });
        ask(&halt).stored
    });
    let tail: Vec<(&Turn, Option<&str>)> = stopped
        .turns
        .iter()
        .rev()
        .take(2)
        .map(|said| {
            (
                &said.turn,
                said.by
                    .as_ref()
                    .and_then(suzatary_core::secretary::stamp::Stamp::actor),
            )
        })
        .collect();
    let question = Turn::Person {
        text: "read every thread".to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
    };
    assert_eq!(
        tail,
        vec![(&stopped_act(), Some("ada")), (&question, Some("ada"))],
        "the question, then the gate's actor's act, which closes it: no empty end"
    );
    assert!(stopped.session.is_none(), "the killed session is dropped");
    assert_eq!(queue::pending(&stopped), 0, "nothing is asked again");

    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &asked("just the permit"),
    )
    .expect("a third question appends");
    assert!(
        ask(&Halt::new()).stored.session.is_some(),
        "a session again"
    );
    assert_eq!(caps.model.opened().len(), 2, "the next round opens cold");
    let told = caps
        .model
        .asked()
        .last()
        .cloned()
        .expect("the model was asked");
    assert!(
        told.turns.contains(&stopped_act()),
        "the cold open is told the stop"
    );
}

/// A round with nothing waiting asks no model after an answer, a failure or a stop.
#[test]
fn nothing_waiting_asks_no_model_after_an_answer_a_failure_or_a_stop() {
    let counsel = suzatary_core::cast::CAST.counsel.name;
    // One reply only, so every later round finds the script run out and fails.
    let caps =
        Fakes::new("session-nothing-waits").model(FakeModel::saying(["{\"say\": \"hi back\"}"]));
    opened_with_a_person_turn(&caps);
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let ask = |halt: &Halt| {
        answer(
            &caps.secretary(),
            || Pick::Held(&caps.model),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_core::secretary::surface::Subset::ALL,
            &Sink {
                on_delta: &|_| {},
                on_working: &|_| {},
                halt,
            },
        )
    };
    let lines = || {
        suzatary_core::secretary::transcript::lines(caps.conversations.conversations(), &id())
            .expect("reads")
            .len()
    };
    let asks = || (caps.model.opened().len(), caps.model.resumed_with().len());

    ask(&Halt::new())
        .expect("the exchange runs")
        .expect("a round ran");
    let (before, models) = (lines(), asks());
    assert_eq!(ask(&Halt::new()).expect("nothing to run"), None);
    assert_eq!((lines(), asks()), (before, models), "no model, no line");

    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &asked(&format!("did {counsel} write back?")),
    )
    .expect("a question appends");
    let ran_out = ask(&Halt::new())
        .expect("a failed model ends the round with the box's words")
        .expect("a round ran");
    assert!(matches!(
        ran_out.ending,
        suzatary_core::conversation::round::Ending::Ended {
            why: suzatary_core::conversation::round::Why::Model { .. }
        }
    ));
    let (before, models) = (lines(), asks());
    assert_eq!(ask(&Halt::new()).expect("nothing to run"), None);
    assert_eq!((lines(), asks()), (before, models), "no model, no line");

    let stopped = Halt::new();
    stopped.stop(common::actor("ada"));
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &asked("read every thread"),
    )
    .expect("a question appends");
    ask(&stopped).expect("the stop ends the exchange");
    let (before, models) = (lines(), asks());
    assert_eq!(ask(&Halt::new()).expect("nothing to run"), None);
    assert_eq!((lines(), asks()), (before, models), "no model, no line");

    assert!(
        caps.model
            .resumed_with()
            .iter()
            .all(|delta| !delta.is_empty()),
        "every resume was sent a turn: {:?}",
        caps.model.resumed_with()
    );
}

/// The stateless arm asks no model when nothing is waiting either.
#[test]
fn nothing_waiting_asks_a_stateless_model_nothing() {
    let caps =
        Fakes::new("stateless-nothing-waits").model(FakeModel::saying(["{\"say\": \"hi back\"}"]));
    opened_with_a_person_turn(&caps);
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let ask = || {
        suzatary_core::secretary::answer(
            &caps.secretary(),
            || Pick::Turn(Box::new(&caps.model)),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_core::secretary::surface::Subset::ALL,
            &Sink {
                on_delta: &|_| {},
                on_working: &|_| {},
                halt: &Halt::new(),
            },
        )
        .expect("the exchange runs")
    };
    assert!(ask().is_some(), "the question is answered");
    let before =
        suzatary_core::secretary::transcript::lines(caps.conversations.conversations(), &id())
            .expect("reads")
            .len();
    assert_eq!(ask(), None);
    assert_eq!(caps.model.asked().len(), 1, "asked once, for the question");
    assert_eq!(
        suzatary_core::secretary::transcript::lines(caps.conversations.conversations(), &id())
            .expect("reads")
            .len(),
        before
    );
}

/// A model that answers with its own name.
struct Named(String);

impl suzatary_core::secretary::model::Model for Named {
    fn name(&self) -> String {
        self.0.clone()
    }

    fn turn(
        &self,
        _: &suzatary_core::secretary::model::Conversation,
        _: &serde_json::Value,
    ) -> Result<suzatary_core::secretary::model::Reply, suzatary_core::secretary::model::ModelError>
    {
        Ok(suzatary_core::secretary::model::Reply {
            say: format!("answered by {}", self.0),
            ..suzatary_core::secretary::model::Reply::default()
        })
    }
}

/// A round reads the order only when something waits, and each round's pick is its own: a
/// round with nothing to answer picks nothing, and the next round answers by the order as it
/// stands then, named by the model that answered.
#[test]
fn a_round_picks_only_when_something_waits_and_by_the_order_then() {
    let caps = Fakes::new("pick-per-round").model(FakeModel::saying(["{\"say\": \"hi\"}"]));
    suzatary_core::secretary::conversations::start(
        caps.conversations.conversations(),
        &caps.model,
        &id(),
        "A morning",
        suzatary_core::secretary::store::Kind::Topic,
        common::now(),
    )
    .expect("the conversation opens");
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let order = std::sync::Mutex::new("first".to_owned());
    let picks = std::sync::atomic::AtomicUsize::new(0);
    let ask = || {
        answer(
            &caps.secretary(),
            || {
                picks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Pick::Turn(Box::new(Named(order.lock().expect("unpoisoned").clone())))
            },
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_core::secretary::surface::Subset::ALL,
            &Sink {
                on_delta: &|_| {},
                on_working: &|_| {},
                halt: &Halt::new(),
            },
        )
        .expect("the exchange runs")
    };

    assert_eq!(ask(), None, "nothing waits");
    assert_eq!(
        picks.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "and nothing is picked"
    );

    *order.lock().expect("unpoisoned") = "second".to_owned();
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 9, 0, 0, 0)
                .expect("an instant"),
            turn: Turn::Person {
                text: "what changed".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
    )
    .expect("the person turn appends");
    let answered = ask().expect("the question is answered");
    assert_eq!(picks.load(std::sync::atomic::Ordering::SeqCst), 1);
    let said = answered
        .stored
        .live()
        .into_iter()
        .rev()
        .find_map(|(_, said)| match &said.turn {
            Turn::Model { text, .. } => Some(text.clone()),
            _ => None,
        });
    assert_eq!(said.as_deref(), Some("answered by second"));
}

/// A stamped turn of the owner's, or a model's or a verb's turn, as `answer` would have stored it.
fn push_said(caps: &Fakes, turn: Turn) {
    let by = matches!(turn, Turn::Person { .. }).then(|| "ada".into());
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 14, 9, 0, 0, 0)
                .expect("an instant"),
            turn,
            by,
        },
    )
    .expect("the turn appends");
}

fn owner(text: &str) -> Turn {
    Turn::Person {
        text: text.to_owned(),
        references: Vec::new(),
        attachments: Vec::new(),
    }
}

fn spoke(text: &str) -> Turn {
    Turn::Model {
        text: text.to_owned(),
        calls: Vec::new(),
    }
}

/// `n` closed rounds of chat: the owner's turn and an answer.
fn chat(caps: &Fakes, n: usize) {
    for k in 0..n {
        push_said(caps, owner(&format!("hello {k}")));
        push_said(caps, spoke("hi"));
    }
}

/// One round of `answer` over the conversation, as the worker runs it.
fn answered_once(
    caps: &Fakes,
    deployment: &suzatary_core::deployment::Deployment,
    policy: &suzatary_base::chains::ChainPolicy,
) -> suzatary_core::secretary::Answered {
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy,
    }];
    answer(
        &caps.secretary(),
        || Pick::Held(&caps.model),
        deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id(),
        "you are the secretary",
        &suzatary_core::secretary::surface::Subset::ALL,
        &Sink {
            on_delta: &|_| {},
            on_working: &|_| {},
            halt: &Halt::new(),
        },
    )
    .expect("the exchange runs")
    .expect("a round ran")
}

/// A conversation whose first round read a mail, then 27 rounds of chat and two turns waiting:
/// sixty turns, the whole of the window an opening sends.
fn a_full_window_that_read_a_mail(caps: &Fakes) {
    suzatary_core::secretary::conversations::start(
        caps.conversations.conversations(),
        &caps.model,
        &id(),
        "A morning",
        suzatary_core::secretary::store::Kind::Topic,
        common::now(),
    )
    .expect("the conversation opens");
    push_said(caps, owner("what did the bank say"));
    push_said(
        caps,
        Turn::Model {
            text: String::new(),
            calls: vec![suzatary_core::secretary::model::ToolCall {
                verb: "show".into(),
                args: std::collections::BTreeMap::new(),
            }],
        },
    );
    push_said(
        caps,
        Turn::Tool {
            verb: "show".into(),
            answer: "the bank's mail".to_owned(),
            outcome: suzatary_core::secretary::outcome::Outcome::Answered,
            files: Vec::new(),
        },
    );
    push_said(caps, spoke("it said so"));
    chat(caps, 27);
    push_said(caps, owner("one"));
    push_said(caps, owner("two"));
}

/// A turn said while a session opens moves nothing the opening sent: the first turn it sent is
/// recorded as sent, so a mail it read in the first round of that sixty-turn window is still held
/// eighty turns later, and `remember` stores a card.
#[test]
fn a_turn_said_while_a_session_opens_moves_nothing_it_was_sent() {
    let gate = common::Gate::new();
    let caps = Fakes::new("session-race").model(
        FakeModel::saying([
            r#"{"say": "opened"}"#,
            r#"{"say": "", "calls": [{"verb": "remember", "args": {
                "fact": "The owner moved in September", "kind": "owner",
                "why": "He said so.", "sources": "mail:a1b2c3d4e5f6"}}]}"#,
            r#"{"say": "Asked."}"#,
        ])
        .gated(gate.clone()),
    );
    a_full_window_that_read_a_mail(&caps);
    let before =
        suzatary_core::secretary::conversations::read(caps.conversations.conversations(), &id())
            .expect("it reads");
    assert_eq!(before.turns.len(), queue::MODEL_WINDOW);
    let sent = queue::for_model(&before)
        .first()
        .map(|(at, _)| *at)
        .expect("a window");
    assert_eq!(sent, 0, "the opening sends the read");

    let deployment = deployment();
    let policy = common::chain();
    let answered = std::thread::scope(|scope| {
        let opening = scope.spawn(|| answered_once(&caps, &deployment, &policy));
        gate.wait_until_entered();
        push_said(&caps, owner("said while it opens"));
        gate.release();
        opening.join().expect("the round finishes")
    });
    assert_eq!(caps.model.opened().len(), 1);
    let kept = answered.stored.session.expect("a session is kept");
    assert_eq!(
        kept.from, sent,
        "what it sent, not what a later fold would send"
    );

    chat(&caps, 40);
    push_said(&caps, owner("remember I moved"));
    let answered = answered_once(&caps, &deployment, &policy);
    assert_eq!(caps.model.opened().len(), 1, "resumed, not reopened");
    let receipt = answered
        .stored
        .turns
        .iter()
        .rev()
        .find_map(|said| match &said.turn {
            Turn::Tool { verb, answer, .. } if verb.as_str() == "remember" => Some(answer.clone()),
            _ => None,
        })
        .expect("remember answered");
    assert!(receipt.contains("\"proposed\""), "a card: {receipt}");
}
/// A session does not hear a round another model answered: Claude answers the first round in a
/// session, a provider the second, and the third, held again, opens a fresh session with the
/// whole window, the provider's round in it, rather than resuming the one that never heard it.
#[test]
fn a_round_another_model_answered_ends_the_session() {
    let caps = Fakes::new("session-unheard").model(FakeModel::saying([
        "{\"say\": \"first, from the session\"}",
        "{\"say\": \"third, from a fresh session\"}",
    ]));
    opened_with_a_person_turn(&caps);
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let round = |pick: Pick<'_>| {
        answer(
            &caps.secretary(),
            || pick,
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_core::secretary::surface::Subset::ALL,
            &Sink {
                on_delta: &|_| {},
                on_working: &|_| {},
                halt: &Halt::new(),
            },
        )
        .expect("the exchange runs")
        .expect("a round ran")
    };
    let push = |text: &str| {
        suzatary_core::secretary::transcript::push(
            caps.conversations.conversations(),
            &id(),
            &asked(text),
        )
        .expect("the question appends");
    };

    round(Pick::Held(&caps.model));
    assert_eq!(caps.model.opened().len(), 1);

    push("and the provider?");
    let after = round(Pick::Turn(Box::new(Named("a provider".to_owned()))));
    assert!(
        after.stored.session.is_none(),
        "the session did not hear this round"
    );
    let entries =
        suzatary_core::secretary::transcript::lines(caps.conversations.conversations(), &id())
            .expect("reads");
    assert!(
        entries.iter().any(|line| matches!(line,
            Entry::Unheard { model, .. } if model == "a provider")),
        "the line names the model that answered, by its own name"
    );

    push("and back?");
    round(Pick::Held(&caps.model));
    assert_eq!(caps.model.opened().len(), 2, "the third round opens");
    assert!(caps.model.resumed_with().is_empty(), "and resumes nothing");
    let sent = caps.model.asked().last().cloned().expect("asked");
    assert!(
        sent.turns.iter().any(|turn| matches!(turn,
            Turn::Model { text, .. } if text == "answered by a provider")),
        "the fresh session is sent the provider's round: {:?}",
        sent.turns
    );
}

/// The box's own closing turn is unheard too: a held round that runs out of rounds ends with a
/// turn the session never said, after tool answers it was never sent, so the next held round
/// opens a session sent them all rather than resuming one that missed them.
#[test]
fn a_round_the_box_ended_ends_the_session() {
    let calling = "{\"say\": \"Checking.\", \"calls\": [{\"verb\": \"context\", \"args\": {}}]}";
    let caps = Fakes::new("session-bound").model(FakeModel::saying(
        std::iter::repeat_n(calling, suzatary_core::secretary::MAX_ROUNDS)
            .chain(["{\"say\": \"from a fresh session\"}"]),
    ));
    opened_with_a_person_turn(&caps);
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let round = || {
        answer(
            &caps.secretary(),
            || Pick::Held(&caps.model),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_core::secretary::surface::Subset::ALL,
            &Sink {
                on_delta: &|_| {},
                on_working: &|_| {},
                halt: &Halt::new(),
            },
        )
        .expect("the exchange runs")
        .expect("a round ran")
    };

    let bounded = round();
    assert_eq!(caps.model.opened().len(), 1);
    assert!(
        bounded.stored.session.is_none(),
        "the box's closing turn was unheard"
    );

    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &asked("go on"),
    )
    .expect("the question appends");
    round();
    assert_eq!(caps.model.opened().len(), 2, "the next round opens");
    let sent = caps.model.asked().last().cloned().expect("asked");
    assert!(
        sent.turns.iter().any(|turn| matches!(turn,
            Turn::Model { text, .. } if text.starts_with("I asked for a verb"))),
        "the fresh session is sent the box's closing turn: {:?}",
        sent.turns
    );
    assert!(
        sent.turns
            .iter()
            .rev()
            .skip_while(|turn| !matches!(turn, Turn::Model { text, .. }
                if text.starts_with("I asked for a verb")))
            .nth(1)
            .is_some_and(|turn| matches!(turn, Turn::Tool { .. })),
        "and the tool answer before it: {:?}",
        sent.turns
    );
}

/// A round of the law's script, each after a person turn.
#[derive(Debug, Clone, Copy)]
enum Round {
    /// The held model answers.
    Held,
    /// A provider holding no session answers.
    Provider,
    /// The held session is lost and will not reopen; the held model answers statelessly.
    Lost,
    /// The held model calls a verb every time it is asked, until the box ends the round.
    Bound,
}

/// A held model that remembers what its session was sent and what it said itself, owner's and
/// model's words only, by their text.
#[derive(Default)]
struct Simulated {
    /// The words the live session holds.
    holds: std::sync::Mutex<Vec<String>>,
    /// What it held when it was first resumed this round.
    at_resume: std::sync::Mutex<Option<Vec<String>>>,
    opens: std::sync::atomic::AtomicUsize,
    said: std::sync::atomic::AtomicUsize,
    calling: std::sync::atomic::AtomicBool,
    lose: std::sync::atomic::AtomicBool,
    /// Every open fails while set.
    refuses: std::sync::atomic::AtomicBool,
    /// How many stateless answers call a verb before one answers.
    stateless_calls: std::sync::atomic::AtomicUsize,
}

fn words(turns: &[Turn]) -> Vec<String> {
    turns
        .iter()
        .filter_map(|turn| match turn {
            Turn::Person { text, .. } | Turn::Model { text, .. } => Some(text.clone()),
            Turn::Tool { answer, .. } => Some(answer.clone()),
            Turn::Act { .. } | Turn::Erased { .. } => None,
        })
        .collect()
}

impl Simulated {
    fn reply(&self) -> suzatary_core::secretary::model::Reply {
        let n = self.said.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let say = format!("held {n}");
        self.holds.lock().expect("unpoisoned").push(say.clone());
        suzatary_core::secretary::model::Reply {
            say,
            calls: if self.calling.load(std::sync::atomic::Ordering::SeqCst) {
                vec![suzatary_core::secretary::model::ToolCall {
                    verb: "context".into(),
                    args: std::collections::BTreeMap::new(),
                }]
            } else {
                Vec::new()
            },
            ..suzatary_core::secretary::model::Reply::default()
        }
    }
}

impl suzatary_core::secretary::model::Model for Simulated {
    fn name(&self) -> String {
        "claude-cli/simulated".to_owned()
    }

    fn turn(
        &self,
        _: &suzatary_core::secretary::model::Conversation,
        _: &serde_json::Value,
    ) -> Result<suzatary_core::secretary::model::Reply, suzatary_core::secretary::model::ModelError>
    {
        let n = self.said.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let calls = self
            .stateless_calls
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |left| left.checked_sub(1),
            )
            .is_ok();
        Ok(suzatary_core::secretary::model::Reply {
            say: format!("stateless {n}"),
            calls: if calls {
                vec![suzatary_core::secretary::model::ToolCall {
                    verb: "context".into(),
                    args: std::collections::BTreeMap::new(),
                }]
            } else {
                Vec::new()
            },
            ..suzatary_core::secretary::model::Reply::default()
        })
    }
}

impl suzatary_core::secretary::model::SessionModel for Simulated {
    fn open(
        &self,
        id: &SessionId,
        conversation: &suzatary_core::secretary::model::Conversation,
        _: &serde_json::Value,
        _: &Halt,
    ) -> Result<
        suzatary_core::secretary::model::Sessioned,
        suzatary_core::secretary::model::ModelError,
    > {
        // A session lost this round will not reopen.
        if self.lose.swap(false, std::sync::atomic::Ordering::SeqCst)
            || self.refuses.load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(suzatary_core::secretary::model::ModelError::Failed(
                "the session will not reopen".to_owned(),
            ));
        }
        self.opens.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *self.holds.lock().expect("unpoisoned") = words(&conversation.turns);
        Ok(suzatary_core::secretary::model::Sessioned {
            reply: self.reply(),
            session: suzatary_core::secretary::model::Session {
                id: id.clone(),
                model: "claude-cli/simulated".to_owned(),
                cwd: String::new(),
                created: "now".to_owned(),
                last: "now".to_owned(),
                surface: None,
            },
        })
    }

    fn resume(
        &self,
        _: &str,
        session: &suzatary_core::secretary::model::Session,
        delta: &suzatary_core::secretary::model::Delta,
        _: &serde_json::Value,
        _: &dyn Fn(&str),
        _: &Halt,
    ) -> Result<
        suzatary_core::secretary::model::Sessioned,
        suzatary_core::secretary::model::ModelError,
    > {
        if self.lose.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(suzatary_core::secretary::model::ModelError::Lost(
                "the session is gone".to_owned(),
            ));
        }
        let mut holds = self.holds.lock().expect("unpoisoned");
        holds.extend(words(delta.turns()));
        self.at_resume
            .lock()
            .expect("unpoisoned")
            .get_or_insert_with(|| holds.clone());
        drop(holds);
        Ok(suzatary_core::secretary::model::Sessioned {
            reply: self.reply(),
            session: session.clone(),
        })
    }
}

/// Each word of `wanted` held at least as often as it is wanted.
fn holds_every(held: &[String], wanted: &[String]) -> bool {
    wanted.iter().all(|word| {
        held.iter().filter(|h| *h == word).count() >= wanted.iter().filter(|w| *w == word).count()
    })
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig { cases: 48, ..Default::default() })]

    /// Whatever answers between them, a resumed session holds every word said since it opened,
    /// and a held round with a session kept resumes it rather than open another.
    #[test]
    fn a_resumed_session_holds_every_word_said_since_it_opened(
        rounds in proptest::collection::vec(
            proptest::prop_oneof![
                4 => proptest::strategy::Just(Round::Held),
                2 => proptest::strategy::Just(Round::Provider),
                1 => proptest::strategy::Just(Round::Lost),
                1 => proptest::strategy::Just(Round::Bound),
            ],
            1..8,
        ),
    ) {
        let caps = Fakes::new("session-law");
        let held = Simulated::default();
        suzatary_core::secretary::conversations::start(
            caps.conversations.conversations(),
            &held,
            &id(),
            "A morning",
            suzatary_core::secretary::store::Kind::Topic,
            common::now(),
        )
        .expect("the conversation opens");
        let deployment = deployment();
        let policy = common::chain();
        let chains = [Bound {
            name: "life",
            caps: caps.secretary(),
            policy: &policy,
        }];
        let read = || suzatary_core::secretary::conversations::read(caps.conversations.conversations(), &id()).expect("reads");
        // Whether the script leaves a session kept, read from the script, never from the fold: a
        // held round keeps one, as does a lost round with none to lose; a provider's round, a
        // lost session that will not reopen and a round the box ends leave none.
        let mut kept_by_script = false;
        for (n, round) in rounds.iter().enumerate() {
            suzatary_core::secretary::transcript::push(
                caps.conversations.conversations(),
                &id(),
                &asked(&format!("person {n}")),
            )
            .expect("the question appends");
            let before = read();
            proptest::prop_assert_eq!(
                before.session.is_some(),
                kept_by_script,
                "before round {}, the fold and the script disagree on a kept session", n
            );
            let opens = held.opens.load(std::sync::atomic::Ordering::SeqCst);
            *held.at_resume.lock().expect("unpoisoned") = None;
            held.calling.store(matches!(round, Round::Bound), std::sync::atomic::Ordering::SeqCst);
            // Only a kept session can be lost.
            held.lose.store(
                matches!(round, Round::Lost) && before.session.is_some(),
                std::sync::atomic::Ordering::SeqCst,
            );
            let provider = Named(format!("provider {n}"));
            answer(
                &caps.secretary(),
                || match round {
                    Round::Provider => Pick::Turn(Box::new(&provider)),
                    Round::Held | Round::Lost | Round::Bound => Pick::Held(&held),
                },
                &deployment,
                &suzatary_core::settings::Settables::default(),
                &chains,
                &id(),
                "you are the secretary",
                &suzatary_core::secretary::surface::Subset::ALL,
                &Sink { on_delta: &|_| {}, on_working: &|_| {}, halt: &Halt::new() },
            )
            .expect("the exchange runs");
            let had = kept_by_script;
            kept_by_script = match round {
                Round::Held => true,
                Round::Lost => !had,
                Round::Provider | Round::Bound => false,
            };
            let Some(kept) = &before.session else { continue };
            if matches!(round, Round::Provider) {
                continue;
            }
            proptest::prop_assert_eq!(
                held.opens.load(std::sync::atomic::Ordering::SeqCst),
                opens,
                "round {} opened a session while one was kept", n
            );
            if let Some(at_resume) = held.at_resume.lock().expect("unpoisoned").clone() {
                let said: Vec<Turn> = before
                    .live()
                    .into_iter()
                    .filter(|(index, _)| *index >= kept.from)
                    .map(|(_, said)| said.turn.clone())
                    .collect();
                let wanted = words(&said);
                proptest::prop_assert!(
                    holds_every(&at_resume, &wanted),
                    "round {}'s session holds {:?}, but {:?} was said since it opened",
                    n, at_resume, wanted
                );
            }
        }
    }
}

/// A session that is lost and will not open again, while its model still answers statelessly,
/// costs the round nothing: every step, the one after the session ended included, is answered
/// statelessly, and the round finishes.
#[test]
fn a_round_whose_opens_keep_failing_is_answered_statelessly_to_its_end() {
    let caps = Fakes::new("session-refusing");
    let held = Simulated::default();
    suzatary_core::secretary::conversations::start(
        caps.conversations.conversations(),
        &held,
        &id(),
        "A morning",
        suzatary_core::secretary::store::Kind::Topic,
        common::now(),
    )
    .expect("the conversation opens");
    let deployment = deployment();
    let policy = common::chain();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let round = || {
        answer(
            &caps.secretary(),
            || Pick::Held(&held),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_core::secretary::surface::Subset::ALL,
            &Sink {
                on_delta: &|_| {},
                on_working: &|_| {},
                halt: &Halt::new(),
            },
        )
    };
    let push = |text: &str| {
        suzatary_core::secretary::transcript::push(
            caps.conversations.conversations(),
            &id(),
            &asked(text),
        )
        .expect("the question appends");
    };

    push("first");
    round().expect("the exchange runs").expect("a round ran");
    assert_eq!(held.opens.load(std::sync::atomic::Ordering::SeqCst), 1);

    held.lose.store(true, std::sync::atomic::Ordering::SeqCst);
    held.refuses
        .store(true, std::sync::atomic::Ordering::SeqCst);
    held.stateless_calls
        .store(1, std::sync::atomic::Ordering::SeqCst);
    push("second");
    let answered = round()
        .expect("every step is answered statelessly")
        .expect("a round ran");
    assert!(answered.stored.session.is_none());
    let said: Vec<String> = answered
        .stored
        .live()
        .into_iter()
        .filter_map(|(_, said)| match &said.turn {
            Turn::Model { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert!(
        said.last()
            .is_some_and(|last| last.starts_with("stateless")),
        "the round ends on the stateless answer: {said:?}"
    );
    assert!(
        said.iter()
            .filter(|text| text.starts_with("stateless"))
            .count()
            >= 2,
        "both steps were answered statelessly: {said:?}"
    );
}
