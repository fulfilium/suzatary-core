//! `rich`, typst's HTML beside the source: the wire always carries the four keys, and an
//! untrusted todo is served as source until it is confirmed.
//!
//! `common::FakeRender` wraps the markup in `<p>…</p>`, so a test sees which fragment reached it
//! without typst on `PATH`.

mod common;

use common::Fakes;
use suzatary_core::render::Render as _;

fn base() -> String {
    let fakes = Fakes::new("markup");
    let deployment = common::deployment([], []);
    common::start(
        std::sync::Arc::new(fakes),
        deployment,
        common::loopback_is_this_box(),
    )
}

fn rich(base: &str, todo: &str) -> serde_json::Value {
    let mut response = common::get(&format!("{base}/api/todo/{todo}"));
    assert_eq!(common::status(&response), 200, "{todo}");
    common::body_json(&mut response)["entry"]["authored"]["rich"].clone()
}

/// The four rich fields are always there, null before anything compiles; each present field's key is named
/// beside them whether it compiled or not.
#[test]
fn the_four_keys_are_always_there_and_null_before_anything_compiles() {
    let base = base();
    let body = rich(&base, "file-permit");
    for field in ["body", "detail", "rationale", "waiting_on"] {
        assert_eq!(body[field], serde_json::Value::Null, "{field}");
    }
    let keys = &body["keys"];
    for field in ["body", "detail", "rationale", "waiting_on"] {
        assert!(keys.get(field).is_some(), "{field}'s key is there");
    }
    assert!(keys["body"].is_string(), "a present body is keyed");
}

/// The source is kept beside the rendering, so `rich` never replaces `body`.
#[test]
fn the_source_is_kept_beside_the_rendering() {
    let base = base();
    let mut response = common::get(&format!("{base}/api/todo/file-permit"));
    let body = common::body_json(&mut response);
    assert_eq!(
        body["entry"]["authored"]["body"],
        "File the *permit* renewal"
    );
}

/// A warmed trusted todo comes back rendered from the fragment it carries.
#[test]
fn a_warmed_trusted_todo_comes_back_rendered() {
    let fakes = Fakes::new("markup-warm");
    // `Render::cached` only answers what was already compiled, so warm before the server starts.
    let escaped =
        suzatary_core::escape::Escaped::from_chain("File the *permit* renewal".to_owned());
    fakes.render.warm(std::slice::from_ref(&escaped));
    let deployment = common::deployment([], []);
    let base = common::start(
        std::sync::Arc::new(fakes),
        deployment,
        common::loopback_is_this_box(),
    );
    let body = rich(&base, "file-permit");
    assert_eq!(body["body"], "<p>File the *permit* renewal</p>");
}

/// An untrusted todo is served as source even when compiled, until confirming re-appends it as
/// `ada`.
#[test]
fn an_untrusted_todo_is_source_until_it_is_confirmed() {
    let ada = common::login("ada@example.com");
    let fakes = Fakes::new("markup-confirm")
        .whois(common::FakeWhois::new().naming(common::v4(), ada.clone()));
    // Compiled on purpose: the route must refuse it even when the cache holds it.
    let escaped =
        suzatary_core::escape::Escaped::from_chain("Email *Harrow* about the deed".to_owned());
    fakes.render.warm(std::slice::from_ref(&escaped));
    let deployment = common::deployment([ada.clone()], [(ada, common::actor("ada"))]);
    let caps = std::sync::Arc::new(fakes);
    let base = common::start(std::sync::Arc::clone(&caps), deployment, Vec::new());

    assert!(rich(&base, "email-harrow")["body"].is_null());

    // The box authors the re-append and the replica seals and hands it over.
    confirm(&base, &caps, "email-harrow");

    assert_eq!(
        rich(&base, "email-harrow")["body"],
        "<p>Email *Harrow* about the deed</p>"
    );
}

/// What the phone does with `POST /api/author`'s answer: seal it, and sync it.
fn confirm(base: &str, caps: &Fakes, todo: &str) {
    let mut authored = common::act(
        &format!("{base}/api/author"),
        &serde_json::json!({ "confirm": { "todo": todo } }),
    );
    assert_eq!(
        common::status(&authored),
        200,
        "{:?}",
        common::body_json(&mut authored)
    );
    let print = common::body_json(&mut authored)["event"]
        .as_str()
        .expect("an event print")
        .to_owned();
    let event: suzatary_core::record::TodoEvent = prodrome_core::event::parse_event(
        &suzatary_core::record::TodoVocabulary::default(),
        &print,
    )
    .expect("the core printed it");
    let object = common::written(&caps.chain, event);
    let mut posted = common::act(
        &format!("{base}/api/objects"),
        &serde_json::json!({"objects": common::signed_prints(&object)}),
    );
    assert_eq!(
        common::status(&posted),
        200,
        "{:?}",
        common::body_json(&mut posted)
    );
}

/// A todo a replica just sealed comes back rendered, because `POST /api/objects` compiles what it
/// adopted before answering.
#[test]
fn a_todo_added_now_comes_back_rendered() {
    let ada = common::login("ada@example.com");
    let fakes =
        Fakes::new("markup-add").whois(common::FakeWhois::new().naming(common::v4(), ada.clone()));
    let deployment = common::deployment([ada.clone()], [(ada, common::actor("ada"))]);
    let caps = std::sync::Arc::new(fakes);
    let base = common::start(std::sync::Arc::clone(&caps), deployment, Vec::new());

    let mut authored = common::act(
        &format!("{base}/api/author"),
        &serde_json::json!({"add": {
            "body": "Send the letter",
            "category": "work",
            "priority": {"how": "flat", "percent": 40},
            "rationale": "because",
        }}),
    );
    assert_eq!(
        common::status(&authored),
        200,
        "{:?}",
        common::body_json(&mut authored)
    );
    let answer = common::body_json(&mut authored);
    let event: suzatary_core::record::TodoEvent = prodrome_core::event::parse_event(
        &suzatary_core::record::TodoVocabulary::default(),
        answer["event"].as_str().expect("a print"),
    )
    .expect("the core printed it");
    let object = common::written(&caps.chain, event);
    let posted = common::act(
        &format!("{base}/api/objects"),
        &serde_json::json!({"objects": common::signed_prints(&object)}),
    );
    assert_eq!(common::status(&posted), 200);

    assert_eq!(
        rich(&base, answer["todo"].as_str().expect("a todo id"))["body"],
        "<p>Send the letter</p>"
    );
}
