//! The secretary on the web, by permission: a host an allow rule covers is read at once, any
//! other becomes a card with the URL and a drafted rule, and the tap reads it.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment([ada()], [(ada(), common::actor("ada"))])
}

fn web_call(url: &str) -> String {
    format!(
        r#"{{"say": "Let me read it.",
            "calls": [{{"verb": "web", "args": {{
              "url": "{url}",
              "always": "*.example.com",
              "why": "Ada asked what the page says.",
              "sources": "todo:life/permitfiling"
            }}}}]}}"#
    )
}

/// Open a conversation, say something, and wait until nobody is answering.
fn exchange(base: &str, said: &str) -> (String, serde_json::Value) {
    common::exchange(base, "The web", said)
}

fn web_answers(payload: &serde_json::Value) -> Vec<String> {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["who"] == "verb" && turn["verb"] == "web")
        .map(|turn| turn["text"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn a_host_an_allow_rule_covers_is_read_at_once_and_asks_nothing() {
    let fakes = Arc::new(
        Fakes::new("web-allowed")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .allowing("*.example.com")
            .model(FakeModel::saying([
                web_call("https://docs.example.com/permit").leak(),
                r#"{"say": "It says what I expected."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "what does the permit page say");

    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "an allowed read asks nothing: {payload:?}"
    );
    let answers = web_answers(&payload);
    assert!(
        answers
            .iter()
            .any(|text| text.contains("the page for: open https://docs.example.com/permit")),
        "the page is the verb's answer: {answers:?}"
    );
    assert_eq!(fakes.web.done.lock().expect("the log").len(), 1);
}

/// A round that has read mail may be asking for that mail, and a fetch carries its URL off the
/// box: so there an allowed host is asked on a card too, as `remember` asks.
#[test]
fn a_round_that_read_mail_asks_even_for_an_allowed_host() {
    let fakes = Arc::new(
        Fakes::new("web-tainted")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .allowing("*.example.com")
            .model(FakeModel::saying([
                r#"{"say": "", "calls": [{"verb": "search", "args": {"query": "permit"}}]}"#,
                web_call("https://docs.example.com/permit").leak(),
                r#"{"say": "I asked to read it."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "what does the permit mail point at");

    assert!(
        fakes.web.done.lock().expect("the log").is_empty(),
        "nothing was fetched"
    );
    let proposals = payload["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 1, "{payload:?}");
    assert_eq!(proposals[0]["act"]["kind"], "web");
    assert_eq!(
        proposals[0]["act"]["target"],
        "https://docs.example.com/permit"
    );
    assert!(
        web_answers(&payload)
            .iter()
            .any(|text| text.contains("not the owner's own words")),
        "the answer says why: {payload:?}"
    );
}

#[test]
fn any_other_host_is_a_request_on_a_card_with_the_exact_url_and_the_rule() {
    let fakes = Arc::new(
        Fakes::new("web-asked")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .allowing("*.wikipedia.org")
            .model(FakeModel::saying([
                web_call("https://forms.example.com/a?id=7").leak(),
                r#"{"say": "I asked; tap it if you want me to read it."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "read the form page");

    assert!(
        fakes.web.done.lock().expect("the log").is_empty(),
        "nothing was fetched"
    );
    let proposals = payload["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 1, "{payload:?}");
    let act = &proposals[0]["act"];
    assert_eq!(act["kind"], "web");
    assert_eq!(act["request"], "fetch");
    assert_eq!(
        act["target"], "https://forms.example.com/a?id=7",
        "the URL whole"
    );
    assert_eq!(act["host"], "forms.example.com");
    assert_eq!(act["always"], "any page on example.com or its subdomains");
}

#[test]
fn a_private_url_is_refused_before_anything_is_asked() {
    let fakes = Arc::new(
        Fakes::new("web-private")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .model(FakeModel::saying([
                web_call("http://127.0.0.1:8787/api/chain").leak(),
                r#"{"say": "That address is refused."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "read the box's own chain");

    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty()
    );
    let refused = payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .any(|turn| turn["verb"] == "web" && turn["outcome"]["is"] == "refused");
    assert!(refused, "the verb refused it: {payload:?}");
}

/// The tap reads the page through the CLI performer's fixture arm and lands it as a `web` answer.
#[cfg(feature = "fixture")]
#[test]
fn a_tapped_request_puts_the_page_in_the_conversation() {
    let fakes = Arc::new(
        Fakes::new("web-tapped")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .model(FakeModel::saying([
                web_call("https://forms.example.com/a").leak(),
                r#"{"say": "Asked."}"#,
                r#"{"say": "Read it."}"#,
            ])),
    );
    let conversations = common::again(&fakes.conversations);
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .recording_acts(conversations)
        .start()
        .base;
    let (id, _) = exchange(&base, "read the form page");

    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let payload = common::settled(&base, &id);
    let answers = web_answers(&payload);
    assert!(
        answers
            .iter()
            .any(|text| text.contains("the fixture's page for: open https://forms.example.com/a")),
        "the tap's page is in the transcript: {answers:?}"
    );
    assert_eq!(payload["proposals"][0]["state"], "acted", "{payload:?}");
}

/// "Always allow" writes the rule the card showed, then reads the page.
#[cfg(feature = "fixture")]
#[test]
fn always_allow_writes_the_drafted_rule_and_reads_the_page() {
    let fakes = Arc::new(
        Fakes::new("web-always")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .model(FakeModel::saying([
                web_call("https://forms.example.com/a").leak(),
                r#"{"say": "Asked."}"#,
                r#"{"say": "Read it, and it is allowed from now on."}"#,
            ])),
    );
    let edits = Arc::new(common::EditsRecorder::new());
    let conversations = common::again(&fakes.conversations);
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .recording_acts(conversations)
        .settings(common::SettingsFixture::with_edits(Arc::clone(&edits) as _))
        .start()
        .base;
    let (id, _) = exchange(&base, "read the form page");

    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({ "always": true }),
    );
    assert_eq!(common::status(&tapped), 200);
    assert_eq!(
        edits
            .allowed()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["*.example.com"],
        "the rule the card offered, and only it"
    );
    let payload = common::settled(&base, &id);
    assert!(
        web_answers(&payload)
            .iter()
            .any(|text| text.contains("the fixture's page for: open https://forms.example.com/a")),
        "and the page was read too"
    );
}

/// `always` on a request that offers no rule is refused, and nothing runs.
#[cfg(feature = "fixture")]
#[test]
fn always_is_refused_where_no_rule_was_offered() {
    let fakes = Arc::new(
        Fakes::new("web-always-norule")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .model(FakeModel::saying([
                r#"{"say": "Let me read it.",
                    "calls": [{"verb": "web", "args": {
                      "url": "https://forms.example.com/a",
                      "why": "Ada asked what the page says.",
                      "sources": "todo:life/permitfiling"
                    }}]}"#,
                r#"{"say": "Asked."}"#,
            ])),
    );
    let edits = Arc::new(common::EditsRecorder::new());
    let conversations = common::again(&fakes.conversations);
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .recording_acts(conversations)
        .settings(common::SettingsFixture::with_edits(Arc::clone(&edits) as _))
        .start()
        .base;
    let (id, _) = exchange(&base, "read the form page");
    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({ "always": true }),
    );
    assert_eq!(common::status(&tapped), 400);
    assert!(edits.allowed().is_empty());
    // The verb's own "asked" answer is a `web` turn too; what must be absent is a page.
    assert!(
        !web_answers(&common::settled(&base, &id))
            .iter()
            .any(|text| text.contains("the fixture's page for")),
        "no page was read"
    );
}
