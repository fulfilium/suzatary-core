//! Push when nobody is watching, over a real server with `v4` committed to `ada@example.com`
//! (see `common`'s header on peer identities): a round that answers with no subscriber to
//! `GET …/stream` pushes exactly once, and with a subscriber connected it pushes nothing.

use crate::common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_base::capability::Login {
    common::login("ada@example.com")
}

fn writer(
    tag: &str,
    model: FakeModel,
) -> (String, Arc<Fakes>, Arc<suzatary_server::serve::Workers>) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(Fakes::new(tag).whois(whois).model(model));
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let common::Running { base, workers, .. } =
        common::Server::new(Arc::clone(&fakes), deployment, Vec::new()).start();
    (base, fakes, workers)
}

fn open(base: &str) -> String {
    common::opened(base, "A morning")
}

fn say(base: &str, id: &str, text: &str) {
    let _ = common::say(base, id, text);
}

/// Poll for the push, not just for idle: the worker writes the final turn and only then pushes.
fn pushed(fakes: &Fakes) -> Vec<suzatary_cards::notify::Push> {
    for _ in 0..400 {
        let pushed = fakes.notify.pushed();
        if !pushed.is_empty() {
            return pushed;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    Vec::new()
}

/// The one push names the conversation and carries none of the answer's words.
#[test]
fn no_subscriber_earns_one_push_naming_the_conversation() {
    let (base, fakes, _workers) = writer(
        "push-no-subscriber",
        FakeModel::saying(["{\"say\": \"All good. Nothing urgent.\"}"]),
    );
    let id = open(&base);

    say(&base, &id, "What changed?");
    common::settled(&base, &id);

    let pushed = pushed(&fakes);
    assert_eq!(
        pushed.len(),
        1,
        "exactly one push for the one round: {pushed:?}"
    );
    assert_eq!(
        pushed[0],
        suzatary_cards::notify::Push::Answered {
            conversation: suzatary_base::conversation_id::ConversationId::new(id.as_str())
                .expect("a slug"),
            brief: false,
        }
    );
    assert_eq!(
        pushed[0].printed("https://example.test").click,
        format!("https://example.test/#/secretary/{id}")
    );
}

#[test]
fn a_connected_subscriber_gets_no_push_at_all() {
    let (base, fakes, _workers) = writer(
        "push-with-subscriber",
        FakeModel::saying(["{\"say\": \"All good. Nothing urgent.\"}"]),
    );
    let id = open(&base);

    // A blocking `.call()` returns only once the subscription exists.
    let response = common::agent()
        .get(format!("{base}/api/secretary/conversations/{id}/stream"))
        .call()
        .expect("the stream connects");
    // Dropping `response` would close the subscription.
    let _held = response;

    say(&base, &id, "What changed?");
    common::settled(&base, &id);

    assert_eq!(
        fakes.notify.pushed(),
        Vec::new(),
        "a subscriber was watching; nothing should have been pushed"
    );
}
