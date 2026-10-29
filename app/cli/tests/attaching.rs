//! A proposed email carries the owner's own files, over a real server with a
//! scripted model and a fake mailbox: `propose_email` reads each source now and
//! records its name, type, size and SHA-256, and refuses a source that is
//! missing, outside the allowed Drive folders, or over the cap.

mod common;

use std::sync::Arc;

use common::{FakeMailbox, FakeModel, Fakes};
use suzatary_core::mail::{AttachmentFile, MailMessage, MailRow};
use suzatary_core::secretary::attachment_source::AttachmentSource;
use suzatary_core::secretary::store::Proposal;

const NOTICE: &[u8] = b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n%%EOF\n";

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment([ada()], [(ada(), common::actor("ada"))])
}

fn message(id: &str) -> MailMessage {
    MailMessage {
        row: MailRow {
            hash: "a1b2c3d4e5f6a1b2".to_owned(),
            id: id.to_owned(),
            sender: "notices@example.test".to_owned(),
            subject: "Approval notice".to_owned(),
            date: None,
            container: "inbox".to_owned(),
            account: "ada@example.com".to_owned(),
            disposition: suzatary_core::mail::Disposed::Undecided,
        },
        thread: "000000000000c3fb".to_owned(),
        headers: Vec::new(),
        body: "Your notice is attached.".to_owned(),
        truncated: false,
        links: Vec::new(),
    }
}

/// A mailbox holding one message with `notice.pdf`, found by its hash or its notmuch id.
fn mailbox() -> FakeMailbox {
    FakeMailbox::new()
        .with_message("a1b2c3d4e5f6a1b2", message("notice@example.test"))
        .with_message("notice@example.test", message("notice@example.test"))
        .with_attachment_file(
            "notice@example.test",
            AttachmentFile {
                name: "notice.pdf".to_owned(),
                mime: "application/pdf".to_owned(),
                content: NOTICE.to_vec(),
            },
        )
}

fn proposing(attach: &str) -> String {
    format!(
        r#"{{"say": "Drafted the reply with the notice.", "calls": [
            {{"verb": "propose_email", "args": {{
                "account": "ada@example.com", "to": "counsel@example.test",
                "subject": "The approval notice", "body": "The notice is attached.",
                "why": "Counsel asked for the approval notice.",
                "sources": "mail:a1b2c3d4e5f6", "attach": {attach}
            }}}}
        ]}}"#
    )
}

fn exchange(base: &str, said: &str) -> serde_json::Value {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "The notice"}),
    );
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({ "say": said }),
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

fn proposal_answers(payload: &serde_json::Value) -> Vec<(String, bool)> {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["who"] == "verb" && turn["verb"] == "propose_email")
        .map(|turn| {
            (
                turn["text"].as_str().unwrap_or_default().to_owned(),
                turn["outcome"]["is"] == "refused",
            )
        })
        .collect()
}

fn drafts(fakes: &Fakes) -> Vec<suzatary_core::secretary::store::Draft> {
    suzatary_core::secretary::inbox::fold(&fakes.inbox)
        .expect("the inbox reads")
        .objects()
        .filter_map(|object| match &object.asked.proposal {
            Proposal::Email(draft) => Some(draft.clone()),
            _ => None,
        })
        .collect()
}

fn scene(tag: &str, mailbox: FakeMailbox, attach: &str) -> (Arc<Fakes>, serde_json::Value) {
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .mailbox(mailbox)
            .model(FakeModel::saying_owned([
                proposing(attach),
                r#"{"say": "Proposed."}"#.to_owned(),
            ])),
    );
    let base = common::start(Arc::clone(&fakes), deployment(), Vec::new());
    let payload = exchange(&base, "reply to counsel with the approval notice");
    (fakes, payload)
}

/// A mail attachment is recorded by its message's notmuch id, with its digest:
/// the model named the message by its hash, and the record names it the way
/// that holds whether or not triage has seen it.
#[test]
fn a_mail_attachment_is_recorded_with_its_digest() {
    let (fakes, payload) = scene(
        "attaching-mail",
        mailbox(),
        r#"[{"mail": "a1b2c3d4e5f6a1b2", "name": "notice.pdf"}]"#,
    );
    let answered = proposal_answers(&payload);
    let (text, refused) = answered.first().expect("one proposal");
    assert!(!refused, "{text}");
    assert!(text.contains("notice.pdf"), "{text}");

    let drafts = drafts(&fakes);
    let [draft] = drafts.as_slice() else {
        panic!("one draft: {drafts:?}");
    };
    let [attached] = draft.attachments.as_slice() else {
        panic!("one attachment: {draft:?}");
    };
    assert_eq!(
        attached.source,
        AttachmentSource::Mail {
            message: "notice@example.test".to_owned(),
            name: "notice.pdf".to_owned(),
        }
    );
    assert_eq!(attached.name, "notice.pdf");
    assert_eq!(attached.media, "application/pdf");
    assert_eq!(attached.size, NOTICE.len() as u64);
    assert_eq!(
        attached.sha256,
        suzatary_core::attachment::AttachmentId::of(NOTICE)
    );
}

/// A source that is not there is refused in its own words, and nothing is proposed.
#[test]
fn a_missing_attachment_is_refused_by_name() {
    let (fakes, payload) = scene(
        "attaching-missing",
        mailbox(),
        r#"[{"mail": "notice@example.test", "name": "exhibit-b.pdf"}]"#,
    );
    let answered = proposal_answers(&payload);
    let (text, refused) = answered.first().expect("one answer");
    assert!(refused, "{text}");
    assert!(
        text.contains("attach[0]") && text.contains("exhibit-b.pdf"),
        "{text}"
    );
    assert!(drafts(&fakes).is_empty(), "nothing was proposed");
}

/// Nothing but the owner's own things: a URL is no source, and a Drive file
/// outside the allowed folders is refused before it is read.
#[test]
fn a_url_or_an_unallowed_drive_file_is_no_attachment() {
    for (tag, attach, said) in [
        (
            "attaching-url",
            r#"[{"url": "https://example.test/notice.pdf"}]"#,
            "names no single source",
        ),
        (
            "attaching-drive",
            r#"[{"drive": "/my-files/Private/passport.pdf"}]"#,
            "not in a folder the owner allowed",
        ),
    ] {
        let (fakes, payload) = scene(tag, mailbox(), attach);
        let answered = proposal_answers(&payload);
        let (text, refused) = answered.first().expect("one answer");
        assert!(refused, "{text}");
        assert!(text.contains(said), "{text}");
        assert!(drafts(&fakes).is_empty(), "nothing was proposed");
    }
}

/// Over 20 MB in all is refused, with the total named.
#[test]
fn attachments_over_the_cap_are_refused() {
    let big = vec![b'x'; 11 * 1024 * 1024];
    let mailbox = mailbox()
        .with_attachment_file(
            "notice@example.test",
            AttachmentFile {
                name: "scan-1.pdf".to_owned(),
                mime: "application/pdf".to_owned(),
                content: big.clone(),
            },
        )
        .with_attachment_file(
            "notice@example.test",
            AttachmentFile {
                name: "scan-2.pdf".to_owned(),
                mime: "application/pdf".to_owned(),
                content: big,
            },
        );
    let (fakes, payload) = scene(
        "attaching-cap",
        mailbox,
        r#"[{"mail": "notice@example.test", "name": "scan-1.pdf"},
            {"mail": "notice@example.test", "name": "scan-2.pdf"}]"#,
    );
    let answered = proposal_answers(&payload);
    let (text, refused) = answered.first().expect("one answer");
    assert!(refused, "{text}");
    assert!(text.contains("20 MB"), "{text}");
    assert!(drafts(&fakes).is_empty(), "nothing was proposed");
}

/// Propose with `notice.pdf` over a server whose sends are recorded, and answer the payload.
fn proposed(tag: &str) -> (Arc<Fakes>, common::Running, String) {
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .mailbox(mailbox())
            .model(FakeModel::saying_owned([
                proposing(r#"[{"mail": "notice@example.test", "name": "notice.pdf"}]"#),
                r#"{"say": "Proposed."}"#.to_owned(),
            ])),
    );
    let running = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new()).start();
    let payload = exchange(&running.base, "reply to counsel with the approval notice");
    let id = payload["id"].as_str().expect("an id").to_owned();
    (fakes, running, id)
}

fn tap(base: &str, id: &str) -> serde_json::Value {
    let mut tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    common::body_json(&mut tapped)
}

/// The tap sends the bytes the card showed: fetched again, checked against the
/// recorded digest, and handed to the transport with their name and type.
#[test]
fn a_tap_sends_the_attachment_it_checked() {
    let (_fakes, running, id) = proposed("attaching-tap");
    let after = tap(&running.base, &id);
    assert_eq!(after["proposals"][0]["state"], "sent", "{after}");

    let parts = running.sent.parts();
    let [sent] = parts.as_slice() else {
        panic!("one send: {parts:?}");
    };
    let [part] = sent.as_slice() else {
        panic!("one attachment: {sent:?}");
    };
    assert_eq!(part.filename, "notice.pdf");
    assert_eq!(part.mime_type, "application/pdf");
    assert_eq!(part.content, NOTICE);
}

/// A file that changed since the card was shown is not sent, and neither is
/// the message: the refusal names the file, and the transport is never asked.
#[test]
fn a_changed_attachment_refuses_the_send_with_nothing_sent() {
    let (fakes, running, id) = proposed("attaching-changed");
    fakes.mailbox.replace_attachment_file(
        "notice@example.test",
        AttachmentFile {
            name: "notice.pdf".to_owned(),
            mime: "application/pdf".to_owned(),
            content: b"%PDF-1.7\nsomething else\n%%EOF\n".to_vec(),
        },
    );
    let after = tap(&running.base, &id);
    let row = &after["proposals"][0];
    assert_eq!(row["state"], "failed", "{row}");
    let outcome = row["outcome"].as_str().unwrap_or_default();
    assert!(
        outcome.contains("notice.pdf changed since it was proposed"),
        "{outcome}"
    );
    assert!(outcome.contains("nothing was sent"), "{outcome}");
    assert!(
        running.sent.sent().is_empty(),
        "the transport was never asked"
    );
}

/// A source that is gone refuses the send the same way, naming what is missing.
#[test]
fn a_missing_attachment_refuses_the_send_with_nothing_sent() {
    let (fakes, running, id) = proposed("attaching-gone");
    fakes
        .mailbox
        .attachment_files
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
    let after = tap(&running.base, &id);
    let row = &after["proposals"][0];
    assert_eq!(row["state"], "failed", "{row}");
    let outcome = row["outcome"].as_str().unwrap_or_default();
    assert!(outcome.contains("notice.pdf on message"), "{outcome}");
    assert!(
        running.sent.sent().is_empty(),
        "the transport was never asked"
    );
}

/// A private file is attached unread and sent only on the tap: `propose_email` records it from
/// what the upload kept, the Drive untouched, and the tap fetches it there, checks its digest
/// and sends exactly those bytes.
#[test]
fn a_private_file_is_attached_unread_and_fetched_by_the_tap() {
    use suzatary_core::attachment::AttachmentId;
    let id = AttachmentId::of(NOTICE);
    let name = format!("{} w2.pdf", &id.as_str()[..8]);
    let drive = common::FakeDrive::new("attaching-private").holding_private(&name, NOTICE);
    let mut fakes = Fakes::new("attaching-private")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .drive(&drive)
        .model(FakeModel::saying_owned([
            proposing(&format!(r#"[{{"upload": "{id}"}}]"#)),
            r#"{"say": "Proposed."}"#.to_owned(),
        ]));
    fakes.files.uploads.insert(
        id.clone(),
        suzatary_core::upload::Upload::Private {
            media: suzatary_core::enclosed::Enclosure::Pdf,
            name: "w2.pdf".to_owned(),
            copy: suzatary_core::vault::PrivateCopy {
                path: suzatary_core::vault::folder().join(&name).expect("a path"),
                size: NOTICE.len() as u64,
                text: "Form W-2. SSN [redacted].".to_owned(),
            },
        },
    );
    let fakes = Arc::new(fakes);
    let running = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new()).start();
    let payload = exchange(&running.base, "send the W-2 to counsel");
    let [(_, false)] = proposal_answers(&payload).as_slice() else {
        panic!("one proposal: {payload}");
    };
    let [draft] = drafts(&fakes).try_into().expect("one draft");
    let [recorded] = draft.attachments.as_slice() else {
        panic!("one attachment: {draft:?}");
    };
    assert_eq!(recorded.name, "w2.pdf");
    assert_eq!(recorded.sha256, id);
    assert!(
        drive.calls().is_empty(),
        "the Drive was not read: {:?}",
        drive.calls()
    );

    let id = payload["id"].as_str().expect("an id").to_owned();
    let after = tap(&running.base, &id);
    assert_eq!(after["proposals"][0]["state"], "sent", "{after}");
    let parts = running.sent.parts();
    let [sent] = parts.as_slice() else {
        panic!("one send: {parts:?}");
    };
    let [part] = sent.as_slice() else {
        panic!("one attachment: {sent:?}");
    };
    assert_eq!(part.filename, "w2.pdf");
    assert_eq!(part.content, NOTICE);
    assert!(
        drive
            .calls()
            .iter()
            .any(|call| call.starts_with("filesystem download /my-files/Suzatary/Private/")),
        "{:?}",
        drive.calls()
    );
}

/// A file read into the conversation attaches from the copy kept then, and only a file this
/// conversation holds: the owner's Allow once (here, a verb's read of a mail attachment, which
/// keeps its copy the same way) is what lets it go on an email, and the send tap sends exactly
/// those bytes.
#[test]
fn a_file_read_here_attaches_from_its_kept_copy_and_nothing_else_does() {
    let id = suzatary_core::attachment::AttachmentId::of(NOTICE);
    let never_read = suzatary_core::attachment::AttachmentId::of(b"never read here");
    let fakes = Arc::new(
        Fakes::new("attaching-kept")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .mailbox(mailbox().with_attachment_read(
                "notice@example.test",
                "notice.pdf",
                suzatary_core::file_read::FileRead::of(
                    &suzatary_core::held_value::HeldFile::Mail {
                        message: "notice@example.test".to_owned(),
                        name: "notice.pdf".to_owned(),
                    },
                    "notice.pdf",
                    "application/pdf",
                    NOTICE.to_vec(),
                    Some(""),
                ),
            ))
            .model(FakeModel::saying_owned([
                r#"{"say": "Reading the notice.", "calls": [
                    {"verb": "attachments", "args": {"id": "notice@example.test", "name": "notice.pdf"}}
                ]}"#
                .to_owned(),
                proposing(&format!(r#"[{{"kept": "{never_read}"}}]"#)),
                proposing(&format!(r#"[{{"kept": "{id}"}}]"#)),
                r#"{"say": "Proposed."}"#.to_owned(),
            ])),
    );
    let running = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new()).start();
    let payload = exchange(&running.base, "send counsel the notice");
    let answers = proposal_answers(&payload);
    let [(refusal, true), (_, false)] = answers.as_slice() else {
        panic!("one refused, one proposed: {answers:?}");
    };
    assert!(
        refusal.contains("was read into this conversation"),
        "{refusal}"
    );
    let [draft] = drafts(&fakes).try_into().expect("one draft");
    let [recorded] = draft.attachments.as_slice() else {
        panic!("one attachment: {draft:?}");
    };
    assert_eq!(recorded.name, "notice.pdf");
    assert_eq!(recorded.source, AttachmentSource::Kept { id: id.clone() });
    assert_eq!(recorded.sha256, id);

    let conversation = payload["id"].as_str().expect("an id").to_owned();
    let mut tapped = common::act(
        &format!(
            "{}/api/secretary/conversations/{conversation}/proposals/0/tap",
            running.base
        ),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    let after = common::body_json(&mut tapped);
    assert_eq!(after["proposals"][0]["state"], "sent", "{after}");
    let parts = running.sent.parts();
    let [sent] = parts.as_slice() else {
        panic!("one send: {parts:?}");
    };
    let [part] = sent.as_slice() else {
        panic!("one attachment: {sent:?}");
    };
    assert_eq!(part.filename, "notice.pdf");
    assert_eq!(part.content, NOTICE);
}
