//! The conversation worker over a real server; two scenes hold `common::Gate` across the model
//! call, since a fast fake could finish a round first.

use crate::common;

use std::sync::Arc;

use common::{FakeModel, Fakes, Gate};
use suzatary_conversations::secretary::queue;

fn ada() -> suzatary_base::capability::Login {
    common::login("ada@example.com")
}

fn writer(tag: &str, model: FakeModel) -> (String, Arc<suzatary_server::serve::Workers>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new(tag).whois(whois).model(model);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, workers, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    (base, workers)
}

fn writer_idle(
    tag: &str,
    model: FakeModel,
    idle: std::time::Duration,
) -> (String, Arc<suzatary_server::serve::Workers>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new(tag).whois(whois).model(model);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, workers, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new())
            .idle(idle)
            .start();
    (base, workers)
}

fn open(base: &str) -> String {
    common::opened(base, "A morning")
}

fn say(base: &str, id: &str, text: &str) {
    let _ = common::say(base, id, text);
}

/// Whether the person turn `text` is owed a round that no round covers yet.
fn queued(base: &str, id: &str, text: &str) -> bool {
    common::reading(base, id, &[])["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .any(|turn| turn["who"] == "person" && turn["text"] == text && turn["queued"] == true)
}

/// The answer the model wrote without the `say` does not settle it.
/// `wait_until_entered` blocks until the fake has registered the call, since a taken line says
/// nothing about whether the worker had reached the model yet.
#[test]
fn a_say_during_an_answer_is_answered_by_the_next_exchange() {
    let gate = Gate::new();
    let (base, workers) = writer(
        "worker-coalesce",
        FakeModel::saying([
            "{\"say\": \"got the first\"}",
            "{\"say\": \"got the second\"}",
        ])
        .gated(gate.clone()),
    );
    let id = open(&base);

    say(&base, &id, "first");
    gate.wait_until_entered();
    assert!(
        !queued(&base, &id, "first"),
        "the round has claimed the first, so it is no longer queued"
    );

    say(&base, &id, "second");
    assert!(
        queued(&base, &id, "second"),
        "said while the first is asked, and queued until a round claims it"
    );
    gate.release();

    let payload = common::poll_conversation(&base, &id, |payload| {
        payload["turns"]
            .as_array()
            .is_some_and(|turns| turns.len() == 4)
    });
    let turns = payload["turns"].as_array().expect("turns");
    assert_eq!(
        turns.len(),
        4,
        "two person turns and two model turns: {turns:?}"
    );
    assert_eq!(turns[0]["who"], "person");
    assert_eq!(turns[0]["text"], "first");
    assert_eq!(turns[1]["who"], "person");
    assert_eq!(turns[1]["text"], "second");
    assert_eq!(turns[2]["who"], "model");
    assert_eq!(turns[2]["text"], "got the first");
    assert_eq!(turns[3]["who"], "model");
    assert_eq!(turns[3]["text"], "got the second");

    // The worker's idle check reads a finished model turn as the last.
    let id = suzatary_base::conversation_id::ConversationId::new(id).expect("a slug");
    assert!(
        workers.is_running(&id),
        "still in the map until it drops itself idle"
    );
}

/// With no round able to complete, a say past `MAX_PENDING` is refused in the lines' exact words.
#[test]
fn a_say_past_max_pending_is_refused_before_it_is_appended() {
    let gate = Gate::new();
    let (base, _workers) = writer(
        "worker-max-pending",
        FakeModel::saying(["{\"say\": \"eventually\"}"]).gated(gate.clone()),
    );
    let id = open(&base);

    say(&base, &id, "line 0");
    gate.wait_until_entered();

    for n in 1..queue::MAX_PENDING {
        say(&base, &id, &format!("line {n}"));
    }

    let refused = common::sealed(
        &base,
        &id,
        &common::question(&base, "one too many", Vec::new(), Vec::new()),
    );
    assert_eq!(
        common::refused_line(&base, &refused),
        format!(
            "{} lines already wait for the secretary; say it after they are answered",
            queue::MAX_PENDING
        )
    );

    gate.release();
    let payload = common::settled(&base, &id);
    let turns = payload["turns"].as_array().expect("turns");
    let persons = turns.iter().filter(|turn| turn["who"] == "person").count();
    assert_eq!(
        persons,
        queue::MAX_PENDING,
        "the refused line never joined the transcript: {turns:?}"
    );
}

/// Once a round ends with nothing queued, the idle worker leaves the table, so no task leaks.
#[test]
fn an_idle_worker_drops_out_of_the_table() {
    // Pins the mechanism, not the production constant of 30 s.
    let idle = std::time::Duration::from_millis(300);
    let (base, workers) = writer_idle(
        "worker-idle",
        FakeModel::saying(["{\"say\": \"just the one\"}"]),
        idle,
    );
    let id = open(&base);

    say(&base, &id, "hello");
    common::settled(&base, &id);

    let id = suzatary_base::conversation_id::ConversationId::new(id).expect("a slug");
    let mut gone = false;
    for _ in 0..100 {
        if !workers.is_running(&id) {
            gone = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(gone, "the worker never dropped itself out of the table");
}

/// The memo that lasts is the transcript: a second server over the same stores answers only what
/// is said after.
#[test]
fn a_restarted_box_refires_nothing_already_answered() {
    let asks = |model: &FakeModel| model.asked().len();
    let deployment = || common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let whois = || common::FakeWhois::new().naming(common::v4(), ada());
    let first = Arc::new(
        Fakes::new("worker-restart")
            .whois(whois())
            .model(FakeModel::saying(["{\"say\": \"once\"}"])),
    );
    let root = first.root.clone();
    let chain = first.chain.inner().clone();
    let base = common::Server::new(Arc::clone(&first), deployment(), Vec::new())
        .start()
        .base;
    let id = open(&base);
    say(&base, &id, "hello");
    common::settled(&base, &id);
    assert_eq!(asks(&first.model), 1);

    let second = Arc::new(
        Fakes::over(root, suzatary_io_chains::objects::Objects::chain(chain))
            .whois(whois())
            .model(FakeModel::saying(["{\"say\": \"the new one\"}"])),
    );
    let base = common::Server::new(Arc::clone(&second), deployment(), Vec::new())
        .start()
        .base;
    // A write the engine hears, on a reading whose rules are all discharged.
    common::renamed(&base, &id, "A renamed morning");
    say(&base, &id, "and now?");
    let payload = common::settled(&base, &id);
    assert_eq!(
        asks(&second.model),
        1,
        "the restarted box asked once, for the new line only"
    );
    let turns = payload["turns"].as_array().expect("turns");
    assert_eq!(turns.last().expect("a turn")["text"], "the new one");
}

/// A panic in the model's call leaves the claim open, and its run is gone with the task, so the
/// conversation's task claims it again with a new run.
#[test]
fn a_round_that_ends_with_no_close_is_retried_by_its_conversation() {
    let (base, _workers) = writer(
        "worker-lost",
        FakeModel::saying(["{\"say\": \"after the retry\"}"]).panicking(1),
    );
    let id = open(&base);
    say(&base, &id, "is it Friday?");
    let payload = common::settled(&base, &id);
    let turns = payload["turns"].as_array().expect("turns");
    assert_eq!(turns.len(), 2, "{turns:?}");
    assert_eq!(turns[1]["text"], "after the retry");
}

/// A holder outside the server (a brief, a sweep) that dies with its round open leaves that round
/// to the next write of its conversation, which retries it before answering.
#[test]
fn a_tab_s_question_after_a_holder_died_is_answered_without_another_write() {
    use suzatary_lines::secretary::transcript;
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new("worker-dead-holder")
            .whois(whois)
            .model(FakeModel::saying([
                "{\"say\": \"the lost one, answered\"}",
                "{\"say\": \"the new one, answered\"}",
            ])),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    let id = open(&base);
    let conversation =
        suzatary_base::conversation_id::ConversationId::new(&id).expect("a conversation id");
    let conversations = fakes.conversations.conversations();
    // Written with no nudge, as another process writes, then claimed by that process's run.
    transcript::push(
        conversations,
        &conversation,
        &suzatary_conversations::secretary::store::Entry::Said {
            at: common::now(),
            turn: suzatary_conversations::secretary::model::Turn::Person {
                text: "the lost question".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: None,
        },
    )
    .expect("the question appends");
    {
        let holder = conversations.runs.begin().expect("the other holder's run");
        assert_eq!(
            transcript::claim(
                conversations,
                &conversation,
                &holder.run,
                common::now(),
                transcript::Retry::Allowed,
                &|_| true,
                |_| Vec::new(),
            )
            .expect("claims"),
            transcript::Claimed::Began
        );
        // The holder dies here: its run's lock goes with it.
    }

    say(&base, &id, "and the new one?");
    let payload = common::settled(&base, &id);
    let texts: Vec<&str> = payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .map(|turn| turn["text"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        texts,
        [
            "the lost question",
            "and the new one?",
            "the lost one, answered",
            "the new one, answered"
        ],
        "{payload}"
    );
    assert!(
        transcript::read(conversations, &conversation)
            .expect("reads")
            .owed()
            .is_empty()
    );
}

/// A question sealed while the box was down; past the bound its model is never asked, and each is
/// drawn once, as its one turn.
#[test]
fn a_question_handed_over_late_is_answered_within_the_bound_and_closed_past_it() {
    use suzatary_conversations::secretary::model::Turn;
    use suzatary_conversations::secretary::store::Entry;
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new("worker-late")
            .whois(whois)
            .model(FakeModel::saying(["{\"say\": \"answered, late\"}"])),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    let sealed_at = |hour: u32, minute: u32, text: &str| Entry::Said {
        at: prodrome_core::literal::Datetime::new(2026, 9, 2, hour, minute, 0, 0)
            .expect("an instant"),
        turn: Turn::Person {
            text: text.to_owned(),
            references: Vec::new(),
            attachments: Vec::new(),
        },
        by: Some(suzatary_inbox::secretary::stamp::Stamp::Actor(
            suzatary_inbox::secretary::stamp::Speaker::device(common::CAST.owner.actor),
        )),
    };
    let texts = |payload: &serde_json::Value| -> Vec<String> {
        payload["turns"]
            .as_array()
            .expect("turns")
            .iter()
            .map(|turn| turn["text"].as_str().unwrap_or_default().to_owned())
            .collect()
    };

    let within = open(&base);
    let late = common::sealed(&base, &within, &sealed_at(11, 30, "sent half an hour ago"));
    let _ = common::post_lines(&base, &late.body);
    let payload = common::settled(&base, &within);
    assert_eq!(texts(&payload), ["sent half an hour ago", "answered, late"]);

    let past = open(&base);
    let old = common::sealed(&base, &past, &sealed_at(10, 0, "sent two hours ago"));
    let _ = common::post_lines(&base, &old.body);
    let payload = common::settled(&base, &past);
    assert_eq!(
        texts(&payload),
        [
            "sent two hours ago",
            "This was not answered in time; ask again."
        ]
    );
    assert_eq!(
        fakes.model.asked().len(),
        1,
        "the late question asked nothing"
    );
}
