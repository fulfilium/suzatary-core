//! The secretary's rounds, pinned: for a scripted model, the lines each way of answering appends
//! and what the model is shown and sent, byte for byte against `tests/pins/`.

use crate::common;
use suzatary_conversations::secretary::store::ConversationState;
use suzatary_lines::secretary::spoken::Spoken;

use common::{FakeModel, Fakes};
use suzatary_base::conversation_id::ConversationId;
use suzatary_conversations::secretary::model::{Halt, ModelError, Turn};
use suzatary_conversations::secretary::outcome;
use suzatary_conversations::secretary::store::Kind;
use suzatary_lines::secretary::conversations;
use suzatary_lines::secretary::transcript;
use suzatary_secretary::conversation::round::{Ending, Sink, Why};
use suzatary_secretary::secretary::Bound;

fn id() -> ConversationId {
    ConversationId::new("2026-09-02-1").expect("a slug")
}

fn deployment() -> suzatary_connect::deployment::Deployment {
    common::deployment(
        [common::login("ada@example.com")],
        [(common::login("ada@example.com"), common::actor("ada"))],
    )
}

fn opened(caps: &Fakes) {
    conversations::start(
        caps.conversations.conversations(),
        &caps.model,
        &id(),
        "A morning",
        Kind::Topic,
        common::now(),
    )
    .expect("the conversation opens");
}

/// `converse`, as the brief runs.
fn conversed(caps: &Fakes, said: &str) -> Result<Option<usize>, String> {
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    suzatary_secretary::secretary::converse(
        &caps.secretary(),
        &deployment,
        &suzatary_connect::settings::Settables::default(),
        &chains,
        &id(),
        suzatary_secretary::secretary::Saying {
            text: said,
            references: &[],
            by: suzatary_secretary::secretary::Asker::Box(
                suzatary_inbox::secretary::own_round::OwnRound::Brief,
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
fn a_round_of_verbs() {
    let caps = Fakes::new("rounds-window").model(FakeModel::saying([CALLS, r#"{"say": "Done."}"#]));
    opened(&caps);
    assert_eq!(conversed(&caps, "what changed?"), Ok(Some(2)));
    common::pinned("window", &common::scene(&caps, &id()));
}

#[test]
fn a_round_that_runs_out() {
    let calling = r#"{"say": "", "calls": [{"verb": "weather", "args": {}}]}"#;
    let caps = Fakes::new("rounds-bound").model(FakeModel::saying([calling; 9]));
    opened(&caps);
    assert_eq!(conversed(&caps, "keep going"), Ok(Some(8)));
    common::pinned("bound", &common::scene(&caps, &id()));
}

#[test]
fn a_round_whose_model_fails() {
    let caps = Fakes::new("rounds-failed").model(FakeModel::saying([]));
    opened(&caps);
    assert_eq!(conversed(&caps, "hello?"), Ok(Some(1)));
    common::pinned("failed", &common::scene(&caps, &id()));
}

/// Two rounds, each a fresh call told the memory and its window, its reply streamed.
#[test]
fn two_rounds_each_a_fresh_call() {
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
        transcript::push(
            caps.conversations.conversations(),
            &id(),
            &suzatary_conversations::secretary::store::Entry::Said {
                at: suzatary_base::capability::Datetime::new(2026, 9, 2, 12, 0, 0, 0)
                    .expect("an instant"),
                turn: Spoken::person(said.to_owned(), Vec::new(), Vec::new()),
                by: None,
            },
        )
        .expect("the person turn appends");
        suzatary_secretary::secretary::answer(
            &caps.secretary(),
            || -> suzatary_conversations::secretary::model::Pick<'_> { Box::new(&caps.model) },
            &deployment,
            &suzatary_connect::settings::Settables::default(),
            &chains,
            &id(),
            "you are the secretary",
            &suzatary_secretary::secretary::surface::Subset::ALL,
            &sink,
            suzatary_lines::secretary::transcript::Retry::Allowed,
            suzatary_secretary::secretary::Holder::Task,
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
    common::pinned("streamed", &scene);
}

/// One `Round` over a transcript-only ask, a surface of `weather` and `todo_show`, and a `run`
/// that records each verb it is handed and refuses `todo_show`.
fn round_of(
    caps: &Fakes,
    bound: usize,
    ran: &std::sync::Mutex<Vec<String>>,
) -> Result<Option<suzatary_secretary::conversation::round::Closed>, suzatary_base::refusal::Refusal>
{
    round_under(
        caps,
        bound,
        ran,
        &Halt::new(),
        suzatary_secretary::conversation::round::LOAD_NOTHING,
    )
}

fn round_under(
    caps: &Fakes,
    bound: usize,
    ran: &std::sync::Mutex<Vec<String>>,
    halt: &Halt,
    load: &suzatary_secretary::conversation::round::Load<'_>,
) -> Result<Option<suzatary_secretary::conversation::round::Closed>, suzatary_base::refusal::Refusal>
{
    use suzatary_secretary::conversation::ask::Ask;
    use suzatary_secretary::conversation::round::{Round, Rules, Transcript};
    use suzatary_secretary::secretary::surface::Subset;
    use suzatary_secretary::secretary::verb_answer::VerbAnswer;

    const SOME: Subset = Subset {
        allows: |verb| verb.name() == "weather" || verb.name() == "todo_show",
        refuses: |called, listed| {
            suzatary_base::refusal::Refusal::not_found(format!("not {called}, only {listed}"))
        },
    };
    let run = |request: suzatary_secretary::verb::Request<
        suzatary_secretary::secretary::SecretaryCall,
    >,
               _sent| {
        let verb = request.call.verb().name();
        ran.lock().expect("not poisoned").push(verb.to_owned());
        if verb == "todo_show" {
            Err(suzatary_base::refusal::Refusal::bad_request("no such todo"))
        } else {
            Ok(VerbAnswer::from(serde_json::json!("sunny")))
        }
    };
    Round::answer(
        &Rules {
            ask: Ask::Transcript { model: &caps.model },
            journal: &caps.journal,
            system: "you are the secretary",
            verbs: &SOME,
            run: &run,
            bound,
            sink: &Sink::quiet(halt),
            load,
            retry: suzatary_lines::secretary::transcript::Retry::Allowed,
            confirms: &|_| true,
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
    transcript::push(
        caps.conversations.conversations(),
        &id(),
        &suzatary_conversations::secretary::store::Entry::Said {
            at: suzatary_base::capability::Datetime::new(2026, 9, 2, 12, 0, 0, 0)
                .expect("an instant"),
            turn: Spoken::person("go".to_owned(), Vec::new(), Vec::new()),
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
            suzatary_conversations::secretary::store::Entry::Said {
                turn:
                    suzatary_conversations::secretary::model::Turn::Tool {
                        verb,
                        answer,
                        outcome: outcome::Outcome::Refused,
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
            Some(suzatary_conversations::secretary::store::Entry::Said {
                turn: suzatary_conversations::secretary::model::Turn::Model { text, calls },
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
    let before = conversations::read(caps.conversations.conversations(), &id())
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
        Some(suzatary_conversations::secretary::store::Entry::Scoped {
            scope: suzatary_conversations::secretary::store::Scope::Transcript,
            ..
        })
    ));
    let turns: Vec<_> = round.appended[1..]
        .iter()
        .map(|entry| match entry {
            suzatary_conversations::secretary::store::Entry::Said { turn, .. } => turn.clone(),
            other => panic!("only turns are appended after the scope: {other:?}"),
        })
        .collect();
    let tool = |answer: &str| suzatary_conversations::secretary::model::Turn::Tool {
        verb: "weather".into(),
        answer: answer.to_owned(),
        outcome: outcome::Outcome::Answered,
        files: Vec::new(),
    };
    assert_eq!(turns.len(), 4);
    assert!(
        matches!(&turns[0], suzatary_conversations::secretary::model::Turn::Model { text, calls } if text == "One." && calls.len() == 2)
    );
    assert_eq!(turns[1], tool("sunny"));
    assert_eq!(turns[2], tool("sunny"));
    assert!(
        matches!(&turns[3], suzatary_conversations::secretary::model::Turn::Model { text, calls } if text == "Two." && calls.is_empty())
    );
    let stored = conversations::read(caps.conversations.conversations(), &id()).expect("reads");
    let gained: Vec<_> = stored.turns[before..]
        .iter()
        .map(|said| said.turn.clone())
        .collect();
    assert_eq!(gained, turns);
}

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
        Some(&suzatary_conversations::secretary::model::Turn::Tool {
            verb: "todo_show".into(),
            answer: "no such todo".to_owned(),
            outcome: outcome::Outcome::Refused,
            files: Vec::new(),
        })
    );
}

fn last_turn(caps: &Fakes) -> Turn {
    conversations::read(caps.conversations.conversations(), &id())
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
            text: suzatary_base::escape::escape_markup(words)
                .as_str()
                .to_owned(),
            calls: Vec::new(),
        }
    );
}

const WEATHER: &str = r#"{"say": "Looking.", "calls": [{"verb": "weather", "args": {}}]}"#;

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

#[test]
fn a_round_whose_load_is_refused_says_so_and_asks_no_model() {
    let caps = Fakes::new("round-end-load").model(FakeModel::saying([]));
    said_first(&caps);
    let refused = || {
        Err(suzatary_base::refusal::Refusal::not_found(
            "no todo life/gone",
        ))
    };
    let ran = std::sync::Mutex::new(Vec::new());
    let closed = round_under(&caps, 8, &ran, &Halt::new(), &refused)
        .expect("runs")
        .expect("a round ran");
    let why = Why::Refused(suzatary_base::refusal::Refusal::not_found(
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

/// The stop line names who pulled it, and a halt pulled before the model is asked asks it nothing.
#[test]
fn a_stopped_round_ends_on_the_stop() {
    let caps = Fakes::new("round-end-stopped").model(FakeModel::saying([WEATHER]));
    said_first(&caps);
    let halt = Halt::new();
    let ada = prodrome_data::event::Actor::new("ada").expect("an actor");
    halt.stop(ada.clone());
    let ran = std::sync::Mutex::new(Vec::new());
    let closed = round_under(
        &caps,
        8,
        &ran,
        &halt,
        suzatary_secretary::conversation::round::LOAD_NOTHING,
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
        Some(suzatary_conversations::secretary::store::Entry::Stopped { by, .. }) if by == "ada"
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

    /// Law: a round ends on a stop, or else never on a call or a verb's answer, and the ending says
    /// which.
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
            halt.stop(prodrome_data::event::Actor::new("ada").expect("an actor"));
        }
        let ran = std::sync::Mutex::new(Vec::new());
        let closed = round_under(&caps, bound, &ran, &halt, suzatary_secretary::conversation::round::LOAD_NOTHING)
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
                    text: suzatary_base::escape::escape_markup(&why.said()).as_str().to_owned(),
                    calls: Vec::new(),
                }
            ),
        }
        // Nothing waits after any ending, so no model is asked again for this question.
        let stored = conversations::read(caps.conversations.conversations(), &id()).expect("reads");
        proptest::prop_assert!(!suzatary_conversations::secretary::queue::waits(&suzatary_conversations::secretary::queue::delta(&stored)));
    }
}

/// A model that, while it writes each answer, lets `during` land a line, as a `say` does when the
/// person asks again before the answer is written.
struct Interrupted<'a> {
    model: &'a FakeModel,
    during: &'a (dyn Fn() + Sync),
}

impl suzatary_conversations::secretary::model::Model for Interrupted<'_> {
    fn name(&self) -> String {
        suzatary_conversations::secretary::model::Model::name(self.model)
    }

    fn streamed(
        &self,
        conversation: &suzatary_conversations::secretary::model::Conversation,
        tools: &serde_json::Value,
        _on_delta: &dyn Fn(&str),
        _halt: &suzatary_conversations::secretary::model::Halt,
    ) -> Result<suzatary_conversations::secretary::model::Reply, ModelError> {
        let reply =
            suzatary_conversations::secretary::model::Model::turn(self.model, conversation, tools);
        (self.during)();
        reply
    }
}

/// The answer rests on the question without having seen it, so the next round sends it.
#[test]
fn a_question_said_during_the_last_answer_is_still_owed() {
    let caps = Fakes::new("round-swallowed").model(FakeModel::saying([
        r#"{"say": "Friday."}"#,
        r#"{"say": "Yes, the same office."}"#,
    ]));
    said_first(&caps);
    let late = std::sync::atomic::AtomicBool::new(true);
    let during = || {
        if late.swap(false, std::sync::atomic::Ordering::SeqCst) {
            transcript::push(
                caps.conversations.conversations(),
                &id(),
                &suzatary_conversations::secretary::store::Entry::Said {
                    at: common::now(),
                    turn: Spoken::person(
                        "and is it the same office?".to_owned(),
                        Vec::new(),
                        Vec::new(),
                    ),
                    by: None,
                },
            )
            .expect("the late question appends");
        }
    };
    let model = Interrupted {
        model: &caps.model,
        during: &during,
    };
    let answered = || {
        use suzatary_secretary::conversation::ask::Ask;
        use suzatary_secretary::conversation::round::{LOAD_NOTHING, Round, Rules, Transcript};
        Round::answer(
            &Rules {
                ask: Ask::Transcript { model: &model },
                journal: &suzatary_base::capability::Nothing,
                system: "you are the secretary",
                verbs: &suzatary_secretary::secretary::surface::Subset::ALL,
                run: &|_, _| Ok(serde_json::json!("unused").into()),
                bound: 8,
                sink: &Sink::quiet(&Halt::new()),
                load: LOAD_NOTHING,
                retry: suzatary_lines::secretary::transcript::Retry::Allowed,
                confirms: &|_| true,
            },
            &Transcript {
                conversations: caps.conversations.conversations(),
                clock: &caps.clock,
                id: &id(),
            },
        )
        .expect("runs")
    };
    assert_eq!(answered().expect("a round ran").ending, Ending::Answered);
    let owed: Vec<String> = transcript::read(caps.conversations.conversations(), &id())
        .expect("reads")
        .owed()
        .into_iter()
        .filter_map(|(_, said)| match &said.turn {
            Turn::Person { text, .. } => Some(text.clone()),
            Turn::Model { .. } | Turn::Tool { .. } | Turn::Act { .. } | Turn::Erased { .. } => None,
        })
        .collect();
    assert_eq!(owed, ["and is it the same office?"]);
    let state = || {
        transcript::read(caps.conversations.conversations(), &id())
            .expect("reads")
            .state()
    };
    assert_eq!(
        state(),
        ConversationState::Queued,
        "between the first answer and the round the late question starts, the page reads it queued, never idle"
    );
    assert_eq!(
        answered().expect("a second round ran").ending,
        Ending::Answered
    );
    assert_eq!(state(), ConversationState::Idle);
    let sent = caps.model.asked();
    assert!(
        sent[1].turns.iter().any(|turn| matches!(
            turn,
            Turn::Person { text, .. } if text == "and is it the same office?"
        )),
        "the second round sent the late question"
    );
    assert!(
        transcript::read(caps.conversations.conversations(), &id())
            .expect("reads")
            .owed()
            .is_empty()
    );
}

/// A model whose first `panics` calls panic, as a round's task that ends with no close.
struct Panicking<'a> {
    model: &'a FakeModel,
    panics: std::sync::atomic::AtomicUsize,
}

impl suzatary_conversations::secretary::model::Model for Panicking<'_> {
    fn name(&self) -> String {
        suzatary_conversations::secretary::model::Model::name(self.model)
    }

    fn streamed(
        &self,
        conversation: &suzatary_conversations::secretary::model::Conversation,
        tools: &serde_json::Value,
        _on_delta: &dyn Fn(&str),
        _halt: &suzatary_conversations::secretary::model::Halt,
    ) -> Result<suzatary_conversations::secretary::model::Reply, ModelError> {
        let left = self.panics.load(std::sync::atomic::Ordering::SeqCst);
        if left > 0 {
            self.panics
                .store(left - 1, std::sync::atomic::Ordering::SeqCst);
            panic!("the round's task ends here, with no close");
        }
        suzatary_conversations::secretary::model::Model::turn(self.model, conversation, tools)
    }
}

/// One round over `model`, a panic inside it caught as a task's end (`Err`).
fn round_over(
    caps: &Fakes,
    model: &dyn suzatary_conversations::secretary::model::Model,
) -> std::thread::Result<Option<suzatary_secretary::conversation::round::Closed>> {
    use suzatary_secretary::conversation::ask::Ask;
    use suzatary_secretary::conversation::round::{LOAD_NOTHING, Round, Rules, Transcript};
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Round::answer(
            &Rules {
                ask: Ask::Transcript { model },
                journal: &suzatary_base::capability::Nothing,
                system: "you are the secretary",
                verbs: &suzatary_secretary::secretary::surface::Subset::ALL,
                run: &|_, _| Ok(serde_json::json!("unused").into()),
                bound: 8,
                sink: &Sink::quiet(&Halt::new()),
                load: LOAD_NOTHING,
                retry: suzatary_lines::secretary::transcript::Retry::Allowed,
                confirms: &|_| true,
            },
            &Transcript {
                conversations: caps.conversations.conversations(),
                clock: &caps.clock,
                id: &id(),
            },
        )
        .expect("runs")
    }))
}

fn claimed_by(caps: &Fakes) -> Vec<String> {
    transcript::lines(caps.conversations.conversations(), &id())
        .expect("reads")
        .into_iter()
        .filter_map(|entry| match entry {
            suzatary_conversations::secretary::store::Entry::Began { run, .. } => {
                Some(run.as_str().to_owned())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_lost_round_is_retried_once_by_a_new_run() {
    let caps = Fakes::new("round-lost-once").model(FakeModel::saying([r#"{"say": "Friday."}"#]));
    said_first(&caps);
    let model = Panicking {
        model: &caps.model,
        panics: std::sync::atomic::AtomicUsize::new(1),
    };
    assert!(
        round_over(&caps, &model).is_err(),
        "the first round's task ended"
    );
    let retried = round_over(&caps, &model)
        .expect("the retry runs")
        .expect("a round ran");
    assert_eq!(retried.ending, Ending::Answered);
    let runs = claimed_by(&caps);
    assert_eq!(runs.len(), 2, "a claim and its retry");
    assert_ne!(runs[0], runs[1], "the retry is a new run");
    assert!(
        transcript::read(caps.conversations.conversations(), &id())
            .expect("reads")
            .owed()
            .is_empty()
    );
}

/// A question two lost rounds covered is closed by the box, which asks no model and claims it no
/// third time.
#[test]
fn a_question_lost_twice_is_closed_by_the_box() {
    let caps = Fakes::new("round-lost-twice").model(FakeModel::saying([r#"{"say": "Friday."}"#]));
    said_first(&caps);
    let model = Panicking {
        model: &caps.model,
        panics: std::sync::atomic::AtomicUsize::new(2),
    };
    assert!(round_over(&caps, &model).is_err());
    assert!(round_over(&caps, &model).is_err());
    let closed = round_over(&caps, &model)
        .expect("the claim step runs")
        .expect("the box closed it");
    assert_eq!(
        closed.ending,
        Ending::Ended {
            why: Why::Unanswered
        }
    );
    assert_eq!(claimed_by(&caps).len(), 2, "no third claim");
    assert!(caps.model.asked().is_empty(), "no model answered");
    assert_eq!(
        last_turn(&caps),
        Turn::Model {
            text: "This was not answered in time; ask again.".to_owned(),
            calls: Vec::new(),
        }
    );
    assert!(
        round_over(&caps, &model).expect("runs").is_none(),
        "closed once"
    );
}

/// A question asked longer ago than the bound is never claimed: the box closes it, so Regenerate
/// can ask it again.
#[test]
fn a_question_past_the_bound_is_closed_unclaimed() {
    let caps = Fakes::new("round-past-bound").model(FakeModel::saying([r#"{"say": "Friday."}"#]));
    said_first(&caps);
    caps.clock.set(common::shift(common::now(), 1));
    let closed = round_over(&caps, &caps.model)
        .expect("the claim step runs")
        .expect("the box closed it");
    assert_eq!(
        closed.ending,
        Ending::Ended {
            why: Why::Unanswered
        }
    );
    assert!(claimed_by(&caps).is_empty());
    assert!(caps.model.asked().is_empty());
}

/// A question said while a round runs joins it: the round claims it under its own run before
/// its next ask, and sends it then.
#[test]
fn a_question_said_during_a_round_joins_it_at_its_next_ask() {
    let caps = Fakes::new("round-join").model(FakeModel::saying([
        WEATHER,
        r#"{"say": "Sunny, and yes."}"#,
    ]));
    said_first(&caps);
    let late = std::sync::atomic::AtomicBool::new(true);
    let during = || {
        if late.swap(false, std::sync::atomic::Ordering::SeqCst) {
            transcript::push(
                caps.conversations.conversations(),
                &id(),
                &suzatary_conversations::secretary::store::Entry::Said {
                    at: common::now(),
                    turn: Spoken::person("and tomorrow?".to_owned(), Vec::new(), Vec::new()),
                    by: None,
                },
            )
            .expect("the follow-up appends");
        }
    };
    let model = Interrupted {
        model: &caps.model,
        during: &during,
    };
    let closed = round_over(&caps, &model)
        .expect("runs")
        .expect("a round ran");
    assert_eq!(closed.ending, Ending::Answered);
    assert_eq!(closed.round.steps, 2, "one round, asked twice");
    let runs = claimed_by(&caps);
    assert_eq!(runs.len(), 2, "the claim and its join");
    assert_eq!(runs[0], runs[1], "the join is the round's own run");
    assert!(
        caps.model.asked()[1].turns.iter().any(|turn| matches!(
            turn,
            Turn::Person { text, .. } if text == "and tomorrow?"
        )),
        "the next ask sent the follow-up"
    );
    assert!(
        transcript::read(caps.conversations.conversations(), &id())
            .expect("reads")
            .owed()
            .is_empty()
    );
}

/// The bound decides whether a round starts and never what a started round covers.
#[test]
fn an_old_question_beside_a_fresh_one_is_sent_with_it() {
    let caps =
        Fakes::new("round-old-beside-fresh").model(FakeModel::saying([r#"{"say": "Both."}"#]));
    opened(&caps);
    let ask = |at: suzatary_base::capability::Datetime, text: &str| {
        transcript::push(
            caps.conversations.conversations(),
            &id(),
            &suzatary_conversations::secretary::store::Entry::Said {
                at,
                turn: Spoken::person(text.to_owned(), Vec::new(), Vec::new()),
                by: None,
            },
        )
        .expect("asked");
    };
    ask(common::shift(common::now(), -1), "is the permit renewed?");
    ask(common::now(), "and the office hours?");
    let answered = round_over(&caps, &caps.model)
        .expect("runs")
        .expect("a round ran");
    assert_eq!(answered.ending, Ending::Answered);
    let sent = &caps.model.asked()[0];
    for question in ["is the permit renewed?", "and the office hours?"] {
        assert!(
            sent.turns
                .iter()
                .any(|turn| matches!(turn, Turn::Person { text, .. } if text == question)),
            "{question} was sent"
        );
    }
    assert!(
        round_over(&caps, &caps.model).expect("runs").is_none(),
        "nothing is owed"
    );
    assert_eq!(
        last_turn(&caps),
        Turn::Model {
            text: "Both.".to_owned(),
            calls: Vec::new(),
        }
    );
}

/// The other holder finds the round's run alive and appends nothing, so the first claims again
/// after its own round, before it returns.
#[test]
fn a_question_said_during_another_holder_s_last_ask_is_answered_by_that_holder() {
    let caps = Fakes::new("round-two-holders").model(FakeModel::saying([
        r#"{"say": "Friday."}"#,
        r#"{"say": "Yes, the same office."}"#,
    ]));
    said_first(&caps);
    let late = std::sync::atomic::AtomicBool::new(true);
    let during = || {
        if late.swap(false, std::sync::atomic::Ordering::SeqCst) {
            transcript::push(
                caps.conversations.conversations(),
                &id(),
                &suzatary_conversations::secretary::store::Entry::Said {
                    at: common::now(),
                    turn: Spoken::person(
                        "and is it the same office?".to_owned(),
                        Vec::new(),
                        Vec::new(),
                    ),
                    by: None,
                },
            )
            .expect("the late question appends");
            let other = caps
                .conversations
                .conversations()
                .runs
                .begin()
                .expect("the other holder's run");
            assert_eq!(
                transcript::claim(
                    caps.conversations.conversations(),
                    &id(),
                    &other.run,
                    common::now(),
                    suzatary_lines::secretary::transcript::Retry::Allowed,
                    &|_| true,
                    |_| Vec::new(),
                )
                .expect("reads"),
                transcript::Claimed::Nothing,
                "the other holder finds the round's run alive"
            );
        }
    };
    let model = Interrupted {
        model: &caps.model,
        during: &during,
    };
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];
    let answered = suzatary_secretary::secretary::answer(
        &caps.secretary(),
        || -> suzatary_conversations::secretary::model::Pick<'_> { Box::new(&model) },
        &deployment,
        &suzatary_connect::settings::Settables::default(),
        &chains,
        &id(),
        "you are the secretary",
        &suzatary_secretary::secretary::surface::Subset::ALL,
        &Sink::quiet(&Halt::new()),
        suzatary_lines::secretary::transcript::Retry::Allowed,
        suzatary_secretary::secretary::Holder::Alone,
    )
    .expect("runs")
    .expect("a round ran");
    assert_eq!(answered.ending, Ending::Answered);
    assert_eq!(answered.rounds, 2, "two rounds, one ask each");
    assert!(
        caps.model.asked()[1].turns.iter().any(|turn| matches!(
            turn,
            Turn::Person { text, .. } if text == "and is it the same office?"
        )),
        "the second round sent the late question"
    );
    assert!(
        transcript::read(caps.conversations.conversations(), &id())
            .expect("reads")
            .owed()
            .is_empty()
    );
}

/// Each call of a round is told to the journal with the counts the model returned, and what it
/// is told holds no words of the call.
#[test]
fn a_call_is_journaled_with_its_counts() {
    let spent = suzatary_base::call_log::TokenUsage {
        prompt_tokens: 11,
        completion_tokens: 22,
        cached_prompt_tokens: 33,
        cache_written_prompt_tokens: 44,
    };
    let caps = Fakes::new("rounds-calls")
        .model(FakeModel::saying([CALLS, r#"{"say": "Done."}"#]).costing(spent));
    opened(&caps);
    assert_eq!(conversed(&caps, "what changed on the rent?"), Ok(Some(2)));
    let calls = caps.journal.calls();
    assert_eq!(calls.len(), 2, "one per call: {calls:?}");
    for (call, step) in calls.iter().zip(1..) {
        assert_eq!(call.conversation.as_deref(), Some("2026-09-02-1"));
        assert_eq!(call.step, Some(step));
        assert_eq!(call.model, "a scripted model");
        assert_eq!(call.usage, Some(spent));
        assert!(call.bytes > 0);
    }
    assert!(
        calls[1].bytes > calls[0].bytes,
        "the second call also holds the first's verbs' answers"
    );
}
