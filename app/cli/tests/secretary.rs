//! The secretary over a real server, with a scripted model and a recording transport.
//!
//! `v4` resolves to the writer `ada@example.com` and `v6` to the viewer `guest@example.com`.

mod common;

use std::sync::Arc;

use common::{FakeMailbox, FakeModel, Fakes, Recorder};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn guest() -> suzatary_core::capability::Login {
    common::login("guest@example.com")
}

/// The script the model answers from: it proposes, says it proposed, then answers an act's round.
fn script() -> FakeModel {
    FakeModel::saying([
        r#"{"say": "Here is the reply I would send.",
            "calls": [{"verb": "propose_email", "args": {
              "account": "her@example.invalid",
              "to": "counsel@example.test, paralegal@example.test",
              "cc": "me@example.test",
              "subject": "The deed documents",
              "body": "Sending the documents today, the last one is scanned.",
              "why": "Counsel asked for the two remaining exhibits and the last scan finished today.",
              "sources": "mail:a1b2c3d4e5f6, todo:life/permitfiling"
            }}]}"#,
        r#"{"say": "Proposed. Tap it when the wording is right."}"#,
        r#"{"say": "Understood, Mira it is."}"#,
    ])
}

/// A model that proposes signing `account` in through `propose_act`, then answers later rounds.
fn auth_script(account: &str) -> FakeModel {
    FakeModel::saying_owned([
        format!(
            r#"{{"say": "Here is the sign-in.", "calls": [{{"verb": "propose_act", "args": {{
              "act": "auth", "account": "{account}",
              "why": "the sync has failed for want of a grant since Friday",
              "sources": "briefing:2026-09-13"}}}}]}}"#
        ),
        r#"{"say": "Proposed. Tap it to sign in."}"#.to_owned(),
        r#"{"say": "Understood."}"#.to_owned(),
    ])
}

/// The mailbox `auth_script`'s account is on.
fn holding(account: &str) -> common::FakeMailbox {
    common::FakeMailbox::new().with_accounts(vec![account.to_owned()])
}

/// A server where `v4` writes as `ada` and `guest` may only read, with a recorder for transport.
fn writing(tag: &str) -> (String, Arc<Recorder>) {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new(tag).whois(whois).model(script());
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let common::Running { base, sent, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    (base, sent)
}

/// Open a conversation, say something, and poll until the model's answer lands.
fn exchange(base: &str, said: &str) -> serde_json::Value {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    assert_eq!(common::status(&opened), 200);
    let opened = common::body_json(&mut opened);
    let id = opened["id"].as_str().expect("an id").to_owned();

    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": said}),
    );
    assert_eq!(common::status(&said), 202, "a model turn answers 202");

    let payload = settled(base, &id);
    assert!(
        !payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "the exchange proposed nothing: {payload:?}"
    );
    payload
}

/// The conversation once the box says nobody is answering, polled on the derived `state`.
fn settled(base: &str, id: &str) -> serde_json::Value {
    for _ in 0..200 {
        let payload = common::reading(base, id, &[]);
        if payload["state"] == "idle" {
            return payload;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the exchange never finished");
}

/// A question names the turns its asker saw, so asking it again after its answer was lost is the
/// line already standing, and the same words asked later, having seen it, are a new line.
#[test]
fn a_question_asked_twice_is_asked_once() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new("secretary-asked-once")
        .whois(whois)
        .model(FakeModel::saying([
            r#"{"say": "one"}"#,
            r#"{"say": "two"}"#,
        ]));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::new(fakes), deployment, Vec::new())
        .start()
        .base;
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "asked once"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let say = |after: u64| {
        let response = common::act(
            &format!("{base}/api/secretary/conversations/{id}/say"),
            &serde_json::json!({"say": "yes", "after": after}),
        );
        assert_eq!(common::status(&response), 202);
        settled(&base, &id)
    };
    assert_eq!(say(0)["total"], 2, "the question and its answer");
    assert_eq!(
        say(0)["total"],
        2,
        "the same question, seen from the same turn, is that line"
    );
    assert_eq!(
        say(2)["total"],
        4,
        "having seen it, the same words are a new question"
    );
}

/// The model's `propose_email` stores a proposal whose payload carries every field a send needs.
#[test]
fn an_exchange_stores_a_proposal_with_the_exact_message() {
    let (base, _) = writing("secretary-exchange");
    let payload = exchange(&base, "propose the harrow reply");

    let turns = payload["turns"].as_array().expect("turns");
    assert_eq!(
        turns[0]["who"], "person",
        "the person's turn is first and is what the model was asked"
    );
    assert!(
        turns.iter().any(|turn| turn["who"] == "verb"
            && turn["verb"] == "propose_email"
            && turn["outcome"]["is"] == "answered"),
        "the verb ran and its answer is a turn: {turns:?}"
    );

    let proposals = payload["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 1);
    let proposal = &proposals[0];
    assert_eq!(proposal["state"], "standing");
    let act = &proposal["act"];
    assert_eq!(act["kind"], "email");
    assert_eq!(act["account"], "her@example.invalid");
    assert_eq!(
        act["to"],
        serde_json::json!(["counsel@example.test", "paralegal@example.test"]),
        "a comma-separated list becomes recipients, trimmed"
    );
    assert_eq!(act["cc"], serde_json::json!(["me@example.test"]));
    assert_eq!(act["subject"], "The deed documents");
    assert_eq!(
        act["body"],
        "Sending the documents today, the last one is scanned."
    );
    assert_eq!(
        proposal["why"],
        "Counsel asked for the two remaining exhibits and the last scan finished today."
    );
    assert_eq!(
        proposal["sources"],
        serde_json::json!(["mail:a1b2c3d4e5f6", "todo:life/permitfiling",]),
        "the sources reach the wire in their text form, which the page parses"
    );
}

/// A proposal with no why or source is refused as a turn the model can read.
#[test]
fn a_proposal_without_a_why_or_a_source_is_refused_to_the_model() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-groundless")
        .whois(whois)
        .model(FakeModel::saying([
            r#"{"say": "Here it is.",
                "calls": [{"verb": "propose_email", "args": {
                  "account": "her@example.invalid",
                  "to": "counsel@example.test",
                  "subject": "The deed documents",
                  "body": "Sending the documents today.",
                  "why": "Because it seemed right.",
                  "sources": ""
                }}]}"#,
            r#"{"say": "I could not name what this rests on."}"#,
        ]));
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base,
        sent: recorder,
        ..
    } = common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    let opened = common::body_json(&mut opened);
    let id = opened["id"].as_str().expect("an id").to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "propose the harrow reply"}),
    );
    assert_eq!(common::status(&said), 202);

    let payload = settled(&base, &id);
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "nothing was stored: {payload:?}"
    );
    let turns = payload["turns"].as_array().expect("turns");
    assert!(
        turns.iter().any(|turn| turn["who"] == "verb"
            && turn["verb"] == "propose_email"
            && turn["outcome"]["is"] == "refused"
            && turn["text"]
                .as_str()
                .unwrap_or_default()
                .contains("at least one thing this rests on")),
        "the model is told what it left out: {turns:?}"
    );
    assert!(recorder.sent().is_empty());
}

/// A deployment that declares a channel's secret, `NTFY_TOPIC`.
fn declared_secret() -> suzatary_core::settings::Settables {
    suzatary_core::settings::Settables::derive(
        suzatary_io::model::TOKEN,
        [suzatary_core::connect::Channel::Ntfy],
        None,
        &[],
    )
}

fn deployment_with_secret() -> suzatary_core::deployment::Deployment {
    common::deployment([ada(), guest()], [(ada(), common::actor("ada"))])
}

fn secret_model() -> FakeModel {
    FakeModel::saying([
        r#"{"say": "I will ask him for the key.",
            "calls": [{"verb": "propose_act", "args": {
              "act": "secret",
              "name": "NTFY_TOPIC",
              "why": "Notifications have been failing for want of a topic.",
              "sources": "todo:life/permitfiling"
            }}]}"#,
        r#"{"say": "Proposed. Tap it and type the key."}"#,
    ])
}

/// A tap with a value stores it under the name, and the conversation never carries the bytes.
#[test]
fn a_tap_with_a_value_stores_it_and_never_echoes_it() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-secret-good")
        .whois(whois)
        .model(secret_model());
    let recorder = Arc::new(common::SecretRecorder::new());
    let base = common::Server::new(Arc::new(fakes), deployment_with_secret(), Vec::new())
        .settings(common::SettingsFixture::with_writer(
            Arc::clone(&recorder) as _
        ))
        .settings_names(declared_secret())
        .start()
        .base;

    let payload = exchange(&base, "the notifications topic is missing");
    let id = payload["id"].as_str().expect("an id").to_owned();
    let act = &payload["proposals"][0]["act"];
    assert_eq!(act["kind"], "secret");
    assert_eq!(act["label"], "your notifications topic");
    assert!(!act.to_string().contains("NTFY_TOPIC"), "{act}");

    let distinctive = "sk-do-not-leak-the-topic";
    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({"value": distinctive}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "acted");
    assert_eq!(
        after["proposals"][0]["outcome"],
        "stored your notifications topic"
    );

    let raw = after.to_string();
    assert!(!raw.contains(distinctive), "{raw}");

    assert_eq!(
        recorder.calls(),
        vec![("NTFY_TOPIC".to_owned(), distinctive.to_owned())]
    );
}

/// A secret tap with no value is refused and the store is never asked.
#[test]
fn a_tap_with_no_value_is_refused_and_the_store_is_never_asked() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-secret-no-value")
        .whois(whois)
        .model(secret_model());
    let recorder = Arc::new(common::SecretRecorder::new());
    let base = common::Server::new(Arc::new(fakes), deployment_with_secret(), Vec::new())
        .settings(common::SettingsFixture::with_writer(
            Arc::clone(&recorder) as _
        ))
        .settings_names(declared_secret())
        .start()
        .base;

    let payload = exchange(&base, "the notifications topic is missing");
    let id = payload["id"].as_str().expect("an id").to_owned();

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 400);
    let body = common::body_json(&mut tapped);
    assert!(
        body.to_string().contains("value"),
        "the refusal names the missing field: {body:?}"
    );
    assert!(recorder.calls().is_empty());
}

/// A value on an `auth` act is refused before the performer runs.
#[test]
fn a_value_on_an_auth_act_is_refused_and_the_performer_never_runs() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-secret-smuggled")
        .whois(whois)
        .mailbox(FakeMailbox::new().with_accounts(vec!["her@example.invalid".to_owned()]))
        .model(FakeModel::saying([
            r#"{"say": "I will ask him to authorise it.",
                "calls": [{"verb": "propose_act", "args": {
                  "act": "auth",
                  "account": "her@example.invalid",
                  "why": "The Microsoft sync has been failing for want of a grant.",
                  "sources": "todo:life/permitfiling"
                }}]}"#,
            r#"{"say": "Proposed. Tap it and the sign-in code will land here."}"#,
        ]));
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base, acts: acting, ..
    } = common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();

    let payload = exchange(&base, "the Microsoft sync needs a grant");
    let id = payload["id"].as_str().expect("an id").to_owned();

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({"value": "should-never-reach-anything"}),
    );
    assert_eq!(common::status(&tapped), 400);
    let body = common::body_json(&mut tapped);
    assert!(
        body.to_string().contains("secret act"),
        "the refusal says a value belongs only to a secret act: {body:?}"
    );
    assert!(
        acting.begun().is_empty(),
        "a smuggled value must not start the flow: {:?}",
        acting.begun()
    );
}

/// An undeclared secret name is refused at propose time, and nothing is stored.
#[test]
fn an_undeclared_secret_name_is_refused_and_nothing_is_stored() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-secret-undeclared")
        .whois(whois)
        .model(FakeModel::saying([
            r#"{"say": "I will ask.",
                "calls": [{"verb": "propose_act", "args": {
                  "act": "secret",
                  "name": "GITHUB_PAT",
                  "why": "Testing an undeclared name.",
                  "sources": "todo:life/permitfiling"
                }}]}"#,
            r#"{"say": "That name is not declared."}"#,
        ]));
    let recorder = Arc::new(common::SecretRecorder::new());
    let base = common::Server::new(Arc::new(fakes), deployment_with_secret(), Vec::new())
        .settings(common::SettingsFixture::with_writer(
            Arc::clone(&recorder) as _
        ))
        .settings_names(declared_secret())
        .start()
        .base;

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    let opened = common::body_json(&mut opened);
    let id = opened["id"].as_str().expect("an id").to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "set GITHUB_PAT"}),
    );
    assert_eq!(common::status(&said), 202);

    let payload = settled(&base, &id);
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "nothing was stored: {payload:?}"
    );
    let turns = payload["turns"].as_array().expect("turns");
    assert!(
        turns.iter().any(|turn| turn["who"] == "verb"
            && turn["verb"] == "propose_act"
            && turn["outcome"]["is"] == "refused"
            && turn["text"]
                .as_str()
                .unwrap_or_default()
                .contains("declared")),
        "the model is told the name is not declared, never the list's members: {turns:?}"
    );
    assert!(recorder.calls().is_empty());
}

/// The standing proposal row's JSON has no `value` field.
#[test]
fn the_standing_proposal_row_carries_no_value_field() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-secret-no-value-field")
        .whois(whois)
        .model(secret_model());
    let base = common::Server::new(Arc::new(fakes), deployment_with_secret(), Vec::new())
        .settings(common::SettingsFixture::with_writer(Arc::new(
            common::SecretRecorder::new(),
        )))
        .settings_names(declared_secret())
        .start()
        .base;

    let payload = exchange(&base, "the notifications topic is missing");
    let row = &payload["proposals"][0];
    assert!(
        row.get("value").is_none() && row["act"].get("value").is_none(),
        "a proposal row must never carry a value field: {row:?}"
    );
}

/// A dismissal closes the card, leaves the Inbox, and reaches the secretary as a turn.
#[test]
fn a_dismissal_answers_a_proposal_and_the_transcript_says_so() {
    let (base, recorder) = writing("secretary-dismiss");
    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();

    let blank = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/dismiss"),
        &serde_json::json!({"reason": "  "}),
    );
    assert_eq!(common::status(&blank), 400, "a dismissal says why");

    let mut dismissed = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/dismiss"),
        &serde_json::json!({"reason": "wrong person, this goes to Mira"}),
    );
    assert_eq!(common::status(&dismissed), 200);
    let after = common::body_json(&mut dismissed);
    assert_eq!(after["proposals"][0]["state"], "dismissed");
    assert_eq!(
        after["proposals"][0]["outcome"],
        "wrong person, this goes to Mira"
    );
    // The dismissal answers with the transcript as it stood before the round it starts.
    assert_eq!(after["state"], "idle");

    let turns = after["turns"].as_array().expect("turns");
    let act = turns.last().expect("a last turn");
    assert_eq!(act["who"], "act");
    assert_eq!(
        act["text"],
        "Ada dismissed it: wrong person, this goes to Mira"
    );
    assert_eq!(
        act["references"],
        serde_json::json!([format!("proposal:{id}/0")]),
        "the act points at what it answered"
    );

    assert!(recorder.sent().is_empty(), "a dismissal sends nothing");

    // The act wakes a round; an act already reads as idle, so this waits for the answer.
    let heard = (0..400)
        .find_map(|_| {
            let payload = common::reading(&base, &id, &[]);
            let last = payload["turns"]
                .as_array()
                .and_then(|turns| turns.last().cloned());
            if last.is_some_and(|turn| turn["who"] == "model") {
                Some(payload)
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
                None
            }
        })
        .expect("the dismissal started a round and the model answered it");
    let turns = heard["turns"].as_array().expect("turns");
    let last = turns.last().expect("a last turn");
    assert_eq!(last["who"], "model", "the act started a round: {turns:?}");
    assert_eq!(last["text"], "Understood, Mira it is.");
    // The round loads what the act pointed at, so the act is the last act turn, not the prior one.
    let act_at = turns
        .iter()
        .rposition(|turn| turn["who"] == "act")
        .expect("the dismissal's act turn");
    assert!(act_at < turns.len() - 1, "{turns:?}");
}

/// The tap sends exactly the stored message, as the recorder's bytes show.
#[test]
fn a_tap_performs_the_stored_message() {
    let (base, recorder) = writing("secretary-tap");
    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();
    assert!(recorder.sent().is_empty(), "proposing sends nothing");

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "sent");

    let sent = recorder.sent();
    assert_eq!(sent.len(), 1, "exactly one message went");
    assert_eq!(sent[0].subject, "The deed documents");
    assert_eq!(
        sent[0].to,
        vec![
            "counsel@example.test".to_owned(),
            "paralegal@example.test".to_owned()
        ]
    );
    assert_eq!(
        sent[0].body, "Sending the documents today, the last one is scanned.",
        "the bytes sent are the bytes shown"
    );

    // An index nothing proposed is a 404, not a 500 and not a second send.
    let nowhere = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/7/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&nowhere), 404);
}

/// A tap with no note appends no turn and asks the model nothing.
#[test]
fn a_tap_with_no_note_asks_the_model_nothing() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new("secretary-tap-no-round")
        .whois(whois)
        .model(script());
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base,
        sent,
        workers,
        ..
    } = common::Server::new(Arc::new(fakes), deployment, Vec::new())
        .idle(std::time::Duration::from_millis(50))
        .start();
    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();
    let conversation =
        suzatary_core::secretary::store::ConversationId::new(id.clone()).expect("a slug");
    let idle = || {
        for _ in 0..200 {
            if !workers.is_running(&conversation) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("the worker never went idle");
    };
    idle();
    let before = settled(&base, &id)["turns"]
        .as_array()
        .expect("turns")
        .len();

    let tapped = answer(&base, &id, "tap", &serde_json::json!({}));
    assert_eq!(common::status(&tapped), 200);
    assert_eq!(sent.sent().len(), 1);
    idle();
    let quiet = settled(&base, &id);
    assert_eq!(
        quiet["turns"].as_array().expect("turns").len(),
        before,
        "no round ran: {quiet:?}"
    );

    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "send it to Mira too"}),
    );
    assert_eq!(common::status(&said), 202);
    let after = settled(&base, &id);
    let turns = after["turns"].as_array().expect("turns");
    assert_eq!(
        turns.len(),
        before + 2,
        "a question and its answer: {turns:?}"
    );
    assert_eq!(turns[before + 1]["text"], "Understood, Mira it is.");
}

/// A tap with a note appends a `Turn::Act` naming the proposal and wakes the secretary.
#[test]
fn a_tap_with_a_note_appends_an_act_turn_and_wakes_the_secretary() {
    let (base, recorder) = writing("secretary-tap-note");
    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();

    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({"note": "go ahead, but cc me on the reply next time"}),
    );
    assert_eq!(common::status(&tapped), 200);
    assert_eq!(
        recorder.sent().len(),
        1,
        "the tap still sends what it stored"
    );

    let heard = (0..400)
        .find_map(|_| {
            let payload = common::reading(&base, &id, &[]);
            let last = payload["turns"]
                .as_array()
                .and_then(|turns| turns.last().cloned());
            if last.is_some_and(|turn| turn["who"] == "model") {
                Some(payload)
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
                None
            }
        })
        .expect("the tap's note started a round and the model answered it");
    let turns = heard["turns"].as_array().expect("turns");
    let act = turns
        .iter()
        .rev()
        .find(|turn| turn["who"] == "act")
        .expect("the tap's act turn");
    assert_eq!(
        act["text"],
        "Ada tapped it: go ahead, but cc me on the reply next time"
    );
    assert_eq!(
        act["references"],
        serde_json::json!([format!("proposal:{id}/0")])
    );
}

/// A changeset's `add` and `complete` land together on tap, as `assistant`.
#[test]
fn a_changeset_add_and_complete_land_together_on_tap() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-changeset-tap")
        .whois(whois)
        .model(FakeModel::saying([
            r#"{"say": "I will propose adding and closing it.",
                "calls": [{"verb": "propose_changes", "args": {
                  "chain": "life",
                  "acts": [
                    {"act": "add", "todo": "secretarychangetest",
                     "body": "A changeset test todo", "rationale": "testing the tap",
                     "priority": "50%"},
                    {"act": "complete", "todo": "secretarychangetest", "note": "done, testing"}
                  ],
                  "why": "Testing that a changeset's acts land together.",
                  "sources": "todo:life/permitfiling"
                }}]}"#,
            r#"{"say": "Proposed. Tap it when ready."}"#,
        ]));
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base,
        sent: recorder,
        ..
    } = common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();

    let payload = exchange(&base, "add and close the changeset test todo");
    let id = payload["id"].as_str().expect("an id").to_owned();
    let proposal = &payload["proposals"][0];
    assert_eq!(proposal["state"], "standing");
    assert_eq!(proposal["act"]["kind"], "changes");
    assert_eq!(proposal["act"]["chain"], "life");
    let rows = proposal["act"]["changes"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "an add and a complete: {rows:?}");
    assert_eq!(proposal["act"]["problem"], "");

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "applied");
    assert!(recorder.sent().is_empty(), "a changeset never sends mail");

    let mut todo = common::get(&format!("{base}/api/todo/secretarychangetest"));
    assert_eq!(common::status(&todo), 200);
    let todo = common::body_json(&mut todo);
    let kinds: Vec<&str> = todo["stream"]
        .as_array()
        .expect("a stream")
        .iter()
        .map(|line| line["kind"].as_str().unwrap_or_default())
        .collect();
    // An `add` is one `Authored` and nothing else; nothing in this application writes `Created`.
    // The completion rests on the content record its writer saw, so the stream orders them.
    assert_eq!(
        kinds,
        vec!["Authored", "Completed"],
        "the add's content record and the completion, in order: {kinds:?}"
    );
}

/// A changeset whose second act collides with its first is refused whole at propose time.
#[test]
fn a_changeset_with_one_bad_act_stores_and_appends_nothing() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-changeset-bad-act")
        .whois(whois)
        .model(FakeModel::saying([
            r#"{"say": "I will propose two adds.",
                "calls": [{"verb": "propose_changes", "args": {
                  "chain": "life",
                  "acts": [
                    {"act": "add", "todo": "secretarychangetest2",
                     "body": "Should never land", "rationale": "testing atomicity",
                     "priority": "50%"},
                    {"act": "add", "todo": "secretarychangetest2",
                     "body": "Already added by act 1", "rationale": "testing atomicity",
                     "priority": "40%"}
                  ],
                  "why": "Testing that a bad act refuses the whole changeset.",
                  "sources": "todo:life/permitfiling"
                }}]}"#,
            r#"{"say": "That did not work."}"#,
        ]));
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base,
        sent: recorder,
        ..
    } = common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    let opened = common::body_json(&mut opened);
    let id = opened["id"].as_str().expect("an id").to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "add secretarychangetest2, twice"}),
    );
    assert_eq!(common::status(&said), 202);

    let payload = settled(&base, &id);
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "a changeset with a bad act is never stored: {payload:?}"
    );
    let turns = payload["turns"].as_array().expect("turns");
    assert!(
        turns.iter().any(|turn| turn["who"] == "verb"
            && turn["verb"] == "propose_changes"
            && turn["outcome"]["is"] == "refused"
            && turn["text"].as_str().unwrap_or_default().contains("act 2")
            && turn["text"]
                .as_str()
                .unwrap_or_default()
                .contains("secretarychangetest2")),
        "the model is told which act, and why: {turns:?}"
    );
    assert!(recorder.sent().is_empty());

    assert_eq!(
        common::status(&common::get(&format!(
            "{base}/api/todo/secretarychangetest2"
        ))),
        404
    );
}

/// A viewer reads the conversations and can neither tap nor dismiss.
#[test]
fn a_viewer_may_read_the_conversations_and_tap_nothing() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-viewer").whois(whois).model(script());
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::new(fakes), deployment, Vec::new())
        .v6()
        .start()
        .base;

    let listing = common::get(&format!("{base}/api/stores/lines/objects"));
    assert_eq!(
        common::status(&listing),
        200,
        "a viewer may see what has been asked"
    );

    for (path, body) in [
        ("/api/secretary/conversations", serde_json::json!({})),
        (
            "/api/secretary/conversations/anything/say",
            serde_json::json!({"say": "hello"}),
        ),
        (
            "/api/secretary/conversations/anything/stop",
            serde_json::json!({}),
        ),
        (
            "/api/secretary/conversations/anything/proposals/0/tap",
            serde_json::json!({}),
        ),
        (
            "/api/secretary/conversations/anything/proposals/0/dismiss",
            serde_json::json!({"reason": "no"}),
        ),
        (
            "/api/secretary/conversations/anything/proposals/0/acknowledge",
            serde_json::json!({}),
        ),
    ] {
        let response = common::act(&format!("{base}{path}"), &body);
        assert_eq!(
            common::status(&response),
            403,
            "{path} let a viewer through"
        );
    }
}

/// What a person's turn points at is loaded through the model's own verbs before it is asked.
#[test]
fn a_turn_carries_what_it_points_at_and_the_box_loads_it() {
    let (base, _) = writing("secretary-references");
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    let opened = common::body_json(&mut opened);
    let id = opened["id"].as_str().expect("an id").to_owned();

    let refused = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({
            "say": "look at this",
            "references": ["todo:../../etc/passwd"],
        }),
    );
    assert_eq!(common::status(&refused), 400);

    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({
            "say": "is this still worth doing?",
            "references": ["todo:life/permitfiling"],
        }),
    );
    assert_eq!(common::status(&said), 202);
    let payload = settled(&base, &id);

    let turns = payload["turns"].as_array().expect("turns");
    assert_eq!(turns[0]["who"], "person");
    assert_eq!(
        turns[0]["references"],
        serde_json::json!(["todo:life/permitfiling"]),
        "the chip is stored on the turn it was attached to"
    );
    assert!(
        turns.iter().any(|turn| turn["who"] == "verb"
            && turn["verb"] == "reference"
            && turn["text"]
                .as_str()
                .unwrap_or_default()
                .starts_with("todo:life/permitfiling")),
        "the box looked it up and filed the answer as a turn: {turns:?}"
    );
}

/// Loopback may neither tap nor dismiss, since it resolves to no peer.
#[test]
fn loopback_may_neither_tap_nor_dismiss() {
    let fakes = Fakes::new("secretary-loopback")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .model(script());
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    // Exempt, so the request arrives as the box's own shell rather than as a named peer.
    let base = common::start(Arc::new(fakes), deployment, common::loopback_is_this_box());

    for (path, body) in [
        (
            "/api/secretary/conversations/anything/proposals/0/tap",
            serde_json::json!({}),
        ),
        (
            "/api/secretary/conversations/anything/proposals/0/dismiss",
            serde_json::json!({"reason": "no"}),
        ),
        (
            "/api/secretary/conversations/anything/proposals/0/acknowledge",
            serde_json::json!({}),
        ),
    ] {
        let response = common::act(&format!("{base}{path}"), &body);
        assert_eq!(
            common::status(&response),
            403,
            "{path} let this box's own shell through"
        );
    }
}

/// An empty turn is refused, and an unopened conversation is a 404.
#[test]
fn the_two_refusals_a_say_can_earn() {
    let (base, _) = writing("secretary-refusals");
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    let opened = common::body_json(&mut opened);
    let id = opened["id"].as_str().expect("an id").to_owned();

    let blank = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "   "}),
    );
    assert_eq!(common::status(&blank), 400);

    let nowhere = common::act(
        &format!("{base}/api/secretary/conversations/nosuchthing/say"),
        &serde_json::json!({"say": "hello"}),
    );
    assert_eq!(common::status(&nowhere), 404);

    let traversal = common::get(&format!(
        "{base}/api/secretary/conversations/..%2Fetc/blocks/{}",
        "0".repeat(64)
    ));
    assert!(
        matches!(common::status(&traversal), 400 | 404),
        "a path-shaped id must not read a file"
    );
}

/// A turn whose typst does not compile is served as its source, block by block.
#[test]
fn a_broken_answer_is_its_source_block_by_block() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let model = common::FakeModel::saying_owned([format!(
        r#"{{"say": "{}"}}"#,
        common::BREAKS_THE_COMPILE
    )]);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::start(
        Arc::new(
            Fakes::new("secretary-render-broken")
                .whois(whois)
                .model(model),
        ),
        deployment,
        Vec::new(),
    );

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "say something broken"}),
    );
    assert_eq!(common::status(&said), 202);
    let payload = settled(&base, &id);
    let turns = payload["turns"].as_array().expect("turns");
    let model_turn = turns
        .iter()
        .find(|turn| turn["who"] == "model")
        .expect("the model answered a turn");
    assert!(
        model_turn["blocks"]
            .as_array()
            .expect("blocks")
            .iter()
            .all(|block| block["html"].is_null()),
        "typst refuses this text, so every block is its source: {model_turn:?}"
    );
}

/// `secretary brief` without `--new` says into [`DEFAULT`]; with it, into a fresh dated id.
mod default_conversation {
    use suzatary_core::secretary::store::{ConversationId, DEFAULT};
    use suzatary_core::secretary::{converse, read_or_open_default, start};

    use super::{ada, common};

    /// A reply that calls no verb.
    fn simple_reply() -> common::FakeModel {
        common::FakeModel::saying(["{\"say\": \"Nothing urgent since yesterday.\"}"])
    }

    #[test]
    fn brief_lands_in_the_default_conversation_opening_it_first() {
        let fakes = common::Fakes::new("brief-default").model(simple_reply());
        let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
        let id = DEFAULT.clone();

        let opened = read_or_open_default(
            fakes.conversations.conversations(),
            &fakes.model,
            &id,
            common::now(),
        )
        .expect("opens lazily");
        assert_eq!(opened.title.get().map(String::as_str), Some("Secretary"));
        assert!(opened.turns.is_empty());

        let answered = converse(
            &fakes.secretary(),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &[],
            &id,
            suzatary_core::secretary::Saying {
                text: "What changed since the last briefing?",
                references: &[],
                by: None,
            },
            "system prompt",
        )
        .expect("the scripted model answers")
        .expect("a round ran");
        assert!(
            !answered.stored.turns.is_empty(),
            "the brief's own turn and the model's answer both landed"
        );

        let again = read_or_open_default(
            fakes.conversations.conversations(),
            &fakes.model,
            &id,
            common::now(),
        )
        .expect("reads what is there");
        assert_eq!(again.title.get().map(String::as_str), Some("Secretary"));
        assert_eq!(
            again.opened, opened.opened,
            "the same conversation, opened once"
        );
        assert_eq!(
            again.turns.len(),
            answered.stored.turns.len(),
            "the morning's turns are still there, and reading again adds none"
        );
    }

    #[test]
    fn new_still_mints_a_dated_conversation_and_never_touches_the_default() {
        let fakes = common::Fakes::new("brief-new").model(simple_reply());
        let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
        let taken = suzatary_core::secretary::transcript::keys(fakes.conversations.conversations())
            .expect("reads the empty conversations directory");
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 14).expect("a day");
        let id = ConversationId::minted(today, &taken);
        assert_ne!(id.as_str(), DEFAULT.as_str(), "a dated id, not the default");

        start(
            fakes.conversations.conversations(),
            &fakes.model,
            &id,
            "Morning",
            suzatary_core::secretary::store::Kind::Topic,
            common::now(),
        )
        .expect("opens a fresh, dated conversation");
        let answered = converse(
            &fakes.secretary(),
            &deployment,
            &suzatary_core::settings::Settables::default(),
            &[],
            &id,
            suzatary_core::secretary::Saying {
                text: "What changed since the last briefing?",
                references: &[],
                by: None,
            },
            "system prompt",
        )
        .expect("the scripted model answers")
        .expect("a round ran");
        assert!(!answered.stored.turns.is_empty());

        let missing = suzatary_core::secretary::transcript::lines(
            fakes.conversations.conversations(),
            &DEFAULT,
        );
        assert!(
            missing.is_err(),
            "a --new brief must never open the default conversation"
        );
    }
}

/// A tap on an act runs the performer once with the tap as consent; a second tap is refused.
#[test]
fn a_tap_on_an_act_begins_it_once_and_records_what_the_performer_said() {
    use suzatary_core::secretary::acts::Act;

    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Arc::new(
        Fakes::new("secretary-act-tap")
            .whois(whois)
            .model(auth_script("ada@example.com"))
            .mailbox(holding("ada@example.com")),
    );
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base,
        sent: recorder,
        acts: acting,
        ..
    } = common::Server::new(Arc::clone(&fakes), deployment, Vec::new()).start();

    let id = exchange(&base, "sign in again")["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let account =
        suzatary_core::mail::address::EmailAddress::new("ada@example.com").expect("an address");
    assert!(acting.begun().is_empty(), "proposing performs nothing");

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "started", "{after}");
    let begun = acting.begun();
    assert_eq!(begun.len(), 1, "exactly one act was begun");
    assert_eq!(begun[0].0, id);
    assert_eq!(begun[0].1, 0);
    assert_eq!(begun[0].2, Act::Auth { account });
    assert!(recorder.sent().is_empty(), "an act sends no mail");
}

/// The real device flow over a scripted OAuth endpoint announces the code, then the grant.
#[test]
fn a_tap_on_a_real_act_announces_the_code_and_then_the_grant() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new("secretary-real-act")
            .whois(whois)
            .model(auth_script("her@study.test"))
            .mailbox(holding("her@study.test")),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);

    // One pending poll a second apart before the grant, so the tap's own answer is read while
    // the flow still runs, however long that read takes.
    let oauth = common::ScriptedOauth::new(vec![
        Ok(common::answered(common::ScriptedOauth::device_code(
            "ABCD-EFGH",
            1,
            900,
        ))),
        Ok(common::answered(
            serde_json::json!({"error": "authorization_pending"}),
        )),
        Ok(common::answered(common::ScriptedOauth::granted(
            "refresh-from-the-act",
        ))),
    ]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .real_acts(
            "real-act",
            "her@study.test",
            common::again(&fakes.conversations),
            oauth,
        )
        .start()
        .base;

    let id = exchange(&base, "sign in again")["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "started", "{after}");

    let turns = after["turns"].as_array().expect("turns");
    let announced = turns.last().expect("at least the announced turn");
    assert_eq!(
        announced["who"], "box",
        "the box relays the sign-in, not the owner"
    );
    let text = announced["text"].as_str().expect("text");
    assert!(text.contains("ABCD-EFGH"), "{text}");
    assert!(text.contains("https://microsoft.com/devicelogin"), "{text}");
    assert!(text.contains("her@study.test"), "{text}");
    // Only `message`, `verification_uri`, `user_code` and the expiry may reach a turn.
    assert!(
        !text.contains("the-secret-code-for-ABCD-EFGH"),
        "the device code's own bytes must never reach a turn: {text}"
    );

    let before = act_turns(turns);
    let acted = common::poll_conversation(&base, &id, std::time::Duration::from_secs(5), |body| {
        body["proposals"][0]["state"] == "acted"
            && body["turns"]
                .as_array()
                .is_some_and(|turns| act_turns(turns) > before)
    });
    assert!(
        acted["proposals"][0]["outcome"]
            .as_str()
            .expect("outcome")
            .contains("her@study.test"),
        "{acted}"
    );
    let turns_after = acted["turns"].as_array().expect("turns");
    assert_eq!(act_turns(turns_after), act_turns(turns) + 1);
    assert!(last_act(turns_after).contains("her@study.test"), "{acted}");
}

/// How many of the box's lines a transcript holds, since a tap's round may add a model turn too.
fn act_turns(turns: &[serde_json::Value]) -> usize {
    turns.iter().filter(|turn| turn["who"] == "box").count()
}

/// The text of the box's last line.
fn last_act(turns: &[serde_json::Value]) -> &str {
    turns
        .iter()
        .rev()
        .find(|turn| turn["who"] == "box")
        .and_then(|turn| turn["text"].as_str())
        .expect("a final act turn with text")
}

/// An unknown account is refused before the transport is asked.
#[test]
fn a_real_act_for_an_account_not_on_the_roster_fails_and_asks_the_transport_nothing() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new("secretary-real-act-unknown")
            .whois(whois)
            .model(auth_script("nope@elsewhere.test"))
            .mailbox(holding("nope@elsewhere.test")),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);

    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .real_acts(
            "real-act-unknown",
            "her@study.test",
            common::again(&fakes.conversations),
            common::ScriptedOauth::new(Vec::new()),
        )
        .start()
        .base;

    let id = exchange(&base, "sign in again")["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "failed", "{after}");
    let reason = after["proposals"][0]["outcome"].as_str().expect("a reason");
    assert!(reason.contains("nope@elsewhere.test"), "{reason}");
    assert!(
        !reason.contains("ran out of answers"),
        "the transport must never have been asked: {reason}"
    );
}

/// `revises` leaves exactly one proposal waiting and marks the earlier one superseded.
#[test]
fn revises_leaves_exactly_one_waiting() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-decision-revises")
        .whois(whois)
        .model(FakeModel::saying([
            r#"{"say": "First draft.",
                "calls": [{"verb": "propose_email", "args": {
                  "account": "her@example.invalid",
                  "to": "counsel@example.test",
                  "subject": "The deed documents",
                  "body": "Sending the documents today.",
                  "why": "Counsel asked for the exhibits.",
                  "sources": "mail:a1b2c3d4e5f6"
                }}]}"#,
            r#"{"say": "Proposed the first draft."}"#,
            r#"{"say": "Actually the subject was wrong, here is the fix.",
                "calls": [{"verb": "propose_email", "args": {
                  "account": "her@example.invalid",
                  "to": "counsel@example.test",
                  "subject": "The deed documents, corrected",
                  "body": "Sending the documents today, corrected subject.",
                  "why": "The first draft had the wrong subject line.",
                  "sources": "mail:a1b2c3d4e5f6",
                  "revises": 0
                }}]}"#,
            r#"{"say": "Proposed the corrected draft."}"#,
        ]));
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::new(fakes), deployment, Vec::new())
        .start()
        .base;

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A morning"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    let first = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "propose the harrow reply"}),
    );
    assert_eq!(common::status(&first), 202);
    let after_first = settled(&base, &id);
    assert_eq!(
        after_first["proposals"]
            .as_array()
            .expect("proposals")
            .len(),
        1
    );

    let second = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "fix the subject and resend"}),
    );
    assert_eq!(common::status(&second), 202);
    let after_second = settled(&base, &id);
    let proposals = after_second["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 2, "{proposals:?}");
    assert_eq!(
        proposals[0]["state"], "superseded",
        "the revised draft supersedes the one it replaces, never a dismissal in Ada's own words: {proposals:?}"
    );
    assert_eq!(proposals[1]["state"], "standing");
}

/// `withdraw_proposal` leaves none waiting and reads as withdrawn with the reason, never as the
/// owner's no.
#[test]
fn withdraw_leaves_none_waiting() {
    let whois = common::FakeWhois::new()
        .naming(common::v4(), ada())
        .naming(common::v6(), guest());
    let fakes = Fakes::new("secretary-decision-withdraw")
        .whois(whois)
        .model(FakeModel::saying([
            r#"{"say": "Here is the draft.",
                "calls": [{"verb": "propose_email", "args": {
                  "account": "her@example.invalid",
                  "to": "counsel@example.test",
                  "subject": "The deed documents",
                  "body": "Sending the documents today.",
                  "why": "Counsel asked for the exhibits.",
                  "sources": "mail:a1b2c3d4e5f6"
                }}]}"#,
            r#"{"say": "Proposed."}"#,
            r#"{"say": "He called and it is resolved, withdrawing this.",
                "calls": [{"verb": "withdraw_proposal", "args": {
                  "index": 0,
                  "why": "He called counsel directly; this draft is moot."
                }}]}"#,
            r#"{"say": "Withdrawn."}"#,
        ]));
    let deployment = common::deployment([ada(), guest()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base,
        sent: recorder,
        ..
    } = common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();

    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();

    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "he called counsel himself, withdraw the draft"}),
    );
    assert_eq!(common::status(&said), 202);
    let after = settled(&base, &id);
    let proposals = after["proposals"].as_array().expect("proposals");
    assert_eq!(
        proposals.len(),
        1,
        "withdrawing appends no new proposal: {proposals:?}"
    );
    assert_eq!(proposals[0]["state"], "withdrawn");
    assert_eq!(
        proposals[0]["outcome"],
        "He called counsel directly; this draft is moot."
    );
    assert!(recorder.sent().is_empty(), "withdrawing sends nothing");
}

/// Every proposal the Inbox lists, read from the tab's replica of the inbox.
fn inbox(base: &str) -> Vec<serde_json::Value> {
    common::inbox_listing(base, "2026-10-03T12:00:00")
}

/// The Inbox lists each proposal open until a tap completes it, and none of it is on `life`.
#[test]
fn the_inbox_lists_each_proposal_and_where_it_stands() {
    let proposing = r#"{"say": "Here is the reply.",
        "calls": [{"verb": "propose_email", "args": {
          "account": "her@example.invalid", "to": "counsel@example.test",
          "subject": "The deed documents", "body": "Sending the documents today.",
          "why": "Counsel asked for the exhibits.", "sources": "mail:a1b2c3d4e5f6"}}]}"#;
    let to_mira = r#"{"say": "Here is the reply.",
        "calls": [{"verb": "propose_email", "args": {
          "account": "her@example.invalid", "to": "mira@example.test",
          "subject": "The deed documents", "body": "Sending the documents today.",
          "why": "Counsel asked for the exhibits.", "sources": "mail:a1b2c3d4e5f6"}}]}"#;
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new("secretary-inbox-route")
        .whois(whois)
        .model(FakeModel::saying([
            proposing,
            r#"{"say": "Proposed."}"#,
            to_mira,
            r#"{"say": "Proposed."}"#,
            r#"{"say": "Understood."}"#,
        ]));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base,
        sent: recorder,
        ..
    } = common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    let tapped = exchange(&base, "propose the harrow reply");
    let tapped_id = tapped["id"].as_str().expect("an id").to_owned();
    let dismissed = exchange(&base, "propose the harrow reply again");
    let dismissed_id = dismissed["id"].as_str().expect("an id").to_owned();

    let before = inbox(&base);
    assert_eq!(before.len(), 2, "{before:?}");
    assert!(
        before
            .iter()
            .all(|row| row["state"]["kind"] == "open" && row["value"] == "absent")
    );
    assert_eq!(
        before[0]["asked"],
        serde_json::json!(format!("proposal:{tapped_id}/0"))
    );

    assert_eq!(
        common::status(&answer(&base, &tapped_id, "tap", &serde_json::json!({}))),
        200
    );
    assert_eq!(recorder.sent().len(), 1);
    assert_eq!(
        common::status(&answer(
            &base,
            &dismissed_id,
            "dismiss",
            &serde_json::json!({"reason": "wrong person, this goes to Mira"})
        )),
        200
    );
    let after = inbox(&base);
    assert_eq!(after[0]["state"]["kind"], "completed", "{after:?}");
    assert_eq!(after[1]["state"]["kind"], "cancelled", "{after:?}");
    assert!(after[1]["state"]["at"].is_string(), "{after:?}");

    let mut fold = common::get(&format!("{base}/api/fold"));
    let fold = common::body_json(&mut fold);
    assert!(
        fold["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .all(|entry| entry["authored"]["block"]["kind"] != "decision"),
        "a proposal writes nothing on life: {fold:?}"
    );
}

/// A server whose transport fails the first sends as `failures` says.
fn failing(
    tag: &str,
    failures: impl IntoIterator<Item = suzatary_core::secretary::responded::Performed>,
) -> (String, Arc<Recorder>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new(tag).whois(whois).model(script());
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, sent, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new())
            .failing(failures)
            .start();
    (base, sent)
}

/// POST one answer to proposal 0 of `id`: `tap`, or `dismiss` with a reason.
fn answer(
    base: &str,
    id: &str,
    verb: &str,
    body: &serde_json::Value,
) -> ureq::http::Response<ureq::Body> {
    common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/{verb}"),
        body,
    )
}

/// A send that sent nothing offers Try again, which sends once, and a third tap sends nothing.
#[test]
fn a_send_that_sent_nothing_is_tried_again() {
    let (base, recorder) = failing(
        "secretary-decision-failed-send",
        [suzatary_core::secretary::responded::Tried::NotSent {
            reason: "auth: the tenant has SMTP AUTH off".to_owned(),
        }
        .into()],
    );
    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();

    let mut tapped = answer(&base, &id, "tap", &serde_json::json!({}));
    assert_eq!(
        common::status(&tapped),
        200,
        "a refused send is recorded, not a 500"
    );
    let after = common::body_json(&mut tapped);
    let card = &after["proposals"][0];
    assert_eq!(card["state"], "failed");
    assert_eq!(card["tries"], 1);
    assert_eq!(card["card"]["answers"][0]["label"], "Try again");
    assert!(recorder.sent().is_empty());

    let mut again = answer(&base, &id, "tap", &serde_json::json!({}));
    assert_eq!(common::status(&again), 200);
    let after = common::body_json(&mut again);
    assert_eq!(after["proposals"][0]["state"], "sent");
    assert_eq!(after["proposals"][0]["tries"], 2);
    assert_eq!(recorder.sent().len(), 1, "sent once, by the second tap");

    let mut third = answer(&base, &id, "tap", &serde_json::json!({}));
    assert_eq!(common::status(&third), 200, "the send on record answers");
    assert_eq!(
        common::body_json(&mut third)["proposals"][0]["state"],
        "sent"
    );
    assert_eq!(recorder.sent().len(), 1, "a sent message is sent once");
}

/// A send that may have gone waits on the owner's acknowledgement and never sends again.
#[test]
fn a_send_that_may_have_gone_waits_until_acknowledged_and_never_sends_again() {
    let (base, recorder) = failing(
        "secretary-decision-unknown-send",
        [suzatary_core::secretary::responded::Done::MayHaveGone {
            reason: "the message was handed over and no answer came: reset".to_owned(),
        }
        .into()],
    );
    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();

    let mut tapped = answer(&base, &id, "tap", &serde_json::json!({}));
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "unknown");
    assert_eq!(
        after["proposals"][0]["card"]["answers"],
        serde_json::json!([])
    );
    assert_eq!(
        inbox(&base)[0]["state"]["kind"],
        "open",
        "listed until acknowledged"
    );

    let mut again = answer(&base, &id, "tap", &serde_json::json!({}));
    assert_eq!(
        common::status(&again),
        200,
        "the response on record answers"
    );
    assert_eq!(
        common::body_json(&mut again)["proposals"][0]["state"],
        "unknown"
    );
    let dismissed = answer(
        &base,
        &id,
        "dismiss",
        &serde_json::json!({"reason": "it went, Sent has it"}),
    );
    assert_eq!(common::status(&dismissed), 409);

    let mut checked = answer(&base, &id, "acknowledge", &serde_json::json!({}));
    assert_eq!(common::status(&checked), 200);
    assert_eq!(
        common::body_json(&mut checked)["proposals"][0]["state"],
        "checked"
    );
    assert_eq!(
        inbox(&base)[0]["state"]["kind"],
        "completed",
        "gone once checked"
    );

    let mut after_check = answer(&base, &id, "tap", &serde_json::json!({}));
    assert_eq!(common::status(&after_check), 200);
    assert_eq!(
        common::body_json(&mut after_check)["proposals"][0]["state"],
        "checked"
    );
    assert!(recorder.sent().is_empty(), "never sent at all");
}

/// Only a send that may have gone is acknowledged.
#[test]
fn a_standing_proposal_is_not_acknowledged() {
    let (base, _) = writing("secretary-acknowledge-standing");
    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();
    assert_eq!(
        common::status(&answer(&base, &id, "acknowledge", &serde_json::json!({}))),
        409
    );
}

/// The same email proposed in two conversations is one inbox object and sends once.
#[test]
fn the_same_email_proposed_twice_is_one_object_and_sends_once() {
    let proposing = r#"{"say": "Here is the reply.",
        "calls": [{"verb": "propose_email", "args": {
          "account": "her@example.invalid", "to": "counsel@example.test",
          "subject": "The deed documents", "body": "Sending the documents today.",
          "why": "Counsel asked for the exhibits.", "sources": "mail:a1b2c3d4e5f6"}}]}"#;
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new("secretary-same-email-twice")
        .whois(whois)
        .model(FakeModel::saying([
            proposing,
            r#"{"say": "Proposed."}"#,
            proposing,
            r#"{"say": "Proposed again."}"#,
        ]));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running {
        base,
        sent: recorder,
        ..
    } = common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    let first = exchange(&base, "propose the harrow reply");
    let second = exchange(&base, "propose the harrow reply");
    let (one, two) = (
        first["id"].as_str().expect("an id").to_owned(),
        second["id"].as_str().expect("an id").to_owned(),
    );
    assert_ne!(one, two, "two conversations");

    assert_eq!(inbox(&base).len(), 1, "one object");

    assert_eq!(
        common::status(&answer(&base, &one, "tap", &serde_json::json!({}))),
        200
    );
    let mut again = answer(&base, &two, "tap", &serde_json::json!({}));
    assert_eq!(common::status(&again), 200);
    assert_eq!(
        common::body_json(&mut again)["proposals"][0]["state"],
        "sent"
    );
    assert_eq!(recorder.sent().len(), 1, "sent once");
}

/// A send that sent nothing may be dismissed.
#[test]
fn a_send_that_sent_nothing_may_be_dismissed() {
    let (base, _) = failing(
        "secretary-decision-failed-dismissed",
        [suzatary_core::secretary::responded::Tried::NotSent {
            reason: "connect: refused".to_owned(),
        }
        .into()],
    );
    let payload = exchange(&base, "propose the harrow reply");
    let id = payload["id"].as_str().expect("an id").to_owned();
    assert_eq!(
        common::status(&answer(&base, &id, "tap", &serde_json::json!({}))),
        200
    );
    let mut dismissed = answer(
        &base,
        &id,
        "dismiss",
        &serde_json::json!({"reason": "not now"}),
    );
    assert_eq!(common::status(&dismissed), 200);
    assert_eq!(
        common::body_json(&mut dismissed)["proposals"][0]["state"],
        "dismissed"
    );
}

/// A photo is stored, served as the type its bytes say, and named by a turn.
#[test]
fn a_photo_is_kept_served_as_an_image_and_named_by_a_turn() {
    let (base, _) = writing("photo");
    // FF D8 FF E0 starts a JPEG, which is all the sniff reads.
    let mut stored = common::act(
        &format!("{base}/api/secretary/attachments"),
        &serde_json::json!({"file": "/9j/4A=="}),
    );
    assert_eq!(common::status(&stored), 200);
    let stored = common::body_json(&mut stored);
    assert_eq!(stored["kind"], "kept");
    let stored = &stored["file"];
    assert_eq!(stored["media_type"], "image/jpeg");
    assert_eq!(stored["name"], "", "a photo has no name");
    assert_eq!(stored["private"], "", "on the box");
    let id = stored["id"].as_str().expect("an id").to_owned();

    let mut served = common::get(&format!("{base}/api/secretary/attachments/{id}"));
    assert_eq!(common::status(&served), 200);
    assert_eq!(
        served.headers().get("content-type").expect("typed"),
        "image/jpeg"
    );
    assert_eq!(
        served
            .headers()
            .get("x-content-type-options")
            .expect("nosniff"),
        "nosniff"
    );
    let bytes = served.body_mut().read_to_vec().expect("a body");
    assert_eq!(bytes, [0xFF, 0xD8, 0xFF, 0xE0]);

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A photo"}),
    );
    let conversation = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{conversation}/say"),
        &serde_json::json!({"say": "", "attachments": [{"id": id}]}),
    );
    assert_eq!(common::status(&said), 202, "a photo alone is a turn");
}

/// A PDF naming an SSN is held for the owner, then kept in his Drive on his say: the first
/// upload keeps nothing and says why; the second, marked private, goes to the vault and leaves
/// only a note on the box, which a turn names and the file route will not serve.
#[test]
fn a_pdf_naming_an_ssn_is_held_then_kept_privately_in_the_drive() {
    use base64::Engine as _;
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Fakes::new("private").whois(whois).model(script());
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, vault, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    let w2 = common::pdf_saying("SSN 123-45-6789 wages 70000");
    let encoded = base64::engine::general_purpose::STANDARD.encode(&w2);

    let mut held = common::act(
        &format!("{base}/api/secretary/attachments"),
        &serde_json::json!({ "file": encoded, "name": "w2.pdf" }),
    );
    assert_eq!(common::status(&held), 200);
    let held = common::body_json(&mut held);
    assert_eq!(held["kind"], "sensitive");
    assert!(
        held["why"]
            .as_str()
            .is_some_and(|why| why.contains("identity number")),
        "{held}"
    );
    assert_eq!(held["folder"], "/my-files/Suzatary/Private");
    assert!(
        vault.0.lock().expect("unpoisoned").is_empty(),
        "nothing kept yet"
    );

    let mut kept = common::act(
        &format!("{base}/api/secretary/attachments"),
        &serde_json::json!({ "file": encoded, "name": "w2.pdf", "private": true }),
    );
    assert_eq!(common::status(&kept), 200);
    let kept = common::body_json(&mut kept);
    assert_eq!(kept["kind"], "kept");
    let file = &kept["file"];
    let id = file["id"].as_str().expect("an id").to_owned();
    let drive_name = format!("{} w2.pdf", &id[..8]);
    assert_eq!(
        file["private"],
        format!("/my-files/Suzatary/Private/{drive_name}")
    );
    assert_eq!(
        vault.0.lock().expect("unpoisoned").as_slice(),
        [(drive_name, w2)]
    );

    let served = common::get(&format!("{base}/api/secretary/attachments/{id}"));
    assert_eq!(common::status(&served), 404, "a private file is not served");

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "Taxes"}),
    );
    let conversation = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let mut said = common::act(
        &format!("{base}/api/secretary/conversations/{conversation}/say"),
        &serde_json::json!({"say": "send this to the accountant", "attachments": [{"id": id, "name": "renamed.pdf"}]}),
    );
    assert_eq!(common::status(&said), 202);
    let said = common::body_json(&mut said);
    let turns = said["conversation"]["turns"].as_array().expect("turns");
    let on_turn = &turns.last().expect("the turn")["attachments"][0];
    assert_eq!(on_turn["name"], "w2.pdf", "named when it was kept");
    assert_eq!(on_turn["private"], file["private"]);
}

/// Upload one file, answering its id: kept on the box, or in the owner's Drive when `private`.
fn uploaded(base: &str, bytes: &[u8], name: &str, private: bool) -> serde_json::Value {
    use base64::Engine as _;
    let mut answered = common::act(
        &format!("{base}/api/secretary/attachments"),
        &serde_json::json!({
            "file": base64::engine::general_purpose::STANDARD.encode(bytes),
            "name": name,
            "private": private,
        }),
    );
    assert_eq!(common::status(&answered), 200);
    common::body_json(&mut answered)
}

/// Say `say` in conversation `id` with these uploads, and wait for the round to settle.
fn sending(base: &str, id: &str, say: &str, files: &[(&str, &str)]) -> serde_json::Value {
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({
            "say": say,
            "attachments": files
                .iter()
                .map(|(id, name)| serde_json::json!({"id": id, "name": name}))
                .collect::<Vec<_>>(),
        }),
    );
    assert_eq!(common::status(&said), 202);
    settled(base, id)
}

/// A new conversation's id.
fn opened(base: &str, title: &str) -> String {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({ "title": title }),
    );
    common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned()
}

/// Every answer `verb` gave in this conversation, as JSON, in the order given.
fn verb_answers(payload: &serde_json::Value, verb: &str) -> Vec<serde_json::Value> {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["who"] == "verb" && turn["verb"] == verb)
        .map(|turn| {
            let text = turn["text"].as_str().expect("an answer");
            serde_json::from_str(text).unwrap_or_else(|_| serde_json::Value::from(text))
        })
        .collect()
}

/// A PDF sent in one conversation is found from another by `search`, with its file id, its
/// conversation and its turn, beside the mail; `upload_read` hands it over as itself. A PDF
/// naming an SSN is found by nothing while the box holds it back, and by its redacted text once
/// the owner keeps it private and sends it; the number itself is never found.
#[test]
fn a_file_sent_in_a_chat_is_found_by_search_and_a_held_one_only_once_released() {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let model = FakeModel::saying([
        r#"{"say": "Kept the lease."}"#,
        r#"{"say": "Looking.", "calls": [
            {"verb": "search", "args": {"query": "SSN"}},
            {"verb": "search", "args": {"query": "lease unit"}},
            {"verb": "search", "args": {"query": "from:landlord@example.test lease"}}
        ]}"#,
        r#"{"say": "The lease is in A lease."}"#,
        r#"{"say": "Kept the form."}"#,
        r#"{"say": "Looking again.", "calls": [
            {"verb": "search", "args": {"query": "SSN"}},
            {"verb": "search", "args": {"query": "123-45-6789"}}
        ]}"#,
        r#"{"say": "The form is in A lease."}"#,
    ]);
    let fakes = Fakes::new("upload-search").whois(whois).model(model);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();

    let lease = uploaded(
        &base,
        &common::pdf_saying("Lease for unit 4"),
        "lease.pdf",
        false,
    );
    let lease = lease["file"]["id"].as_str().expect("an id").to_owned();
    let w2 = common::pdf_saying("SSN 123-45-6789");
    assert_eq!(uploaded(&base, &w2, "w2.pdf", false)["kind"], "sensitive");

    let sent_in = opened(&base, "A lease");
    sending(
        &base,
        &sent_in,
        "here is the lease",
        &[(&lease, "lease.pdf")],
    );
    let asking = opened(&base, "Where is it");
    let first = verb_answers(&sending(&base, &asking, "find my papers", &[]), "search");
    assert_eq!(first.len(), 3, "{first:?}");
    assert_eq!(
        first[0]["files"]["found"],
        serde_json::json!([]),
        "a held-back file is found by nothing"
    );
    let found = &first[1]["files"]["found"];
    assert_eq!(found.as_array().map(Vec::len), Some(1), "{found}");
    assert_eq!(found[0]["file"], lease);
    assert_eq!(found[0]["name"], "lease.pdf");
    assert_eq!(found[0]["conversation"], sent_in);
    assert_eq!(found[0]["turn"], 0);
    assert_eq!(found[0]["said"], "here is the lease");
    assert!(
        found[0]["excerpt"]
            .as_str()
            .is_some_and(|excerpt| excerpt.contains("unit 4")),
        "{found}"
    );
    assert!(
        first[1]["mail"].is_array(),
        "the mail is answered beside it"
    );
    assert!(
        first[2]["files"]["not_searched"]
            .as_str()
            .is_some_and(|why| why.contains("from:")),
        "a mail field is not a question for a file: {}",
        first[2]
    );

    let w2 = uploaded(&base, &w2, "w2.pdf", true);
    let w2 = w2["file"]["id"].as_str().expect("an id").to_owned();
    sending(&base, &sent_in, "and my tax form", &[(&w2, "w2.pdf")]);
    let again = verb_answers(&sending(&base, &asking, "and now?", &[]), "search");
    assert_eq!(
        again.len(),
        5,
        "the first round's three, then this round's two"
    );
    let found = &again[3]["files"]["found"];
    assert_eq!(found.as_array().map(Vec::len), Some(1), "{found}");
    assert_eq!(found[0]["file"], w2);
    assert_eq!(found[0]["conversation"], sent_in);
    assert!(
        found[0]["private"]
            .as_str()
            .is_some_and(|path| path.starts_with("/my-files/Suzatary/Private/")),
        "{found}"
    );
    assert_eq!(
        again[4]["files"]["found"],
        serde_json::json!([]),
        "the number itself is never indexed"
    );
}

/// Erasing the turn that sent a file takes its words out of the index at once, and a search
/// finds it no more.
#[test]
fn erasing_the_turn_that_sent_a_file_takes_it_out_of_the_index() {
    let model = FakeModel::saying([
        r#"{"say": "Kept the lease."}"#,
        r#"{"say": "Looking.", "calls": [
            {"verb": "search", "args": {"query": "lease unit"}}
        ]}"#,
        r#"{"say": "The lease is in A lease."}"#,
        r#"{"say": "Looking again.", "calls": [
            {"verb": "search", "args": {"query": "lease unit"}}
        ]}"#,
        r#"{"say": "The lease is gone."}"#,
    ]);
    let fakes = Arc::new(
        Fakes::new("upload-erase")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .model(model),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, .. } =
        common::Server::new(Arc::clone(&fakes), deployment, Vec::new()).start();
    let lease = uploaded(
        &base,
        &common::pdf_saying("Lease for unit 4"),
        "lease.pdf",
        false,
    );
    let lease = lease["file"]["id"].as_str().expect("an id").to_owned();
    let sent_in = opened(&base, "A lease");
    sending(
        &base,
        &sent_in,
        "here is the lease",
        &[(&lease, "lease.pdf")],
    );
    let asking = opened(&base, "Where is it");
    let first = verb_answers(&sending(&base, &asking, "the lease?", &[]), "search");
    assert_eq!(first[0]["files"]["found"][0]["file"], lease);
    let entry = fakes
        .root
        .join("secretary/upload-index")
        .join(format!("{lease}.json"));
    assert!(entry.exists(), "the lease's words are in the index");
    let erased = common::act(
        &format!("{base}/api/secretary/conversations/{sent_in}/erase"),
        &serde_json::json!({"turns": [0]}),
    );
    assert_eq!(common::status(&erased), 200);
    assert!(
        !entry.exists(),
        "erasing its turn takes its words out of the index"
    );
    let last = verb_answers(&sending(&base, &asking, "and now?", &[]), "search");
    assert_eq!(
        last.last().expect("this round's search")["files"]["found"],
        serde_json::json!([]),
        "an erased upload is found by nothing"
    );
}

/// A search answer is stored at most 64 KiB like every verb's answer, and the files lead it, so
/// an answer cut for a flood of mail rows still stores the file it found.
#[test]
fn a_search_answer_is_stored_within_the_cap_and_keeps_its_files() {
    let corpus = (0..2000).fold(common::FakeCorpus::new(), |corpus, n| {
        corpus.with(common::FakeMessage {
            hash: format!("{n:064x}"),
            thread: format!("thread-{n}"),
            sender: "clerk@example.test".to_owned(),
            subject: format!("Lease renewal notice {n}"),
            body: String::new(),
            timestamp: "2026-09-25 09:00".to_owned(),
        })
    });
    let model = FakeModel::saying([
        r#"{"say": "Kept."}"#,
        r#"{"say": "Looking.", "calls": [{"verb": "search", "args": {"query": "lease"}}]}"#,
        r#"{"say": "Found it."}"#,
    ]);
    let fakes = Fakes::new("upload-search-cap")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .corpus(corpus)
        .model(model);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    let lease = uploaded(
        &base,
        &common::pdf_saying("Lease for unit 4"),
        "lease.pdf",
        false,
    );
    let lease = lease["file"]["id"].as_str().expect("an id").to_owned();
    let id = opened(&base, "A lease");
    sending(&base, &id, "here is the lease", &[(&lease, "lease.pdf")]);
    let payload = sending(&base, &id, "find the lease", &[]);
    let stored = payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .find(|turn| turn["who"] == "verb" && turn["verb"] == "search")
        .and_then(|turn| turn["text"].as_str())
        .expect("the search's answer")
        .to_owned();
    assert!(
        stored.len() <= suzatary_core::secretary::line_words::STORED_ANSWER_BYTES,
        "{} bytes stored",
        stored.len()
    );
    assert!(
        stored.ends_with("of this answer were not stored]"),
        "the cut is said"
    );
    assert!(
        stored.contains(&lease),
        "the file leads, so the cut keeps it"
    );
}

/// `upload_read` hands a file found by `search` to the model as itself, and refuses a file no
/// turn sent.
#[test]
fn upload_read_opens_a_file_a_turn_sent_and_no_other() {
    let lease_pdf = common::pdf_saying("Lease for unit 4");
    let never = suzatary_core::attachment::AttachmentId::of(b"never sent");
    let model = FakeModel::saying_owned([
        r#"{"say": "Kept."}"#.to_owned(),
        format!(
            r#"{{"say": "Opening.", "calls": [
                {{"verb": "upload_read", "args": {{"file": "{}"}}}},
                {{"verb": "upload_read", "args": {{"file": "{never}"}}}}
            ]}}"#,
            suzatary_core::attachment::AttachmentId::of(&lease_pdf)
        ),
        r#"{"say": "Read it."}"#.to_owned(),
    ]);
    let fakes = Fakes::new("upload-read")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .model(model);
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, .. } =
        common::Server::new(Arc::new(fakes), deployment, Vec::new()).start();
    let lease = uploaded(&base, &lease_pdf, "lease.pdf", false);
    let lease = lease["file"]["id"].as_str().expect("an id").to_owned();
    let id = opened(&base, "A lease");
    sending(&base, &id, "here is the lease", &[(&lease, "lease.pdf")]);
    let payload = sending(&base, &id, "read it back", &[]);
    let turns: Vec<&serde_json::Value> = payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["who"] == "verb" && turn["verb"] == "upload_read")
        .collect();
    assert_eq!(turns.len(), 2, "{payload}");
    assert_eq!(turns[0]["outcome"]["is"], "answered", "{}", turns[0]);
    let read: serde_json::Value =
        serde_json::from_str(turns[0]["text"].as_str().expect("an answer")).expect("JSON");
    assert_eq!(read["file"], lease);
    assert_eq!(read["media"], "application/pdf");
    assert_eq!(turns[1]["outcome"]["is"], "refused", "{}", turns[1]);
    assert!(
        turns[1]["text"]
            .as_str()
            .is_some_and(|why| why.contains("never sent")),
        "{}",
        turns[1]
    );
}

/// Bytes that are not an image, an unstored photo, and a malformed id are refused by name.
#[test]
fn what_is_not_a_stored_photo_is_refused() {
    let (base, _) = writing("notaphoto");
    // "<svg/>" in base64.
    let svg = common::act(
        &format!("{base}/api/secretary/attachments"),
        &serde_json::json!({"file": "PHN2Zy8+"}),
    );
    assert_eq!(common::status(&svg), 400);

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "No photo"}),
    );
    let conversation = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let unknown = common::act(
        &format!("{base}/api/secretary/conversations/{conversation}/say"),
        &serde_json::json!({"say": "look", "attachments": [{"id": "a".repeat(64)}]}),
    );
    assert_eq!(common::status(&unknown), 400, "a photo nobody stored");

    let path = common::get(&format!("{base}/api/secretary/attachments/..%2F..%2Fetc"));
    assert_eq!(
        common::status(&path),
        400,
        "an id is 64 hex digits and nothing else"
    );
}
