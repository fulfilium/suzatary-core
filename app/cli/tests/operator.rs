//! The box's operator, over a real server whose daemon names loopback as the box's own node: a
//! question asked at the terminal is the owner's words stamped as the operator's, and a card that
//! sends mail is refused to the operator's tap with nothing sent.

use crate::common;

use std::sync::Arc;

use common::{FakeModel, Fakes};
use suzatary_cli::commands::{ask, tap};
use suzatary_connect::cast::CAST;
use suzatary_inbox::secretary::stamp::{Speaker, Stamp};
use suzatary_signatures::signatures::Seat;

fn owner() -> suzatary_base::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_connect::deployment::Deployment {
    common::deployment([owner()], [(owner(), common::actor(CAST.owner.actor))])
}

fn running(tag: &str, model: FakeModel) -> (Arc<Fakes>, common::Running) {
    let fakes = Arc::new(Fakes::new(tag).model(model));
    let running = common::Server::new(
        Arc::clone(&fakes),
        deployment(),
        common::loopback_is_this_box(),
    )
    .start();
    (fakes, running)
}

/// The question is stamped as asked at the box's terminal and signed by the root key, so both
/// tell it from a device's.
#[test]
fn an_operators_question_is_the_owners_words_stamped_as_the_operators() {
    let (fakes, running) = running(
        "operator-ask",
        FakeModel::saying([r#"{"say": "Nothing is due this week."}"#]),
    );
    let asked = ask::ask(
        &common::operator(&running.base),
        &common::topic(fakes.conversations.conversations()),
        "what is due this week",
    )
    .expect("the operator asks");
    assert!(asked.answer.contains("Nothing is due"), "{}", asked.answer);
    let stored = suzatary_lines::secretary::conversations::read(
        fakes.conversations.conversations(),
        &asked.conversation,
    )
    .expect("reads");
    let said = stored
        .turns
        .iter()
        .find(|said| {
            matches!(
                said.turn,
                suzatary_conversations::secretary::model::Turn::Person { .. }
            )
        })
        .expect("the question's line");
    assert_eq!(
        said.by,
        Some(Stamp::Actor(Speaker::at(CAST.owner.actor, Seat::Operator)))
    );
    assert!(Stamp::owners(said.by.as_ref()), "the owner's words");
    assert_eq!(
        suzatary_conversations::secretary::taint::held(&stored),
        suzatary_conversations::secretary::taint::Taint::Clean
    );
    let signers: Vec<String> =
        suzatary_store::store::Reads::objects(fakes.conversations.conversations().lines)
            .expect("the lines read")
            .into_iter()
            .filter_map(|(_, object)| match object {
                prodrome_data::event::Envelope::Signed(signed) if signed.object == asked.line => {
                    Some(signed.key.as_str().to_owned())
                }
                _ => None,
            })
            .collect();
    assert_eq!(
        signers,
        [common::cast_keys().root().public().as_str().to_owned()],
        "the root key signs the question's line"
    );
}

/// `ask` returns its own round's words, never the earlier round's reply, which lands after the
/// question and so may be read after it.
#[test]
fn a_question_said_while_a_round_answers_gets_its_own_reply() {
    let gate = common::Gate::new();
    let (fakes, running) = running(
        "operator-ask-window",
        FakeModel::saying([
            r#"{"say": "The permit is due Friday."}"#,
            r#"{"say": "The garden invoice is paid."}"#,
        ])
        .gated(gate.clone()),
    );
    let id = common::topic(fakes.conversations.conversations());
    let first = {
        let base = running.base.clone();
        let id = id.clone();
        std::thread::spawn(move || {
            ask::ask(&common::operator(&base), &id, "when is the permit due")
        })
    };
    gate.wait_until_entered();
    // The first round holds its model; the second question is let go once the box holds it.
    let held = gate.clone();
    let holding = Arc::clone(&fakes);
    let asked = id.clone();
    let releaser = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while std::time::Instant::now() < deadline {
            let stored = suzatary_lines::secretary::conversations::read(
                holding.conversations.conversations(),
                &asked,
            )
            .expect("reads");
            let questions = stored
                .turns
                .iter()
                .filter(|said| {
                    matches!(
                        said.turn,
                        suzatary_conversations::secretary::model::Turn::Person { .. }
                    )
                })
                .count();
            if questions >= 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        held.release();
    });
    let second = ask::ask(
        &common::operator(&running.base),
        &id,
        "is the garden invoice paid",
    )
    .expect("the second question is answered");
    releaser.join().expect("the releaser ends");
    let first = first
        .join()
        .expect("the first ask ends")
        .expect("the first question is answered");
    assert_eq!(first.answer, "The permit is due Friday.");
    assert_eq!(second.answer, "The garden invoice is paid.");
}

/// The transport is never asked, and the card waits for the owner's tap.
#[test]
fn an_operators_tap_on_a_card_that_sends_is_refused() {
    let (fakes, running) = running(
        "operator-send",
        FakeModel::saying([
            r#"{"say": "Drafted.", "calls": [{"verb": "propose_email", "args": {
                "account": "ada@example.com", "to": "counsel@example.test",
                "subject": "Thursday", "body": "Thursday works.",
                "why": "Counsel asked which day works.", "sources": "todo:life/permitfiling"
            }}]}"#,
            r#"{"say": "Proposed."}"#,
        ]),
    );
    let operator = common::operator(&running.base);
    let asked = ask::ask(
        &operator,
        &common::topic(fakes.conversations.conversations()),
        "tell counsel thursday works",
    )
    .expect("the operator asks");
    let proposal =
        &common::reading(&running.base, asked.conversation.as_str(), &[])["proposals"][0];
    assert_eq!(proposal["act"]["kind"], "email", "{proposal}");

    let refused = tap::tap(
        &operator,
        &suzatary_base::reference::ProposalRef {
            conversation: asked.conversation.clone(),
            index: 0,
        },
        "",
        &suzatary_server::secretary::routes::TapAnswer::Plain,
    )
    .expect_err("the operator does not send");
    let said = format!("{refused:#}");
    assert!(said.starts_with("403"), "{said}");
    assert!(said.contains("sending stays the owner's tap"), "{said}");
    assert!(
        running.sent.sent().is_empty(),
        "the transport was never asked"
    );
    let proposal =
        &common::reading(&running.base, asked.conversation.as_str(), &[])["proposals"][0];
    assert_eq!(proposal["state"], "standing", "{proposal}");
}
