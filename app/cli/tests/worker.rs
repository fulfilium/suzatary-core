//! The conversation worker over a real server, `v4` committed to `ada@example.com` (see
//! `common`'s header on peer identities). Two scenes hold `common::Gate` across the model call,
//! since a fast fake could finish a round before the second `say` arrives.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes, Gate};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn writer(tag: &str, model: FakeModel) -> (String, Arc<suzatary_cli::serve::Workers>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new(tag).whois(whois).model(model);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, workers, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    (base, workers)
}

/// Like [`writer`], with a chosen idle wait rather than the real thirty seconds.
fn writer_idle(
    tag: &str,
    model: FakeModel,
    idle: std::time::Duration,
) -> (String, Arc<suzatary_cli::serve::Workers>) {
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
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    assert_eq!(common::status(&opened), 200);
    common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

fn say(base: &str, id: &str, text: &str) -> ureq::http::Response<ureq::Body> {
    common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": text}),
    )
}

/// Whether `say`'s own answer shows the person turn `text` as queued: the answer is built before
/// the worker is nudged, so this reads no race.
fn queued(answer: &mut ureq::http::Response<ureq::Body>, text: &str) -> bool {
    common::body_json(answer)["conversation"]["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .any(|turn| turn["who"] == "person" && turn["text"] == text && turn["queued"] == true)
}

/// Two `say`s during one turn produce one exchange, and both person lines are delivered.
///
/// `wait_until_entered` blocks until the fake has registered the call, since a 202 says nothing
/// about whether the worker had reached the model yet.
#[test]
fn two_says_in_one_turn_are_one_exchange() {
    let gate = Gate::new();
    let (base, workers) = writer(
        "worker-coalesce",
        FakeModel::saying(["{\"say\": \"got both\"}"]).gated(gate.clone()),
    );
    let id = open(&base);

    let mut first = say(&base, &id, "first");
    assert_eq!(common::status(&first), 202);
    assert!(
        queued(&mut first, "first"),
        "the turn just appended is queued in its own answer"
    );
    gate.wait_until_entered();

    let mut second = say(&base, &id, "second");
    assert_eq!(
        common::status(&second),
        202,
        "the worker is nudged, not raced"
    );
    assert!(
        queued(&mut second, "second"),
        "sent before the first settles, and queued too"
    );
    gate.release();

    let payload = common::settled(&base, &id);
    let turns = payload["turns"].as_array().expect("turns");
    assert_eq!(
        turns.len(),
        3,
        "two person turns and one model turn: {turns:?}"
    );
    assert_eq!(turns[0]["who"], "person");
    assert_eq!(turns[0]["text"], "first");
    assert_eq!(turns[1]["who"], "person");
    assert_eq!(turns[1]["text"], "second");
    assert_eq!(turns[2]["who"], "model");
    assert_eq!(turns[2]["text"], "got both");

    // And `queue::delta` is empty: the last turn is a finished model turn, which is what the
    // worker's idle check reads.
    let id = suzatary_core::secretary::store::ConversationId::new(id).expect("a slug");
    assert!(
        workers.is_running(&id),
        "still in the map until it drops itself idle"
    );
}

/// With the gate held so no round can complete, `MAX_PENDING` says all succeed and the next is
/// refused with the exact sentence, before it is appended.
#[test]
fn a_say_past_max_pending_is_refused_before_it_is_appended() {
    let gate = Gate::new();
    let (base, _workers) = writer(
        "worker-max-pending",
        FakeModel::saying(["{\"say\": \"eventually\"}"]).gated(gate.clone()),
    );
    let id = open(&base);

    let first = say(&base, &id, "line 0");
    assert_eq!(common::status(&first), 202);
    gate.wait_until_entered();

    for n in 1..suzatary_core::secretary::queue::MAX_PENDING {
        let response = say(&base, &id, &format!("line {n}"));
        assert_eq!(
            common::status(&response),
            202,
            "line {n} should still queue"
        );
    }

    let mut refused = say(&base, &id, "one too many");
    assert_eq!(common::status(&refused), 400);
    let body = common::body_json(&mut refused);
    assert_eq!(
        body["error"],
        format!(
            "{} lines are already waiting for the secretary; say it after they are answered",
            suzatary_core::secretary::queue::MAX_PENDING
        )
    );

    gate.release();
    let payload = common::settled(&base, &id);
    let turns = payload["turns"].as_array().expect("turns");
    let persons = turns.iter().filter(|turn| turn["who"] == "person").count();
    assert_eq!(
        persons,
        suzatary_core::secretary::queue::MAX_PENDING,
        "the refused line never joined the transcript: {turns:?}"
    );
}

/// The worker exits when idle: once a round finishes with nothing queued, the table holds no
/// entry for the conversation after the idle timeout, so no task leaks per conversation.
#[test]
fn an_idle_worker_drops_out_of_the_table() {
    // A short idle, not the real 30 s: this pins the mechanism, not the production constant.
    let idle = std::time::Duration::from_millis(300);
    let (base, workers) = writer_idle(
        "worker-idle",
        FakeModel::saying(["{\"say\": \"just the one\"}"]),
        idle,
    );
    let id = open(&base);

    let response = say(&base, &id, "hello");
    assert_eq!(common::status(&response), 202);
    common::settled(&base, &id);

    let id = suzatary_core::secretary::store::ConversationId::new(id).expect("a slug");
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

/// A rule whose outcome is recorded is never fired again by a restarted box: the memo that
/// lasts is the transcript. A second server over the same stores, told the conversation moved,
/// asks nothing, and answers only what is said after.
#[test]
fn a_restarted_box_refires_nothing_already_answered() {
    let asks = |model: &FakeModel| model.asked().len() + model.resumed_with().len();
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
    assert_eq!(common::status(&say(&base, &id, "hello")), 202);
    common::settled(&base, &id);
    assert_eq!(asks(&first.model), 1);

    let second = Arc::new(
        Fakes::over(root, suzatary_io::objects::Objects::chain(chain))
            .whois(whois())
            .model(FakeModel::saying(["{\"say\": \"the new one\"}"])),
    );
    let base = common::Server::new(Arc::clone(&second), deployment(), Vec::new())
        .start()
        .base;
    // A write the engine hears, on a reading whose rules are all discharged.
    let renamed = common::act(
        &format!("{base}/api/secretary/conversations/{id}/title"),
        &serde_json::json!({"title": "A renamed morning"}),
    );
    assert_eq!(common::status(&renamed), 200);
    assert_eq!(common::status(&say(&base, &id, "and now?")), 202);
    let payload = common::settled(&base, &id);
    assert_eq!(
        asks(&second.model),
        1,
        "the restarted box asked once, for the new line only"
    );
    let turns = payload["turns"].as_array().expect("turns");
    assert_eq!(turns.last().expect("a turn")["text"], "the new one");
}
