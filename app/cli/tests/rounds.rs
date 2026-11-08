//! The secretary's rounds, pinned: for a scripted model, the lines each way of answering appends
//! and what the model is shown and sent, byte for byte against `tests/pins/`.
//!
//! The sweep's and the review's rounds are pinned beside their own fixtures, in `sweep.rs` and
//! `review.rs`.

mod common;

use common::{FakeModel, Fakes};
use suzatary_core::capability::{Halt, ModelError, Turn};
use suzatary_core::conversation::round::{Ending, Sink, Why};
use suzatary_core::conversation_id::ConversationId;
use suzatary_core::secretary::Bound;
use suzatary_core::secretary::store::Kind;

fn id() -> ConversationId {
    ConversationId::new("2026-09-02-1").expect("a slug")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment(
        [common::login("ada@example.com")],
        [(common::login("ada@example.com"), common::actor("ada"))],
    )
}

fn opened(caps: &Fakes) {
    suzatary_core::secretary::start(
        caps.conversations.conversations(),
        &caps.model,
        &id(),
        "A morning",
        Kind::Topic,
        common::now(),
    )
    .expect("the conversation opens");
}

/// A person's turn answered by the model with no session: `converse`, as the brief runs.
fn conversed(caps: &Fakes, said: &str) -> Result<Option<usize>, String> {
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    suzatary_core::secretary::converse(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        &id(),
        suzatary_core::secretary::Saying {
            text: said,
            references: &[],
            by: suzatary_core::secretary::Asker::Box(
                suzatary_core::secretary::own_round::OwnRound::Brief,
            ),
        },
        "you are the secretary",
    )
    .map(|answered| answered.map(|answered| answered.rounds))
    .map_err(|refusal| refusal.message().to_owned())
}

const CALLS: &str = r#"{"say": "Looking.", "calls": [
    {"verb": "context", "args": {}},
    {"verb": "no_such_verb", "args": {}},
    {"verb": "todo_show", "args": {"todo_id": "nosuchtodo"}}
]}"#;

/// A verb, an unknown verb and a refused verb, then an answer.
#[test]
fn a_round_without_a_session() {
    let caps = Fakes::new("rounds-window").model(FakeModel::saying([CALLS, r#"{"say": "Done."}"#]));
    opened(&caps);
    assert_eq!(conversed(&caps, "what changed?"), Ok(Some(2)));
    common::pinned("window", &common::scene(&caps, &id()));
}

/// A model that never stops calling runs out at the bound, and says so.
#[test]
fn a_round_that_runs_out() {
    let calling = r#"{"say": "", "calls": [{"verb": "weather", "args": {}}]}"#;
    let caps = Fakes::new("rounds-bound").model(FakeModel::saying([calling; 9]));
    opened(&caps);
    assert_eq!(conversed(&caps, "keep going"), Ok(Some(8)));
    common::pinned("bound", &common::scene(&caps, &id()));
}

/// A model that fails ends the exchange in the transcript, with the box's words.
#[test]
fn a_round_whose_model_fails() {
    let caps = Fakes::new("rounds-failed").model(FakeModel::saying([]));
    opened(&caps);
    assert_eq!(conversed(&caps, "hello?"), Ok(Some(1)));
    common::pinned("failed", &common::scene(&caps, &id()));
}

/// A session opened on the first round and resumed with the delta on the second.
#[test]
fn a_round_over_a_session() {
    let caps = Fakes::new("rounds-session").model(FakeModel::saying([
        CALLS,
        r#"{"say": "Done."}"#,
        r#"{"say": "And again."}"#,
    ]));
    opened(&caps);
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let deltas = std::sync::Mutex::new(Vec::new());
    let working = std::sync::Mutex::new(Vec::new());
    let on_delta = |text: &str| deltas.lock().expect("not poisoned").push(text.to_owned());
    let on_working = |verb: &str| working.lock().expect("not poisoned").push(verb.to_owned());
    let sink = Sink {
        on_delta: &on_delta,
        on_working: &on_working,
        halt: &Halt::new(),
    };
    for said in ["what changed?", "and now?"] {
        suzatary_core::secretary::transcript::push(
            caps.conversations.conversations(),
            &id(),
            &suzatary_core::secretary::store::Entry::Said {
                at: suzatary_core::capability::Datetime::new(2026, 9, 2, 12, 0, 0, 0)
                    .expect("an instant"),
                turn: suzatary_core::capability::Turn::Person {
                    text: said.to_owned(),
                    references: Vec::new(),
                    attachments: Vec::new(),
                },
                by: None,
            },
        )
        .expect("the person turn appends");
        suzatary_core::secretary::answer(
            &caps.secretary(),
            || suzatary_core::capability::Pick::Held(&caps.model),
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
    }
    let scene = format!(
        "{}# working\n{:?}\n# deltas\n{:?}\n",
        common::scene(&caps, &id()),
        working.lock().expect("not poisoned"),
        deltas.lock().expect("not poisoned"),
    );
    common::pinned("session", &scene);
}

/// One `Round` over a transcript-only ask, a surface of `weather` and `todo_show`, and a `run`
/// that records each verb it is handed and refuses `todo_show`.
fn round_of(
    caps: &Fakes,
    bound: usize,
    ran: &std::sync::Mutex<Vec<String>>,
) -> Result<Option<suzatary_core::conversation::round::Closed>, suzatary_core::refusal::Refusal> {
    round_under(
        caps,
        bound,
        ran,
        &Halt::new(),
        suzatary_core::conversation::round::LOAD_NOTHING,
    )
}

/// [`round_of`] under a halt and a load of the caller's.
fn round_under(
    caps: &Fakes,
    bound: usize,
    ran: &std::sync::Mutex<Vec<String>>,
    halt: &Halt,
    load: &suzatary_core::conversation::round::Load<'_>,
) -> Result<Option<suzatary_core::conversation::round::Closed>, suzatary_core::refusal::Refusal> {
    use suzatary_core::conversation::ask::Ask;
    use suzatary_core::conversation::round::{Round, Rules, Transcript};
    use suzatary_core::secretary::surface::Subset;
    use suzatary_core::secretary::verb_answer::VerbAnswer;

    const SOME: Subset = Subset {
        allows: |verb| verb.name() == "weather" || verb.name() == "todo_show",
        refuses: |called, listed| {
            suzatary_core::refusal::Refusal::not_found(format!("not {called}, only {listed}"))
        },
    };
    let run = |request: suzatary_core::verb::Request<suzatary_core::secretary::SecretaryCall>| {
        let verb = request.call.verb().name();
        ran.lock().expect("not poisoned").push(verb.to_owned());
        if verb == "todo_show" {
            Err(suzatary_core::refusal::Refusal::bad_request("no such todo"))
        } else {
            Ok(VerbAnswer::from(serde_json::json!("sunny")))
        }
    };
    Round::answer(
        &Rules {
            ask: Ask::Transcript { model: &caps.model },
            system: "you are the secretary",
            verbs: &SOME,
            run: &run,
            bound,
            sink: &Sink::quiet(halt),
            load,
        },
        &Transcript {
            conversations: caps.conversations.conversations(),
            clock: &caps.clock,
            id: &id(),
        },
    )
}

fn said_first(caps: &Fakes) {
    opened(caps);
    suzatary_core::secretary::transcript::push(
        caps.conversations.conversations(),
        &id(),
        &suzatary_core::secretary::store::Entry::Said {
            at: suzatary_core::capability::Datetime::new(2026, 9, 2, 12, 0, 0, 0)
                .expect("an instant"),
            turn: suzatary_core::capability::Turn::Person {
                text: "go".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
    )
    .expect("the person turn appends");
}

/// Law: a round never runs a verb its surface does not list; the call is answered with the
/// surface's refusal instead.
#[test]
fn a_round_never_runs_a_verb_its_surface_does_not_list() {
    let caps = Fakes::new("round-law-surface").model(FakeModel::saying([
        r#"{"say": "", "calls": [{"verb": "context", "args": {}}, {"verb": "weather", "args": {}}, {"verb": "send", "args": {}}]}"#,
        r#"{"say": "Done."}"#,
    ]));
    said_first(&caps);
    let ran = std::sync::Mutex::new(Vec::new());
    let round = round_of(&caps, 8, &ran)
        .expect("runs")
        .expect("a round ran")
        .round;

    assert_eq!(*ran.lock().expect("not poisoned"), ["weather"]);
    assert_eq!(caps.model.shown()[0], ["todo_show", "weather"]);
    let refused: Vec<_> = round
        .appended
        .iter()
        .filter_map(|entry| match entry {
            suzatary_core::secretary::store::Entry::Said {
                turn:
                    suzatary_core::capability::Turn::Tool {
                        verb,
                        answer,
                        outcome: suzatary_core::secretary::outcome::Outcome::Refused,
                        ..
                    },
                ..
            } => Some((verb.as_str(), answer.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(
        refused,
        [
            ("context", "not context, only todo_show, weather"),
            ("send", "not send, only todo_show, weather"),
        ]
    );
}

/// Law: a round stops at its bound, having asked exactly that many times, and says so.
#[test]
fn a_round_stops_at_its_bound() {
    let calling = r#"{"say": "", "calls": [{"verb": "weather", "args": {}}]}"#;
    for bound in 1..=4 {
        let caps =
            Fakes::new(&format!("round-law-bound-{bound}")).model(FakeModel::saying([calling; 6]));
        said_first(&caps);
        let ran = std::sync::Mutex::new(Vec::new());
        let closed = round_of(&caps, bound, &ran)
            .expect("runs")
            .expect("a round ran");
        let round = closed.round;

        assert_eq!(
            closed.ending,
            Ending::Ended {
                why: Why::Bound(bound)
            }
        );
        assert_eq!(round.steps, bound);
        assert_eq!(caps.model.asked().len(), bound);
        assert_eq!(ran.lock().expect("not poisoned").len(), bound);
        assert!(matches!(
            round.appended.last(),
            Some(suzatary_core::secretary::store::Entry::Said {
                turn: suzatary_core::capability::Turn::Model { text, calls },
                ..
            }) if calls.is_empty() && text.contains(&format!("{bound} times"))
        ));
    }
}

/// Law: what a round appends is exactly the model's turns and the verbs' answers, in order,
/// and it is what the conversation file gained.
#[test]
fn a_round_appends_the_models_turns_and_the_verbs_answers_in_order() {
    let caps = Fakes::new("round-law-order").model(FakeModel::saying([
        r#"{"say": "One.", "calls": [{"verb": "weather", "args": {}}, {"verb": "weather", "args": {"day": "tomorrow"}}]}"#,
        r#"{"say": "Two."}"#,
    ]));
    said_first(&caps);
    let before = suzatary_core::secretary::read(caps.conversations.conversations(), &id())
        .expect("reads")
        .turns
        .len();
    let ran = std::sync::Mutex::new(Vec::new());
    let round = round_of(&caps, 8, &ran)
        .expect("runs")
        .expect("a round ran")
        .round;

    // A transcript ask records its scope once, the first time it sends one, and then only turns.
    assert!(matches!(
        round.appended.first(),
        Some(suzatary_core::secretary::store::Entry::Scoped {
            scope: suzatary_core::secretary::store::Scope::Transcript,
            ..
        })
    ));
    let turns: Vec<_> = round.appended[1..]
        .iter()
        .map(|entry| match entry {
            suzatary_core::secretary::store::Entry::Said { turn, .. } => turn.clone(),
            other => panic!("only turns are appended after the scope: {other:?}"),
        })
        .collect();
    let tool = |answer: &str| suzatary_core::capability::Turn::Tool {
        verb: "weather".into(),
        answer: answer.to_owned(),
        outcome: suzatary_core::secretary::outcome::Outcome::Answered,
        files: Vec::new(),
    };
    assert_eq!(turns.len(), 4);
    assert!(
        matches!(&turns[0], suzatary_core::capability::Turn::Model { text, calls } if text == "One." && calls.len() == 2)
    );
    assert_eq!(turns[1], tool("sunny"));
    assert_eq!(turns[2], tool("sunny"));
    assert!(
        matches!(&turns[3], suzatary_core::capability::Turn::Model { text, calls } if text == "Two." && calls.is_empty())
    );
    let stored =
        suzatary_core::secretary::read(caps.conversations.conversations(), &id()).expect("reads");
    let gained: Vec<_> = stored.turns[before..]
        .iter()
        .map(|said| said.turn.clone())
        .collect();
    assert_eq!(gained, turns);
}

/// Law: a verb's refusal is an answer the model reads, not an error that ends the round.
#[test]
fn a_refusal_is_an_answer_not_an_error() {
    let caps = Fakes::new("round-law-refusal").model(FakeModel::saying([
        r#"{"say": "", "calls": [{"verb": "todo_show", "args": {"todo_id": "gone"}}]}"#,
        r#"{"say": "It is not there."}"#,
    ]));
    said_first(&caps);
    let ran = std::sync::Mutex::new(Vec::new());
    let round = round_of(&caps, 8, &ran)
        .expect("a refusal is not an error")
        .expect("a round ran")
        .round;

    assert_eq!(
        round.steps, 2,
        "the model was asked again after the refusal"
    );
    let second = &caps.model.asked()[1];
    assert_eq!(
        second.turns.last(),
        Some(&suzatary_core::capability::Turn::Tool {
            verb: "todo_show".into(),
            answer: "no such todo".to_owned(),
            outcome: suzatary_core::secretary::outcome::Outcome::Refused,
            files: Vec::new(),
        })
    );
}

/// The conversation's last live turn.
fn last_turn(caps: &Fakes) -> Turn {
    suzatary_core::secretary::read(caps.conversations.conversations(), &id())
        .expect("reads")
        .live()
        .last()
        .map(|(_, said)| said.turn.clone())
        .expect("a turn")
}

/// A round of [`round_of`] that ended with `why`, and the box's words as the last turn.
fn ended_with(caps: &Fakes, why: &Why, words: &str) {
    let ran = std::sync::Mutex::new(Vec::new());
    let closed = round_of(caps, 8, &ran).expect("runs").expect("a round ran");
    assert_eq!(closed.ending, Ending::Ended { why: why.clone() });
    assert_eq!(why.said(), words);
    assert_eq!(
        last_turn(caps),
        Turn::Model {
            text: suzatary_core::escape::escape_markup(words)
                .as_str()
                .to_owned(),
            calls: Vec::new(),
        }
    );
}

const WEATHER: &str = r#"{"say": "Looking.", "calls": [{"verb": "weather", "args": {}}]}"#;

/// A model that failed some other way quotes the backend, escaped so its words are not markup.
#[test]
fn a_round_whose_model_failed_quotes_the_backend_escaped() {
    let failed = ModelError::Failed("the API answered 500: *overloaded*".to_owned());
    let caps =
        Fakes::new("round-end-failed").model(FakeModel::saying([]).then_failing(failed.clone()));
    said_first(&caps);
    ended_with(
        &caps,
        &Why::Model {
            failed,
            after: None,
        },
        "I could not finish: the model failed (the API answered 500: *overloaded*). Ask again.",
    );
    let Turn::Model { text, .. } = last_turn(&caps) else {
        panic!("the box's turn is a model turn");
    };
    assert!(text.contains(r"\*overloaded\*"), "{text}");
}

/// What the question points at would not load: the round ends before any model is asked.
#[test]
fn a_round_whose_load_is_refused_says_so_and_asks_no_model() {
    let caps = Fakes::new("round-end-load").model(FakeModel::saying([]));
    said_first(&caps);
    let refused = || {
        Err(suzatary_core::refusal::Refusal::not_found(
            "no todo life/gone",
        ))
    };
    let ran = std::sync::Mutex::new(Vec::new());
    let closed = round_under(&caps, 8, &ran, &Halt::new(), &refused)
        .expect("runs")
        .expect("a round ran");
    let why = Why::Refused(suzatary_core::refusal::Refusal::not_found(
        "no todo life/gone",
    ));
    assert_eq!(closed.ending, Ending::Ended { why: why.clone() });
    assert_eq!(
        why.said(),
        "I could not finish: no todo life/gone. Ask again."
    );
    assert!(caps.model.asked().is_empty());
    assert!(matches!(last_turn(&caps), Turn::Model { calls, .. } if calls.is_empty()));
}

/// A pulled halt ends the round on a stop line naming who pulled it, not on the model's turn, and
/// a halt pulled before the model is asked asks it nothing.
#[test]
fn a_stopped_round_ends_on_the_stop() {
    let caps = Fakes::new("round-end-stopped").model(FakeModel::saying([WEATHER]));
    said_first(&caps);
    let halt = Halt::new();
    let ada = prodrome_core::event::Actor::new("ada").expect("an actor");
    halt.stop(ada.clone());
    let ran = std::sync::Mutex::new(Vec::new());
    let closed = round_under(
        &caps,
        8,
        &ran,
        &halt,
        suzatary_core::conversation::round::LOAD_NOTHING,
    )
    .expect("runs")
    .expect("a round ran");
    assert_eq!(
        closed.ending,
        Ending::Ended {
            why: Why::Stopped(ada)
        }
    );
    assert!(ran.lock().expect("not poisoned").is_empty());
    assert!(caps.model.asked().is_empty(), "no model was asked");
    assert!(matches!(
        closed.round.appended.last(),
        Some(suzatary_core::secretary::store::Entry::Stopped { by, .. }) if by == "ada"
    ));
}

/// One scripted step of the law below.
#[derive(Debug, Clone)]
enum Step {
    Answers,
    Calls,
    CallsARefusedVerb,
    Silent,
    Fails(ModelError),
}

fn step() -> impl proptest::strategy::Strategy<Value = Step> {
    use proptest::prelude::*;
    prop_oneof![
        Just(Step::Answers),
        Just(Step::Calls),
        Just(Step::CallsARefusedVerb),
        Just(Step::Silent),
        Just(Step::Fails(ModelError::TooLong(
            "Prompt is too long".to_owned()
        ))),
        (1u64..600).prop_map(|secs| Step::Fails(ModelError::TimedOut(
            std::time::Duration::from_secs(secs)
        ))),
        Just(Step::Fails(ModelError::Failed("exit 1".to_owned()))),
        Just(Step::Fails(ModelError::Unreachable("no route".to_owned()))),
    ]
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(48))]

    /// Law: whatever the model does, a round that ran ends on the model's answer in words, or on
    /// the box's turn saying why, or on a stop; never on a call or a verb's answer, and the
    /// ending says which.
    #[test]
    fn every_round_ends_on_an_answer_or_the_boxs_words(
        script in proptest::collection::vec(step(), 0..6),
        bound in 1usize..5,
        stopped in proptest::bool::weighted(0.15),
    ) {
        static RUN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let model = script.iter().fold(FakeModel::saying([]), |model, step| match step {
            Step::Answers => model.then_saying(r#"{"say": "Here it is."}"#),
            Step::Calls => model.then_saying(WEATHER),
            Step::CallsARefusedVerb => {
                model.then_saying(r#"{"say": "", "calls": [{"verb": "todo_show", "args": {}}]}"#)
            }
            Step::Silent => model.then_saying(r#"{"say": "  "}"#),
            Step::Fails(failed) => model.then_failing(failed.clone()),
        });
        let caps = Fakes::new(&format!("round-law-ending-{n}")).model(model);
        said_first(&caps);
        let halt = Halt::new();
        if stopped {
            halt.stop(prodrome_core::event::Actor::new("ada").expect("an actor"));
        }
        let ran = std::sync::Mutex::new(Vec::new());
        let closed = round_under(&caps, bound, &ran, &halt, suzatary_core::conversation::round::LOAD_NOTHING)
            .expect("the store takes every line")
            .expect("a person's turn waited");
        let last = last_turn(&caps);
        match &closed.ending {
            Ending::Answered => proptest::prop_assert_eq!(
                last,
                Turn::Model { text: "Here it is.".to_owned(), calls: Vec::new() }
            ),
            Ending::Ended { why: Why::Stopped(_) } => {
                proptest::prop_assert!(stopped);
                proptest::prop_assert!(
                    matches!(&last, Turn::Act { text, .. } if text.contains("stopped the answer")),
                    "{:?}", last
                );
            }
            Ending::Ended { why } => proptest::prop_assert_eq!(
                last,
                Turn::Model {
                    text: suzatary_core::escape::escape_markup(&why.said()).as_str().to_owned(),
                    calls: Vec::new(),
                }
            ),
        }
        // Nothing waits after any ending, so no model is asked again for this question.
        let stored = suzatary_core::secretary::read(caps.conversations.conversations(), &id()).expect("reads");
        proptest::prop_assert!(suzatary_core::secretary::queue::Pending::of(&stored).is_none());
    }
}
