//! Forwarding a stored message over a real server with a scripted model: the model and every
//! store see the original's held value only as its placeholder, and the owner's tap sends the
//! original as the message holds it, or nothing when its file changed.

mod common;

use std::sync::Arc;

use common::{FakeMailbox, FakeModel, Fakes};
use suzatary_core::capability::MailError;
use suzatary_core::held_value::{Handle, Reopened, Reveal, Shown};
use suzatary_core::mail::{MailMessage, MailRow};
use suzatary_core::secretary::store::Style;

/// A synthetic SSN no person holds, as the form writes it and as its copy does.
const SSN: &str = "000-12-3456";
const SSN_DIGITS: &str = "000123456";

const ID: &str = "form-1@example.test";

/// Two attachments of one name, told apart by their bytes ("first" and "second", base64).
const FIRST: &[u8] = b"first";
const SECOND: &[u8] = b"second";

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment([ada()], [(ada(), common::actor("ada"))])
}

/// The message file, which holds the number in both spellings and two attachments of one name.
fn raw() -> Vec<u8> {
    format!(
        "From: payroll@example.test\r\nTo: ada@example.com\r\nSubject: Your W-2\r\n\
         Message-ID: <{ID}>\r\nMIME-Version: 1.0\r\n\
         Content-Type: multipart/mixed; boundary=XX\r\n\r\n\
         --XX\r\nContent-Type: text/plain\r\n\r\n{}\r\n\
         --XX\r\nContent-Type: image/png\r\nContent-Disposition: attachment; filename=image.png\r\n\
         Content-Transfer-Encoding: base64\r\n\r\nZmlyc3Q=\r\n\
         --XX\r\nContent-Type: image/png\r\nContent-Disposition: attachment; filename=image.png\r\n\
         Content-Transfer-Encoding: base64\r\n\r\nc2Vjb25k\r\n\
         --XX--\r\n",
        body()
    )
    .into_bytes()
}

/// The text, unmasked.
fn body() -> String {
    format!("Your SSN on file is {SSN}; the form's copy reads SSN {SSN_DIGITS}.")
}

/// The message as the mailbox answers a model: the number is its placeholder.
fn masked() -> MailMessage {
    let placeholder = format!(
        "[SSN ending 3456 · tap to show · {}]",
        Handle::new(ID, 0).expect("a handle").print()
    );
    MailMessage {
        row: MailRow {
            hash: "b1b2c3d4e5f6a1b2".to_owned(),
            id: ID.to_owned(),
            sender: "payroll@example.test".to_owned(),
            subject: "Your W-2".to_owned(),
            date: None,
            container: "inbox".to_owned(),
            account: "ada@example.com".to_owned(),
            disposition: suzatary_core::mail::Disposed::Undecided,
        },
        thread: "000000000000c3fc".to_owned(),
        headers: Vec::new(),
        body: format!(
            "Your SSN on file is {placeholder}; the form's copy reads SSN {placeholder}."
        ),
        truncated: false,
        links: Vec::new(),
    }
}

fn mailbox() -> FakeMailbox {
    FakeMailbox::new()
        .with_message(ID, masked())
        .with_message_file(ID, raw())
}

/// The tap's read of the original: the file as the box holds it then, read as the box reads it.
struct Originals(Vec<u8>);

impl Reveal for Originals {
    fn value(&self, _: &str, _: u16) -> Result<Option<Shown>, MailError> {
        Ok(None)
    }

    fn original(&self, message: &str, size: u64) -> Result<Reopened, MailError> {
        Ok(if message != ID {
            Reopened::Gone
        } else if self.0.len() as u64 != size {
            Reopened::Resized
        } else {
            Reopened::Read(suzatary_io::mailbox::forwarded::original(self.0.clone()))
        })
    }
}

/// A forward proposed as `style`, over a server whose tap reads `originals`; the
/// conversation's id and the verb's answer to the model.
fn proposed(
    tag: &str,
    style: Style,
    originals: Vec<u8>,
) -> (Arc<Fakes>, common::Running, String, String) {
    let (fakes, running, payload) = scene(tag, style, originals, SHOW);
    let id = payload["id"].as_str().expect("an id").to_owned();
    let answered = forward_turn(&payload);
    assert_eq!(answered["outcome"]["is"], "answered", "{answered}");
    let answered = answered["text"].as_str().unwrap_or_default().to_owned();
    (fakes, running, id, answered)
}

/// The call that reads the message into the conversation before it is forwarded.
const SHOW: &str = r#"{"verb": "show", "args": {"id": "form-1@example.test"}},"#;

/// The verb's turn answering the forward.
fn forward_turn(payload: &serde_json::Value) -> serde_json::Value {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .find(|turn| turn["who"] == "verb" && turn["verb"] == "propose_forward")
        .cloned()
        .expect("one forward asked for")
}

/// A server whose model calls `first` and then forwards the message as `style`, its tap reading
/// `originals`; the conversation once the round is done.
fn scene(
    tag: &str,
    style: Style,
    originals: Vec<u8>,
    first: &str,
) -> (Arc<Fakes>, common::Running, serde_json::Value) {
    let as_ = style.as_str();
    let proposing = format!(
        r#"{{"say": "Drafted the forward.", "calls": [
            {first}
            {{"verb": "propose_forward", "args": {{
                "account": "ada@example.com", "to": "counsel@example.test",
                "subject": "Fwd: Your W-2", "message": "{ID}", "as": "{as_}",
                "note": "The W-2 you asked for.",
                "why": "Counsel asked for the W-2.", "sources": "mail:b1b2c3d4e5f6"
            }}}}
        ]}}"#
    );
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .mailbox(mailbox())
            .model(FakeModel::saying_owned([
                proposing,
                r#"{"say": "Proposed."}"#.to_owned(),
            ])),
    );
    let running = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .reveal(Arc::new(Originals(originals)))
        .start();
    let payload = exchange(&running.base);
    (fakes, running, payload)
}

fn exchange(base: &str) -> serde_json::Value {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "The W-2"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({ "say": "forward my W-2 to counsel" }),
    );
    assert_eq!(common::status(&said), 202);
    for _ in 0..200 {
        let payload = common::reading(base, &id, &[]);
        if payload["state"] == "idle" {
            return payload;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the exchange never finished");
}

fn tap(base: &str, id: &str) -> serde_json::Value {
    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    common::body_json(&mut tapped)
}

/// Whether any file under `dir` holds `text`.
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

/// The model, the card and every store hold the placeholder; the tap's send holds the number.
#[test]
fn a_forward_carries_the_held_value_only_in_what_the_tap_sends() {
    for style in [Style::Inline, Style::Attached] {
        let (fakes, running, id, answered) =
            proposed(&format!("forwarding-{}", style.as_str()), style, raw());
        assert!(
            !answered.contains(SSN) && !answered.contains(SSN_DIGITS),
            "{answered}"
        );
        assert!(answered.contains("SSN ending 3456"), "{answered}");
        let card = common::reading(&running.base, &id, &[]);
        let row = &card["proposals"][0];
        assert!(
            !row.to_string().contains(SSN) && !row.to_string().contains(SSN_DIGITS),
            "{row}"
        );
        assert!(
            row["card"]["answers"][0]["question"]
                .as_str()
                .is_some_and(|asked| asked.contains("counsel@example.test")
                    && asked.contains("SSN ending 3456")),
            "{row}"
        );

        let after = tap(&running.base, &id);
        assert_eq!(after["proposals"][0]["state"], "sent", "{after}");
        let sent = running.sent.sent();
        let parts = running.sent.parts();
        let ([draft], [files]) = (sent.as_slice(), parts.as_slice()) else {
            panic!("one send: {sent:?}");
        };
        assert_eq!(draft.to, ["counsel@example.test"]);
        assert_eq!(draft.subject, "Fwd: Your W-2");
        let names: Vec<&str> = files.iter().map(|part| part.filename.as_str()).collect();
        match style {
            Style::Inline => {
                assert!(
                    draft.body.starts_with("The W-2 you asked for.\n\n"),
                    "{}",
                    draft.body
                );
                assert!(draft.body.contains(&body()), "{}", draft.body);
                assert!(
                    draft.body.contains("From: payroll@example.test"),
                    "{}",
                    draft.body
                );
                assert_eq!(names, ["image.png", "image.png"]);
                assert_eq!(files[0].content, FIRST);
                assert_eq!(files[1].content, SECOND);
            }
            Style::Attached => {
                assert_eq!(draft.body, "The W-2 you asked for.");
                assert_eq!(names, ["forwarded.eml"]);
                assert_eq!(files[0].mime_type, "message/rfc822");
                assert_eq!(files[0].content, raw());
            }
        }

        for dir in [fakes.root.clone(), common::requests_dir(running.port())] {
            for spelled in [SSN, SSN_DIGITS] {
                assert!(!holds(&dir, spelled), "{} holds the value", dir.display());
            }
        }
    }

    read_binding_holds();
}

/// Only a message the conversation read leaves: the verb refuses one it never read, and a tap
/// after the reading turn is erased sends nothing.
fn read_binding_holds() {
    let (fakes, _, payload) = scene("forwarding-unread", Style::Attached, raw(), "");
    let refused = forward_turn(&payload);
    assert_eq!(refused["outcome"]["is"], "refused", "{refused}");
    assert!(
        refused["text"]
            .as_str()
            .is_some_and(|said| said.contains("only a message this conversation read")),
        "{refused}"
    );
    assert_eq!(
        suzatary_core::secretary::inbox::fold(&fakes.inbox)
            .expect("the inbox reads")
            .objects()
            .count(),
        0,
        "nothing was proposed"
    );
    let (_, running, payload) = scene("forwarding-erased", Style::Attached, raw(), SHOW);
    let id = payload["id"].as_str().expect("an id");
    let shown = payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .position(|turn| turn["who"] == "verb" && turn["verb"] == "show")
        .expect("the message was shown");
    let erased = common::act(
        &format!("{}/api/secretary/conversations/{id}/erase", running.base),
        &serde_json::json!({ "turns": [shown] }),
    );
    assert_eq!(common::status(&erased), 200);
    let after = tap(&running.base, id);
    let row = &after["proposals"][0];
    assert_eq!(row["state"], "failed", "{row}");
    assert!(
        row["outcome"]
            .as_str()
            .is_some_and(|said| said.contains("no mail this conversation read")),
        "{row}"
    );
    assert!(
        running.sent.sent().is_empty(),
        "the transport was never asked"
    );
}

/// A message whose file is not the one proposed, whether or not its size moved, is not
/// forwarded in either form, and the refusal says to ask again.
#[test]
fn a_changed_original_is_not_forwarded() {
    let resized = [raw(), b"A line added since.\r\n".to_vec()].concat();
    let rewritten = String::from_utf8(raw())
        .expect("text")
        .replace("Your W-2", "Your W-3")
        .into_bytes();
    for (case, changed) in [("resized", resized), ("rewritten", rewritten)] {
        for style in [Style::Inline, Style::Attached] {
            let (_fakes, running, id, _) = proposed(
                &format!("forwarding-{case}-{}", style.as_str()),
                style,
                changed.clone(),
            );
            let after = tap(&running.base, &id);
            let row = &after["proposals"][0];
            assert_eq!(row["state"], "failed", "{row}");
            let outcome = row["outcome"].as_str().unwrap_or_default();
            assert!(
                outcome.contains("the message changed since the forward was proposed")
                    && outcome.contains("ask the secretary for a new forward")
                    && !outcome.contains("forwarded.eml"),
                "{outcome}"
            );
            assert!(
                running.sent.sent().is_empty(),
                "the transport was never asked"
            );
        }
    }
}
