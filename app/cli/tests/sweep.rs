//! The open-list sweep end to end over `secretary::sweep::sweep`: a real chain, handler,
//! conversation file and candidate search, with only the model scripted.
//!
//! A sweep is started by the box, so no server or gate is involved, except in the last test,
//! where performing its proposal must cross a real `Tap`.

mod common;

use std::sync::Arc;

use common::{FakeCorpus, FakeMessage, FakeModel, Fakes};
use prodrome_core::event::mk_completed;
use prodrome_core::policy::Untrusted;
use suzatary_core::authoring::{NewTodo, build_todo};
use suzatary_core::capability::Datetime;
use suzatary_core::record::{EventStore, TodoEvent, mk_source};
use suzatary_core::secretary::Bound;
use suzatary_core::secretary::changes::Act;
use suzatary_core::secretary::store::Proposal;
use suzatary_core::secretary::sweep::{dry_run, sweep};
use suzatary_core::store::Reads as _;

/// The instant every todo below is authored, well before [`common::now`], so a candidate between
/// the two is unambiguously later.
fn created() -> Datetime {
    Datetime::new(2026, 8, 20, 12, 0, 0, 0).expect("a real instant")
}

/// The fixture deployment; a sweep reaches no gate.
fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment(
        [common::login("ada@example.com")],
        [(common::login("ada@example.com"), common::actor("ada"))],
    )
}

fn store(root: &std::path::Path) -> EventStore {
    EventStore::new(
        root.join("events"),
        Untrusted::of([common::actor("triage")]),
    )
}

/// An open todo sourced from a thread.
fn a_sourced_todo(events: &EventStore, id: &str, thread: &str, sender: &str) {
    let event = build_todo(
        &NewTodo {
            todo: id.to_owned(),
            fields: suzatary_core::todo::TodoFields {
                body: "Check in for the airline flight".to_owned(),
                rationale: "A real consequence within weeks.".to_owned(),
                kind: "todo".to_owned(),
                category: "home".to_owned(),
                price: suzatary_core::todo::Price::Flat { percent: 45 },
                detail: String::new(),
                waiting_on: String::new(),
            },
            source: Some(mk_source(sender, "Your trip", created(), "", thread, "")),
            subtodos: Vec::new(),
        },
        suzatary_core::zone::Moment {
            at: created(),
            zone: suzatary_core::zone::Zone::UTC,
        },
        "ada",
        &["work", "home", "other", "study"],
        &suzatary_io::typeset::InProcess,
    )
    .expect("a valid todo");
    suzatary_core::store::append_to(events, suzatary_core::record::GENESIS, event)
        .expect("appends");
}

/// An open todo with no source at all.
fn a_plain_todo(events: &EventStore, id: &str, body: &str) {
    let event = build_todo(
        &NewTodo {
            todo: id.to_owned(),
            fields: suzatary_core::todo::TodoFields {
                body: body.to_owned(),
                rationale: "An errand with a recoverable cost.".to_owned(),
                kind: "todo".to_owned(),
                category: "home".to_owned(),
                price: suzatary_core::todo::Price::Flat { percent: 75 },
                detail: String::new(),
                waiting_on: String::new(),
            },
            source: None,
            subtodos: Vec::new(),
        },
        suzatary_core::zone::Moment {
            at: created(),
            zone: suzatary_core::zone::Zone::UTC,
        },
        "ada",
        &["work", "home", "other", "study"],
        &suzatary_io::typeset::InProcess,
    )
    .expect("a valid todo");
    suzatary_core::store::append_to(events, suzatary_core::record::GENESIS, event)
        .expect("appends");
}

const WHY: &str = "The airline confirmation says he is checked in.";

fn propose_completion_script(todo: &str, hash: &str) -> FakeModel {
    FakeModel::saying_owned([
        format!(
            r#"{{"say": "Proposing.", "calls": [
                 {{"verb": "propose_changes", "args": {{
                    "acts": [{{"act": "complete", "todo": "{todo}", "note": "checked in"}}],
                    "why": "{WHY}",
                    "sources": "mail:{hash}"
                 }}}}
               ]}}"#
        ),
        r#"{"say": "Done."}"#.to_owned(),
    ])
}

/// An open todo whose thread later carries settling mail gets one `Complete` proposal carrying
/// its `mail:<hash>`.
#[test]
fn an_open_todo_with_later_mail_gets_one_completed_proposal() {
    let root = common::scratch("sweep-completes");
    let events = store(&root);
    a_sourced_todo(&events, "flightcheckin", "th1", "noreply@airline.example");
    let caps = Fakes::with_store("sweep-completes", events)
        .corpus(FakeCorpus::new().with(FakeMessage {
            hash: "aaaaaaaaaaaa".to_owned(),
            thread: "th1".to_owned(),
            sender: "noreply@airline.example".to_owned(),
            subject: "You're checked in".to_owned(),
            body: "You are checked in for flight UA523.".to_owned(),
            timestamp: "2026-08-25 09:00:00".to_owned(),
        }))
        .model(propose_completion_script("flightcheckin", "aaaaaaaaaaaa"));
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let swept = sweep(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the sweep runs");

    assert_eq!(
        swept.rounds, 2,
        "one round to propose, one to say it is done"
    );
    assert_eq!(swept.proposed.len(), 1, "{:?}", swept.stored.proposals);
    let (_, standing) = &swept.proposed[0];
    match &standing.object.asked.proposal {
        Proposal::Changes(changeset) => {
            assert_eq!(changeset.chain(), "life");
            assert_eq!(changeset.acts().len(), 1);
            assert!(
                matches!(&changeset.acts()[0], Act::Complete { todo, .. } if todo == "flightcheckin"),
                "{:?}",
                changeset.acts()
            );
        }
        other => panic!("expected a changeset, got {other:?}"),
    }
    assert_eq!(standing.object.asked.reasons.why(), WHY);
    assert!(
        standing
            .object
            .asked
            .reasons
            .sources()
            .iter()
            .any(|reference| reference.to_string() == "mail:aaaaaaaaaaaa"),
        "{:?}",
        standing.object.asked.reasons.sources()
    );
    let chain_events: Vec<_> = caps.chain.objects().expect("the chain reads");
    assert!(
        !chain_events.iter().any(|(_, envelope)| matches!(
            envelope.event(),
            Some(suzatary_core::record::TodoEvent::Completed(_))
        )),
        "the sweep proposes; it never appends"
    );
}

/// A sweep whose model cannot answer is the verb's failure, so the triage wrapper's exit status
/// reads it: no stamp is written, and the pass ends failed at `sweep`.
#[test]
fn a_sweep_the_model_cannot_answer_fails_the_verb() {
    let root = common::scratch("sweep-refused");
    let events = store(&root);
    a_sourced_todo(&events, "flightcheckin", "th1", "noreply@airline.example");
    let caps = Fakes::with_store("sweep-refused", events)
        .corpus(FakeCorpus::new().with(FakeMessage {
            hash: "aaaaaaaaaaaa".to_owned(),
            thread: "th1".to_owned(),
            sender: "noreply@airline.example".to_owned(),
            subject: "You're checked in".to_owned(),
            body: "You are checked in for flight UA523.".to_owned(),
            timestamp: "2026-08-25 09:00:00".to_owned(),
        }))
        .model(FakeModel::saying([]));
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let outcome = sweep(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    );
    assert!(outcome.is_err(), "a model that cannot answer is a refusal");
    let failed = suzatary_cli::commands::secretary::swept_or_failed(outcome, true)
        .expect_err("the verb fails, so its exit status is non-zero");
    assert!(failed.to_string().contains("did not run"), "{failed}");
}

/// A todo with no later mail never reaches the model: no conversation, no proposal.
#[test]
fn an_open_todo_with_no_later_mail_gets_no_proposal() {
    let root = common::scratch("sweep-quiet");
    let events = store(&root);
    a_plain_todo(&events, "quiettodo", "Water the office plants");
    let caps = Fakes::with_store("sweep-quiet", events);
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let swept = sweep(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the sweep runs");

    assert_eq!(swept.rounds, 0, "absence of evidence asks no model");
    assert!(swept.proposed.is_empty());
    assert!(
        suzatary_core::secretary::transcript::keys(caps.conversations.conversations())
            .expect("the store reads")
            .is_empty(),
        "no conversation was opened for nothing"
    );
}

/// A scripted `complete` of a todo the round reads as already resolved is refused, and the chain
/// does not move.
#[test]
fn completing_an_already_resolved_todo_is_refused_and_proposes_nothing() {
    let root = common::scratch("sweep-resolved");
    let events = store(&root);
    // The one that keeps the round alive: open, with later mail.
    a_sourced_todo(&events, "flightcheckin", "th1", "noreply@airline.example");
    // The one the script will wrongly try to complete.
    a_plain_todo(&events, "alreadydone", "Renew the parking permit");
    suzatary_core::store::append_to(
        &events,
        suzatary_core::record::GENESIS,
        mk_completed("alreadydone", common::now(), "ada", "done already").expect("valid"),
    )
    .expect("appends");
    let caps = Fakes::with_store("sweep-resolved", events)
        .corpus(FakeCorpus::new().with(FakeMessage {
            hash: "bbbbbbbbbbbb".to_owned(),
            thread: "th1".to_owned(),
            sender: "noreply@airline.example".to_owned(),
            subject: "You're checked in".to_owned(),
            body: "You are checked in for flight UA523.".to_owned(),
            timestamp: "2026-08-25 09:00:00".to_owned(),
        }))
        .model(propose_completion_script("alreadydone", "bbbbbbbbbbbb"));
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let swept = sweep(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the sweep runs");

    assert!(
        swept.proposed.is_empty(),
        "a complete on a resolved todo must not become a standing proposal: {:?}",
        swept.stored.proposals
    );
    // The refusal is on the transcript as a refused tool turn, not a silent drop.
    let refused = swept.stored.turns.iter().any(|said| {
        matches!(
            &said.turn,
            suzatary_core::capability::Turn::Tool { verb, outcome, .. }
                if verb.as_str() == "propose_changes" && outcome.refused()
        )
    });
    assert!(refused, "{:?}", swept.stored.turns);
}

/// A scripted `todo_add` is refused by the round rather than dispatched, and no todo is added.
#[test]
fn a_todo_add_call_is_refused_by_the_verb_set() {
    let root = common::scratch("sweep-verbset");
    let events = store(&root);
    a_sourced_todo(&events, "flightcheckin", "th1", "noreply@airline.example");
    let caps = Fakes::with_store("sweep-verbset", events)
        .corpus(FakeCorpus::new().with(FakeMessage {
            hash: "cccccccccccc".to_owned(),
            thread: "th1".to_owned(),
            sender: "noreply@airline.example".to_owned(),
            subject: "You're checked in".to_owned(),
            body: "You are checked in for flight UA523.".to_owned(),
            timestamp: "2026-08-25 09:00:00".to_owned(),
        }))
        .model(FakeModel::saying_owned([
            r#"{"say": "Adding one instead.", "calls": [
                 {"verb": "todo_add", "args": {
                    "todo_id": "sneakedin", "body": "Should not exist",
                    "rationale": "an attempt"}}
               ]}"#
            .to_owned(),
            r#"{"say": "Done."}"#.to_owned(),
        ]));
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let swept = sweep(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the sweep runs");

    assert!(swept.proposed.is_empty());
    let refusal = swept.stored.turns.iter().find_map(|said| match &said.turn {
        suzatary_core::capability::Turn::Tool {
            verb,
            answer,
            outcome,
            files: _,
        } if verb.as_str() == "todo_add" => Some((answer.clone(), outcome.refused())),
        _ => None,
    });
    let (answer, refused) = refusal.expect("the todo_add call is on the transcript");
    assert!(refused, "{answer}");
    assert!(answer.contains("the sweep may not call"), "{answer}");

    let chain_events = caps.chain.objects().expect("the chain reads");
    let authored_ids: std::collections::BTreeSet<String> = chain_events
        .iter()
        .filter_map(|(_, envelope)| match envelope.event() {
            Some(suzatary_core::record::TodoEvent::Authored(record)) => {
                Some(record.todo.as_str().to_owned())
            }
            _ => None,
        })
        .collect();
    assert!(!authored_ids.contains("sneakedin"), "{authored_ids:?}");
    assert!(authored_ids.contains("flightcheckin"), "{authored_ids:?}");
}

/// A `complete` on an open todo the round never showed the model is refused, though the chain
/// calls it open.
#[test]
fn completing_an_open_todo_with_no_candidates_is_refused_and_proposes_nothing() {
    let root = common::scratch("sweep-unswept");
    let events = store(&root);
    a_sourced_todo(&events, "flightcheckin", "th1", "noreply@airline.example");
    a_plain_todo(&events, "quiettodo", "Water the office plants");
    let caps = Fakes::with_store("sweep-unswept", events)
        .corpus(FakeCorpus::new().with(FakeMessage {
            hash: "dddddddddddd".to_owned(),
            thread: "th1".to_owned(),
            sender: "noreply@airline.example".to_owned(),
            subject: "You're checked in".to_owned(),
            body: "You are checked in for flight UA523.".to_owned(),
            timestamp: "2026-08-25 09:00:00".to_owned(),
        }))
        .model(propose_completion_script("quiettodo", "dddddddddddd"));
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let swept = sweep(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the sweep runs");

    assert!(
        swept.proposed.is_empty(),
        "a complete on a todo this round never listed must not become a standing proposal: \
         {:?}",
        swept.stored.proposals
    );
    let refusal = swept.stored.turns.iter().find_map(|said| match &said.turn {
        suzatary_core::capability::Turn::Tool {
            verb,
            answer,
            outcome,
            files: _,
        } if verb.as_str() == "propose_changes" => Some((answer.clone(), outcome.refused())),
        _ => None,
    });
    let (answer, refused) = refusal.expect("the propose_changes call is on the transcript");
    assert!(refused, "{answer}");
    assert!(answer.contains("quiettodo"), "{answer}");

    let chain_events = caps.chain.objects().expect("the chain reads");
    assert!(
        !chain_events.iter().any(|(_, envelope)| matches!(
            envelope.event(),
            Some(suzatary_core::record::TodoEvent::Completed(_))
        )),
        "the sweep proposes; it never appends"
    );
}

/// Sweep, then a tap through `POST …/proposals/<n>/tap` on a real server: the one settled todo
/// completes and the other is untouched.
#[test]
fn a_swept_completion_is_applied_through_the_real_tap_route() {
    let root = common::scratch("sweep-tap");
    let events = store(&root);
    a_sourced_todo(&events, "flightcheckin", "th1", "noreply@airline.example");
    // `th2` has no later mail, so the model never sees this one.
    a_sourced_todo(&events, "trainbooking", "th2", "tickets@rail.example");
    let caps = Fakes::with_store("sweep-tap", events)
        .whois(common::FakeWhois::new().naming(common::v4(), common::login("ada@example.com")))
        .corpus(FakeCorpus::new().with(FakeMessage {
            hash: "eeeeeeeeeeee".to_owned(),
            thread: "th1".to_owned(),
            sender: "noreply@airline.example".to_owned(),
            subject: "You're checked in".to_owned(),
            body: "You are checked in for flight UA523.".to_owned(),
            timestamp: "2026-08-25 09:00:00".to_owned(),
        }))
        .model(propose_completion_script("flightcheckin", "eeeeeeeeeeee"));
    let policy = common::chain();
    let dep = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let swept = sweep(
        &caps.secretary(),
        &dep,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the sweep runs");
    assert_eq!(swept.proposed.len(), 1, "{:?}", swept.stored.proposals);
    let (index, standing) = &swept.proposed[0];
    match &standing.object.asked.proposal {
        Proposal::Changes(changeset) => assert!(
            matches!(
                &changeset.acts()[0],
                Act::Complete { todo, .. } if todo == "flightcheckin"
            ),
            "{:?}",
            changeset.acts()
        ),
        other => panic!("expected a changeset, got {other:?}"),
    }

    let id = today_of(prodrome_core::fpl::instant_of(common::now()).date());

    let base = common::start(Arc::new(caps), dep, Vec::new());

    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/{index}/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(
        common::status(&tapped),
        200,
        "{:?}",
        common::body_json(&mut tapped)
    );
    let after = common::body_json(&mut tapped);
    let row = &after["proposals"][index];
    assert_eq!(row["state"], "applied", "{after}");
    assert!(
        row["sources"]
            .as_array()
            .expect("sources")
            .iter()
            .any(|reference| reference == "mail:eeeeeeeeeeee"),
        "{after}"
    );

    // A fresh store over the same directory, so this reads the object store rather than the
    // server's handle.
    let chain_reader = suzatary_io::objects::Objects::chain(store(&root));
    let objects = chain_reader.objects().expect("the chain reads");
    let completed_flightcheckin: Vec<_> = objects
        .iter()
        .filter_map(|(_, envelope)| match envelope.event() {
            Some(TodoEvent::Completed(lifecycle)) if lifecycle.todo.as_str() == "flightcheckin" => {
                Some(lifecycle)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        completed_flightcheckin.len(),
        1,
        "{completed_flightcheckin:?}"
    );
    assert_eq!(
        completed_flightcheckin[0].actor.as_str(),
        "assistant",
        "{completed_flightcheckin:?}"
    );

    assert!(
        !objects.iter().any(|(_, envelope)| matches!(
            envelope.event(),
            Some(TodoEvent::Completed(lifecycle))
                if lifecycle.todo.as_str() == "trainbooking"
        )),
        "{objects:?}"
    );
}

/// `--dry-run` gathers and bounds the open list without opening a conversation or needing a
/// model.
#[test]
fn dry_run_over_the_fixture_opens_no_conversation_and_reports_counts() {
    let root = common::scratch("sweep-dryrun");
    let events = store(&root);
    a_sourced_todo(&events, "flightcheckin", "th1", "noreply@airline.example");
    let caps =
        Fakes::with_store("sweep-dryrun", events).corpus(FakeCorpus::new().with(FakeMessage {
            hash: "eeeeeeeeeeee".to_owned(),
            thread: "th1".to_owned(),
            sender: "noreply@airline.example".to_owned(),
            subject: "You're checked in".to_owned(),
            body: "You are checked in for flight UA523.".to_owned(),
            timestamp: "2026-08-25 09:00:00".to_owned(),
        }));
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let report = dry_run(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
    )
    .expect("the dry run runs");

    // The one message matches both the thread and the sender query, so candidates reports it
    // twice.
    assert_eq!(report.considered, 1);
    assert_eq!(report.in_round, 1);
    assert_eq!(report.candidates, 2);
    assert_eq!(report.todos.len(), 1);
    assert_eq!(report.todos[0].0.as_str(), "flightcheckin");
    assert_eq!(report.todos[0].1, 2);
    assert!(report.turn_chars > 0, "a real user turn was built to count");

    assert!(
        suzatary_core::secretary::transcript::keys(caps.conversations.conversations())
            .expect("the store reads")
            .is_empty(),
        "a dry run must not open a conversation"
    );
}

/// The sweep's round, pinned: its lines and what the model is shown and sent, for a script that
/// proposes, calls a verb outside the sweep's set, then finishes.
#[test]
fn the_sweeps_round_is_pinned() {
    let root = common::scratch("sweep-pinned");
    let events = store(&root);
    a_sourced_todo(&events, "flightcheckin", "th1", "noreply@airline.example");
    let caps = Fakes::with_store("sweep-pinned", events)
        .corpus(FakeCorpus::new().with(FakeMessage {
            hash: "dddddddddddd".to_owned(),
            thread: "th1".to_owned(),
            sender: "noreply@airline.example".to_owned(),
            subject: "You're checked in".to_owned(),
            body: "You are checked in for flight UA523.".to_owned(),
            timestamp: "2026-08-25 09:00:00".to_owned(),
        }))
        .model(FakeModel::saying_owned([
            format!(
                r#"{{"say": "Proposing.", "calls": [
                     {{"verb": "propose_changes", "args": {{
                        "acts": [{{"act": "complete", "todo": "flightcheckin", "note": "in"}}],
                        "why": "{WHY}", "sources": "mail:dddddddddddd"}}}},
                     {{"verb": "todo_add", "args": {{"todo_id": "x", "body": "no"}}}}
                   ]}}"#
            ),
            r#"{"say": "Done."}"#.to_owned(),
        ]));
    let policy = common::chain();
    let deployment = deployment();
    let chains = [Bound {
        name: "life",
        caps: caps.secretary(),
        policy: &policy,
    }];

    let swept = sweep(
        &caps.secretary(),
        &deployment,
        &suzatary_core::settings::Settables::default(),
        &chains,
        "a brief",
    )
    .expect("the sweep runs");

    assert_eq!(swept.rounds, 2);
    common::pinned("sweep", &common::scene(&caps, &swept.stored.id));
}

/// Day `day`'s sweep conversation.
fn today_of(day: chrono::NaiveDate) -> suzatary_core::secretary::store::ConversationId {
    suzatary_core::secretary::store::ConversationId::daily(
        suzatary_core::secretary::daily::Daily::Sweep,
        day,
    )
}
