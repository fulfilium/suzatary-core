//! The box's operator, over a real server whose daemon names loopback as the box's own node: a
//! question asked at the terminal is the owner's words stamped as the operator's, and a card that
//! sends mail is refused to the operator's tap with nothing sent.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};
use suzatary_cli::commands::{ask, tap};
use suzatary_core::cast::CAST;
use suzatary_core::seat::Seat;
use suzatary_core::secretary::stamp::{Speaker, Stamp};

fn owner() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
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

/// A question in a fresh topic is said through the say route and lands as the owner's own line,
/// stamped as asked at the box's terminal, and the box records the request under the root key,
/// so both tell it from a device's.
#[test]
fn an_operators_question_is_the_owners_words_stamped_as_the_operators() {
    let (fakes, running) = running(
        "operator-ask",
        FakeModel::saying([r#"{"say": "Nothing is due this week."}"#]),
    );
    let asked = ask::ask(
        &common::operator(&running.base),
        &ask::Conversation::New,
        "what is due this week",
    )
    .expect("the operator asks");
    assert!(asked.answer.contains("Nothing is due"), "{}", asked.answer);
    let stored =
        suzatary_core::secretary::read(fakes.conversations.conversations(), &asked.conversation)
            .expect("reads");
    let said = stored
        .turns
        .iter()
        .find(|said| matches!(said.turn, suzatary_core::capability::Turn::Person { .. }))
        .expect("the question's line");
    assert_eq!(
        said.by,
        Some(Stamp::Actor(Speaker::at(CAST.owner.actor, Seat::Operator)))
    );
    assert!(Stamp::owners(said.by.as_ref()), "the owner's words");
    assert!(!suzatary_core::secretary::taint::tainted(&stored));
    let record = running
        .requests
        .get(&asked.request)
        .expect("the records read")
        .expect("the question is recorded");
    assert_eq!(record.key, common::cast_keys().root().public().as_str());
    assert_eq!(record.actor, CAST.owner.actor);
}

/// The operator's tap on a card that sends mail is refused with the reason, and the transport is
/// never asked; the card waits for the owner's tap.
#[test]
fn an_operators_tap_on_a_card_that_sends_is_refused() {
    let (_fakes, running) = running(
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
        &ask::Conversation::New,
        "tell counsel thursday works",
    )
    .expect("the operator asks");
    let proposal =
        &common::reading(&running.base, asked.conversation.as_str(), &[])["proposals"][0];
    assert_eq!(proposal["act"]["kind"], "email", "{proposal}");

    let refused = tap::tap(
        &operator,
        &suzatary_core::reference::ProposalRef {
            conversation: asked.conversation.clone(),
            index: 0,
        },
        "",
        &suzatary_core::secretary::routes::TapAnswer::Plain,
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
