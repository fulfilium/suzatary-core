//! A held value over a real server. Its reveal (`POST /api/held/reveal`) gives a trusted device of
//! an actor the value, once; an untrusted device, a viewer and a replay get nothing, and the box
//! keeps the value in no request record or kept answer. A proposed email names it by its
//! placeholder, and only the owner's tap writes the value into the message on the wire.

mod common;

use std::sync::Arc;

use common::{FakeCorpus, FakeMessage, FakeModel, Fakes};
use suzatary_core::capability::MailError;
use suzatary_core::held_value::{Handle, HeldFile, Reopened, Reveal, Shown};
use suzatary_core::redact::Kind;

/// A synthetic SSN no person holds.
const SSN: &str = "000-12-3456";

/// The message that holds the value, by the hash `show` takes and the notmuch id the fake corpus
/// answers it with.
const HASH: &str = "f0f0f0f0f0f0";
const MESSAGE: &str = "id-f0f0f0f0f0f0";

/// A box whose mail holds one value, at `MESSAGE`'s first place.
struct OneValue;

impl Reveal for OneValue {
    fn value(&self, message: &str, ordinal: u16) -> Result<Option<Shown>, MailError> {
        Ok((message == MESSAGE && ordinal == 0).then(|| Shown::new(Kind::Ssn, SSN)))
    }

    fn original(&self, _: &str, _: u64) -> Result<Reopened, MailError> {
        Ok(Reopened::Gone)
    }
}

fn handle(message: &str, ordinal: u16) -> String {
    Handle::new(message, ordinal).expect("a handle").print()
}

/// A box over one value, its peer `login` mapped to the actor ada when `acts`.
fn serving(tag: &str, login: &str, acts: bool) -> common::Running {
    let login = common::login(login);
    let fakes = Arc::new(
        Fakes::new(tag).whois(common::FakeWhois::new().naming(common::v4(), login.clone())),
    );
    let actors = if acts {
        vec![(login.clone(), common::actor("ada"))]
    } else {
        Vec::new()
    };
    let deployment = common::deployment([login], actors);
    common::Server::new(fakes, deployment, Vec::new())
        .reveal(Arc::new(OneValue))
        .start()
}

fn answered(response: ureq::http::Response<ureq::Body>) -> (u16, String) {
    let status = common::status(&response);
    let mut response = response;
    let text = response.body_mut().read_to_string().expect("a body");
    (status, text)
}

/// Every file under `dir`, read, whether any holds `text`.
fn holds(dir: &std::path::Path, text: &str) -> bool {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| {
            let path = entry.path();
            if path.is_dir() {
                holds(&path, text)
            } else {
                std::fs::read(&path)
                    .is_ok_and(|bytes| bytes.windows(text.len()).any(|w| w == text.as_bytes()))
            }
        })
}

#[test]
fn a_trusted_device_reveals_the_value_once_and_the_box_keeps_none_of_it() {
    let running = serving("held-trusted", "ada@example.com", true);
    let url = format!("{}/api/held/reveal", running.base);
    let body = serde_json::json!({ "handle": handle(MESSAGE, 0) });

    let (status, text) = answered(common::act_signed(&url, &body, &common::phone()));
    assert_eq!(status, 200, "{text}");
    let shown: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(shown, serde_json::json!({ "type": "Shown", "value": SSN }));

    // The same request again is a replay: the box answers what it kept, which is not the value.
    let (status, text) = answered(common::act_signed(&url, &body, &common::phone()));
    assert_eq!((status, text.as_str()), (200, r#"{"type":"NotKept"}"#));

    let port = running.port();
    for dir in [
        common::requests_dir(port),
        std::env::temp_dir().join(format!("suzatary-test-answers-{port}")),
    ] {
        assert!(!holds(&dir, SSN), "{} holds the value", dir.display());
        assert!(!holds(&dir, "3456"), "{} holds its digits", dir.display());
    }

    let gone = serde_json::json!({ "handle": handle(MESSAGE, 1) });
    let (status, text) = answered(common::act(&url, &gone));
    assert_eq!((status, text.as_str()), (200, r#"{"type":"Gone"}"#));
}

#[test]
fn an_untrusted_device_a_viewer_and_a_malformed_handle_are_refused() {
    let running = serving("held-refused", "ada@example.com", true);
    let url = format!("{}/api/held/reveal", running.base);
    let body = serde_json::json!({ "handle": handle(MESSAGE, 0) });
    let stranger = prodrome_core::sign::Secret::new(&"7".repeat(64)).expect("a seed");
    let (status, text) = answered(common::act_signed(&url, &body, &stranger));
    assert_eq!(status, 403, "{text}");
    assert!(!text.contains(SSN), "{text}");
    let (status, text) = answered(common::act_unsigned(&url, &body));
    assert_ne!(status, 200, "{text}");
    assert!(!text.contains(SSN), "{text}");
    let (status, _) = answered(common::act(
        &url,
        &serde_json::json!({ "handle": "held:not base64.0" }),
    ));
    assert_eq!(status, 400);

    let viewing = serving("held-viewer", "bo@example.com", false);
    let url = format!("{}/api/held/reveal", viewing.base);
    let (status, text) = answered(common::act(&url, &body));
    assert_eq!(status, 403, "{text}");
    assert!(!text.contains(SSN), "{text}");
}

/// A model that reads the message, then proposes three emails naming a held value in the body (the
/// one the box holds, one at a place the message does not hold, and the held one under a forged
/// kind) and four the verb refuses (a withheld value in the body, a held value in the subject, a
/// code, and a value from a message the conversation did not read).
fn proposing() -> FakeModel {
    let held = |kind: &str, ordinal| {
        format!(
            "[{kind} ending 3456 · tap to show · {}]",
            handle(MESSAGE, ordinal)
        )
    };
    let email = |subject: &str, body: String| {
        serde_json::json!({"verb": "propose_email", "args": {
            "account": "her@example.invalid",
            "to": "counsel@example.test",
            "subject": subject,
            "body": body,
            "why": "Counsel asked for the number for the filing.",
            "sources": "mail:a1b2c3d4e5f6",
        }})
    };
    FakeModel::saying_owned([
        serde_json::json!({"say": "Here are the drafts.", "calls": [
            {"verb": "show", "args": {"id": HASH}},
            email("The filing", format!("My SSN is {}, for the form.", held("SSN", 0))),
            email("The filing again", format!("It is {}.", held("SSN", 1))),
            email("The filing, forged", format!("It is {}.", held("ITIN", 0))),
            email("Withheld", "It is [SSN ending 3456 · withheld].".to_owned()),
            email(&held("SSN", 0), "In the subject.".to_owned()),
            email("A code", format!(
                "It is [verification code · tap to show · {}].",
                handle(MESSAGE, 0)
            )),
            email("Unread", format!(
                "It is [SSN ending 3456 · tap to show · {}].",
                handle("other@example.test", 0)
            )),
        ]})
        .to_string(),
        r#"{"say": "Proposed."}"#.to_owned(),
    ])
}

/// The conversation once nobody is answering.
fn settled(base: &str, id: &str) -> serde_json::Value {
    for _ in 0..200 {
        let payload = common::reading(base, id, &[]);
        if payload["state"] == "idle" {
            return payload;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("the conversation never settled");
}

#[test]
fn a_held_value_reaches_only_the_wire_and_only_on_the_owners_tap() {
    let login = common::login("ada@example.com");
    let fakes = Arc::new(
        Fakes::new("held-send")
            .whois(common::FakeWhois::new().naming(common::v4(), login.clone()))
            .model(proposing())
            .corpus(FakeCorpus::new().with(FakeMessage {
                hash: HASH.to_owned(),
                thread: "t1".to_owned(),
                sender: "irs@example.test".to_owned(),
                subject: "Your form".to_owned(),
                body: "The form you sent.".to_owned(),
                timestamp: "2026-10-01T09:00:00".to_owned(),
            })),
    );
    let deployment = common::deployment([login.clone()], [(login, common::actor("ada"))]);
    let running = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .reveal(Arc::new(OneValue))
        .start();
    let base = &running.base;

    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "The filing"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": "draft the filing reply"}),
    );
    assert_eq!(common::status(&said), 202);
    let proposed = settled(base, &id);
    let proposals = proposed["proposals"].as_array().expect("proposals");
    assert_eq!(
        proposals.len(),
        3,
        "the withheld, subject, code and unread drafts are refused: {proposals:?}"
    );
    assert!(running.sent.sent().is_empty(), "proposing sends nothing");
    let sentence = proposals[0]["card"]["sentence"]
        .as_str()
        .expect("a sentence");
    assert!(
        sentence.starts_with("Includes your SSN ending 3456, from ")
            && sentence.contains("Everyone on To and Cc receives it: counsel@example.test."),
        "{sentence}"
    );

    let tap = |index: usize| {
        let mut tapped = common::act_signed(
            &format!("{base}/api/secretary/conversations/{id}/proposals/{index}/tap"),
            &serde_json::json!({}),
            &common::phone(),
        );
        assert_eq!(common::status(&tapped), 200);
        common::body_json(&mut tapped)
    };
    let after = tap(0);
    assert_eq!(after["proposals"][0]["state"], "sent");
    let sent = running.sent.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].body, format!("My SSN is {SSN}, for the form."));

    for (index, why) in [
        (1, "the SSN ending 3456 is no longer held"),
        (
            2,
            "the message or file now holds a different value where the ITIN ending 3456 was",
        ),
    ] {
        let after = tap(index);
        let row = &after["proposals"][index];
        assert_eq!(row["state"], "failed", "{row}");
        let outcome = row["outcome"].as_str().expect("an outcome");
        assert!(outcome.contains(why), "{outcome}");
    }
    assert_eq!(running.sent.sent().len(), 1, "a refused tap sends nothing");

    // The value is in no prompt, stored line, kept answer or request record.
    let asked = format!("{:?}{:?}", fakes.model.asked(), fakes.model.resumed_with());
    assert!(!asked.contains(SSN), "a prompt holds the value");
    let port = running.port();
    for dir in [
        fakes.root.clone(),
        common::requests_dir(port),
        std::env::temp_dir().join(format!("suzatary-test-answers-{port}")),
    ] {
        assert!(!holds(&dir, SSN), "{} holds the value", dir.display());
    }
    assert!(!settled(base, &id).to_string().contains(SSN));
}

/// A PDF that names the SSN in its text layer, which the box therefore holds back; the marker
/// shows where its bytes went.
const W2: &[u8] = b"%PDF-1.7\n% held-file-bytes-7f3a\n%%EOF\n";
const W2_BYTES: &str = "held-file-bytes-7f3a";

/// The PDF's text layer, as the fake reader reads every PDF.
fn w2_layer() -> String {
    format!("Employee SSN: {SSN}. Wages: 70,000.\u{c}")
}

/// The message the withheld PDF is attached to, and one whose attachment no turn reads.
const W2_MESSAGE: &str = "w2-msg";
const OTHER_MESSAGE: &str = "other-msg";

fn attached(message: &str) -> HeldFile {
    HeldFile::Mail {
        message: message.to_owned(),
        name: "w2.pdf".to_owned(),
    }
}

/// A box whose mail holds no value of its own: the values here are in the PDF.
struct NoMailValue;

impl Reveal for NoMailValue {
    fn value(&self, _: &str, _: u16) -> Result<Option<Shown>, MailError> {
        Ok(None)
    }

    fn original(&self, _: &str, _: u64) -> Result<Reopened, MailError> {
        Ok(Reopened::Gone)
    }
}

/// A mailbox whose two messages each carry `W2`, read as the box reads it: withheld.
fn w2_mailbox() -> common::FakeMailbox {
    let mut mailbox = common::FakeMailbox::new();
    for id in [W2_MESSAGE, OTHER_MESSAGE] {
        mailbox = mailbox
            .with_message(
                id,
                suzatary_core::mail::MailMessage {
                    row: suzatary_core::mail::MailRow {
                        hash: "b2c3d4e5f6a1b2c3".to_owned(),
                        id: id.to_owned(),
                        sender: "payroll@example.test".to_owned(),
                        subject: "Your W-2".to_owned(),
                        date: None,
                        container: "inbox".to_owned(),
                        account: "ada@example.com".to_owned(),
                        disposition: suzatary_core::mail::Disposed::Undecided,
                    },
                    thread: "000000000000c3fc".to_owned(),
                    headers: Vec::new(),
                    body: "Your W-2 is attached.".to_owned(),
                    truncated: false,
                    links: Vec::new(),
                },
            )
            .with_attachment_read(
                id,
                "w2.pdf",
                suzatary_core::file_read::FileRead::of(
                    &attached(id),
                    "w2.pdf",
                    "application/pdf",
                    W2.to_vec(),
                    Some(&w2_layer()),
                ),
            )
            .with_attachment_file(
                id,
                suzatary_core::mail::AttachmentFile {
                    name: "w2.pdf".to_owned(),
                    mime: "application/pdf".to_owned(),
                    content: W2.to_vec(),
                },
            );
    }
    mailbox
}

/// The placeholders the box writes for the withheld PDF on `message`: the file itself, and the
/// SSN at the first place of its text layer.
fn w2_placeholders(message: &str) -> (String, String) {
    use suzatary_core::held_value::{Holder, Place, Placeholder, Reach};
    (
        Placeholder::file(&attached(message), "w2.pdf", W2.len() as u64).print(),
        Placeholder::of(
            Kind::Ssn,
            SSN,
            Reach::Revealable(Place::new(Holder::File(attached(message)), 0).expect("a place")),
        )
        .print(),
    )
}

/// A box over the two messages, its model saying `model`, its peer mapped to the actor ada.
fn serving_files(tag: &str, model: FakeModel) -> (Arc<Fakes>, common::Running) {
    let login = common::login("ada@example.com");
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(common::FakeWhois::new().naming(common::v4(), login.clone()))
            .model(model)
            .mailbox(w2_mailbox())
            .pdf_layer(&w2_layer()),
    );
    let deployment = common::deployment([login.clone()], [(login, common::actor("ada"))]);
    let running = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .reveal(Arc::new(NoMailValue))
        .start();
    (fakes, running)
}

/// A conversation opened on `running` that says `said` once and settles.
fn conversation(base: &str, said: &str) -> String {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "The W-2"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let answered = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({ "say": said }),
    );
    assert_eq!(common::status(&answered), 202);
    settled(base, &id);
    id
}

/// A model that reads the W-2 on `W2_MESSAGE` and stops.
fn reading_the_w2() -> FakeModel {
    FakeModel::saying_owned([
        serde_json::json!({"say": "Reading the W-2.", "calls": [
            {"verb": "attachments", "args": {"id": W2_MESSAGE, "name": "w2.pdf"}},
        ]})
        .to_string(),
        r#"{"say": "Read."}"#.to_owned(),
        r#"{"say": "Hello."}"#.to_owned(),
    ])
}

#[test]
fn a_trusted_device_opens_a_held_file_its_conversation_read_and_the_box_keeps_none_of_it() {
    use base64::Engine as _;
    use suzatary_core::held_value::{Holder, Place};
    let (_fakes, running) = serving_files("held-file-reveal", reading_the_w2());
    let id = conversation(&running.base, "what is on my W-2");
    let url = format!("{}/api/held/reveal", running.base);
    let encoded = base64::engine::general_purpose::STANDARD.encode(W2);
    let asking = |handle: &Handle, conversation: Option<&str>| serde_json::json!({ "handle": handle.print(), "conversation": conversation });
    let file = asking(&Handle::File(attached(W2_MESSAGE)), Some(&id));

    let stranger = prodrome_core::sign::Secret::new(&"7".repeat(64)).expect("a seed");
    let (status, text) = answered(common::act_signed(&url, &file, &stranger));
    assert_eq!(status, 403, "{text}");
    assert!(!text.contains(&encoded), "{text}");

    let (status, text) = answered(common::act_signed(&url, &file, &common::phone()));
    assert_eq!(status, 200, "{text}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("JSON"),
        serde_json::json!({
            "type": "File", "name": "w2.pdf", "media": "application/pdf", "bytes": encoded,
        })
    );
    let (status, text) = answered(common::act_signed(&url, &file, &common::phone()));
    assert_eq!((status, text.as_str()), (200, r#"{"type":"NotKept"}"#));

    let in_layer =
        Handle::Value(Place::new(Holder::File(attached(W2_MESSAGE)), 0).expect("a place"));
    let (status, text) = answered(common::act_signed(
        &url,
        &asking(&in_layer, Some(&id)),
        &common::phone(),
    ));
    assert_eq!(status, 200, "{text}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&text).expect("JSON"),
        serde_json::json!({ "type": "Shown", "value": SSN })
    );

    // A handle a model could forge: a file the conversation never read, the right file in a
    // conversation that never read it, a source no reader withholds, and a file's handle with
    // no conversation.
    let other = conversation(&running.base, "hello");
    let kept = format!(
        "held:file:{}.file",
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(format!("FromKept(id='{}')", "ab".repeat(32)))
    );
    for (body, status) in [
        (
            asking(&Handle::File(attached(OTHER_MESSAGE)), Some(&id)),
            403,
        ),
        (
            asking(&Handle::File(attached(W2_MESSAGE)), Some(&other)),
            403,
        ),
        (
            serde_json::json!({ "handle": kept, "conversation": id }),
            400,
        ),
        (asking(&Handle::File(attached(W2_MESSAGE)), None), 400),
    ] {
        let (got, text) = answered(common::act_signed(&url, &body, &common::phone()));
        assert_eq!(got, status, "{body}: {text}");
        assert!(!text.contains(&encoded), "{text}");
    }

    let port = running.port();
    for dir in [
        common::requests_dir(port),
        std::env::temp_dir().join(format!("suzatary-test-answers-{port}")),
    ] {
        for kept in [W2_BYTES, encoded.as_str(), SSN] {
            assert!(!holds(&dir, kept), "{} holds {kept}", dir.display());
        }
    }
}

/// A model that reads the withheld PDF, then proposes it and its SSN in one body, the PDF under
/// a withheld placeholder, the PDF under another name, and a file from a message it never read.
fn proposing_the_file() -> FakeModel {
    let (file, ssn) = w2_placeholders(W2_MESSAGE);
    let (unread, _) = w2_placeholders(OTHER_MESSAGE);
    let email = |subject: &str, body: String| {
        serde_json::json!({"verb": "propose_email", "args": {
            "account": "her@example.invalid",
            "to": "counsel@example.test",
            "subject": subject,
            "body": body,
            "why": "Counsel asked for the W-2.",
            "sources": "mail:b2c3d4e5f6a1",
        }})
    };
    FakeModel::saying_owned([
        serde_json::json!({"say": "Here is the draft.", "calls": [
            {"verb": "attachments", "args": {"id": W2_MESSAGE, "name": "w2.pdf"}},
            email("The W-2", format!("Attached is {file}; my SSN is {ssn}.")),
            email("Renamed", format!("Attached is {}.", file.replace("w2.pdf", "payslip.pdf"))),
            email("Withheld", format!("Attached is {}.", file.replace(
                &format!(" · tap to show · {}]", Handle::File(attached(W2_MESSAGE)).print()),
                " · withheld]",
            ))),
            email("Unread", format!("Attached is {unread}.")),
        ]})
        .to_string(),
        r#"{"say": "Proposed."}"#.to_owned(),
    ])
}

fn tap(base: &str, id: &str, index: usize) -> serde_json::Value {
    let mut tapped = common::act_signed(
        &format!("{base}/api/secretary/conversations/{id}/proposals/{index}/tap"),
        &serde_json::json!({}),
        &common::phone(),
    );
    assert_eq!(common::status(&tapped), 200);
    common::body_json(&mut tapped)
}

#[test]
fn a_held_file_reaches_only_the_wire_and_only_on_the_owners_tap() {
    let (fakes, running) = serving_files("held-file-send", proposing_the_file());
    let base = &running.base;
    let id = conversation(base, "send counsel my W-2");
    let proposed = settled(base, &id);
    let proposals = proposed["proposals"].as_array().expect("proposals");
    assert_eq!(
        proposals.len(),
        1,
        "the renamed, withheld and unread files are refused: {proposals:?}"
    );
    assert!(running.sent.sent().is_empty(), "proposing sends nothing");
    let card = proposals[0]["card"].to_string();
    assert!(card.contains("file w2.pdf, under 1 KB"), "{card}");

    assert_eq!(tap(base, &id, 0)["proposals"][0]["state"], "sent");
    let sent = running.sent.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].body,
        format!("Attached is w2.pdf; my SSN is {SSN}.")
    );
    let parts = running.sent.parts();
    assert_eq!(parts[0].len(), 1, "{:?}", parts[0].len());
    assert_eq!(
        (
            parts[0][0].filename.as_str(),
            parts[0][0].content.as_slice()
        ),
        ("w2.pdf", W2)
    );

    // Neither the file's bytes nor the number is in a prompt, a stored line, a kept answer or a
    // request record, and no file was handed to the model.
    let asked = format!("{:?}{:?}", fakes.model.asked(), fakes.model.resumed_with());
    for kept in [W2_BYTES, SSN] {
        assert!(!asked.contains(kept), "a prompt holds {kept}");
    }
    let port = running.port();
    for dir in [
        fakes.root.clone(),
        common::requests_dir(port),
        std::env::temp_dir().join(format!("suzatary-test-answers-{port}")),
    ] {
        for kept in [W2_BYTES, SSN] {
            assert!(!holds(&dir, kept), "{} holds {kept}", dir.display());
        }
    }
    let reading = settled(base, &id).to_string();
    assert!(
        !reading.contains(SSN) && !reading.contains(W2_BYTES),
        "{reading}"
    );
}
