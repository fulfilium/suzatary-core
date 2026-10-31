//! `/api/mail/*` over a faked `Mailbox` (notmuch is not in CI): everything above that door is
//! real, including the reverse index joining a message to the todo it produced. Read as this
//! box throughout; who may read mail is `gate.rs`'s. The query string that reaches notmuch is the
//! interpreter's, tested in `suzatary_io::mailbox`, not here.

mod common;

use common::Fakes;
use suzatary_core::mail::{MailHeader, MailLink, MailMessage, MailRow};

fn a_row(hash: &str, account: &str) -> MailRow {
    MailRow {
        hash: hash.to_owned(),
        id: format!("{hash}@example.com"),
        sender: "counsel@example.com".to_owned(),
        subject: "Permit renewal for case 12345".to_owned(),
        date: None,
        container: "inbox".to_owned(),
        account: account.to_owned(),
        disposition: suzatary_core::mail::Disposed::Undecided,
        held: suzatary_core::held_value::Kinds::empty(),
    }
}

fn a_message(hash: &str) -> MailMessage {
    MailMessage {
        row: a_row(hash, "ada@university.example"),
        thread: "000000000000c3fb".to_owned(),
        headers: vec![
            MailHeader {
                name: "From".to_owned(),
                value: "counsel@example.com".to_owned(),
            },
            MailHeader {
                name: "Subject".to_owned(),
                value: "Permit renewal for case 12345".to_owned(),
            },
        ],
        body: "The permit is due.".to_owned(),
        truncated: false,
        links: vec![MailLink {
            url: "https://example.com/permit".to_owned(),
            label: "Upload documents".to_owned(),
        }],
    }
}

fn start_with(mailbox: common::FakeMailbox) -> String {
    let fakes = Fakes::new("mail").mailbox(mailbox);
    let deployment = common::deployment([], []);
    common::start(
        std::sync::Arc::new(fakes),
        deployment,
        common::loopback_is_this_box(),
    )
}

/// The limit is capped, not obeyed.
#[test]
fn the_limit_is_capped_not_obeyed() {
    let base = start_with(common::FakeMailbox::new());
    let response = common::get(&format!("{base}/api/mail/inbox?limit=100000"));
    assert_eq!(common::status(&response), 200);
    for bad in ["0", "-3", "many"] {
        let response = common::get(&format!("{base}/api/mail/inbox?limit={bad}"));
        assert_eq!(common::status(&response), 400, "{bad}");
    }
}

/// A search with no query is a 400, not the whole corpus.
#[test]
fn a_search_with_no_query_is_400() {
    let base = start_with(common::FakeMailbox::new());
    assert_eq!(
        common::status(&common::get(&format!("{base}/api/mail/search"))),
        400
    );
    assert_eq!(
        common::status(&common::get(&format!("{base}/api/mail/search?q=%20"))),
        400
    );
}

/// The todos a message produced come back with it, over the six-todo chain's `email-harrow`.
#[test]
fn the_todos_a_message_produced_come_back_with_it() {
    let hash = common::AGENTS_TODO_MESSAGE_HASH;
    let mailbox = common::FakeMailbox::new().with_message(hash, a_message(hash));
    let base = start_with(mailbox);
    let mut response = common::get(&format!("{base}/api/mail/message/{hash}"));
    assert_eq!(common::status(&response), 200, "{}", response.status());
    let body = common::body_json(&mut response);
    let todos: Vec<&str> = body["todos"]
        .as_array()
        .expect("todos")
        .iter()
        .map(|t| t["todo"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(todos, vec!["email-harrow"]);
}

/// EVERY READING TOLERATES A DANGLING MAIL REFERENCE: with the message gone (its account removed
/// with its mail), the message reads as no longer stored with what the todo citing it recorded,
/// its conversation reads the same words, and the todo itself still reads whole.
#[test]
fn every_reading_tolerates_a_message_no_longer_stored() {
    let hash = common::AGENTS_TODO_MESSAGE_HASH;
    let base = start_with(common::FakeMailbox::new());
    let mut message = common::get(&format!("{base}/api/mail/message/{hash}"));
    assert_eq!(common::status(&message), 404);
    let said = common::body_json(&mut message)["error"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(said.starts_with(suzatary_core::mail::gone::SAID), "{said}");
    assert!(
        said.contains("“"),
        "the subject a todo recorded is named: {said}"
    );
    let mut thread = common::get(&format!("{base}/api/mail/thread/000000000000c3fb"));
    assert_eq!(common::status(&thread), 404);
    assert!(
        common::body_json(&mut thread)["error"]
            .as_str()
            .is_some_and(|said| said.starts_with(suzatary_core::mail::gone::SAID))
    );
    let todo = common::get(&format!("{base}/api/todo/email-harrow"));
    assert_eq!(common::status(&todo), 200, "the todo citing it still reads");
}

/// An id that would be notmuch syntax is refused before the corpus: the fake
/// mailbox is never asked.
#[test]
fn an_id_that_would_be_notmuch_syntax_is_refused_before_the_corpus() {
    let mailbox = common::FakeMailbox::new();
    let fakes = Fakes::new("mail-syntax").mailbox(mailbox);
    let caps = std::sync::Arc::new(fakes);
    let deployment = common::deployment([], []);
    let base = common::start(
        std::sync::Arc::clone(&caps),
        deployment,
        common::loopback_is_this_box(),
    );
    for bad in ["a b@x", "a\"b@x", "a*@x", "a(b)@x", "ab"] {
        let response = common::get(&format!(
            "{base}/api/mail/message/{}",
            urlencoding_lite(bad)
        ));
        assert_eq!(common::status(&response), 400, "{bad}");
    }
    assert_eq!(caps.mailbox.asked_messages(), Vec::<String>::new());
}

/// Every mail route is a read: none of them appears among the writing routes.
#[test]
fn every_mail_route_is_a_read() {
    let base = start_with(common::FakeMailbox::new());
    for path in [
        "/api/mail/inbox",
        "/api/mail/search?q=permit",
        &format!("/api/mail/message/{}", common::AGENTS_TODO_MESSAGE_HASH),
        "/api/mail/thread/000000000000c3fb",
    ] {
        let response = common::agent()
            .post(format!("{base}{path}"))
            .send_json(serde_json::json!({}))
            .expect("the request completes");
        // A POST to a read route has no handler, so axum answers 405.
        assert_eq!(common::status(&response), 405, "{path}");
    }
}

/// A tiny, dependency-free percent-encoder for the handful of characters the
/// notmuch-syntax test needs to get through the URL unmangled.
fn urlencoding_lite(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            ' ' => "%20".to_owned(),
            '"' => "%22".to_owned(),
            '*' => "%2A".to_owned(),
            '(' => "%28".to_owned(),
            ')' => "%29".to_owned(),
            other => other.to_string(),
        })
        .collect()
}
