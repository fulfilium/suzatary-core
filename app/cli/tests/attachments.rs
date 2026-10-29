//! The secretary reads attachments over a real server with a scripted model and a fake mailbox:
//! the `attachments` verb answers what it asked for, with no tap needed for a read.

mod common;

use std::sync::Arc;

use common::{FakeMailbox, FakeModel, Fakes};
use suzatary_core::capability::Turn;
use suzatary_core::enclosed::Enclosure;
use suzatary_core::file_read::FileRead;
use suzatary_core::mail::AttachmentMeta;

const PDF: &[u8] = b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n%%EOF\n";

/// A mailbox holding message `id` under its own notmuch id, which `attachments` names a file by.
fn holding(id: &str) -> FakeMailbox {
    FakeMailbox::new().with_message(
        id,
        suzatary_core::mail::MailMessage {
            row: suzatary_core::mail::MailRow {
                hash: "a1b2c3d4e5f6a1b2".to_owned(),
                id: id.to_owned(),
                sender: "hr@example.test".to_owned(),
                subject: "Attached".to_owned(),
                date: None,
                container: "inbox".to_owned(),
                account: "ada@example.com".to_owned(),
                disposition: suzatary_core::mail::Disposed::Undecided,
            },
            thread: "000000000000c3fd".to_owned(),
            headers: Vec::new(),
            body: "See the attachment.".to_owned(),
            truncated: false,
            links: Vec::new(),
        },
    )
}

/// The source of attachment `name` on message `id`, as `attachments` names it.
fn mail(id: &str, name: &str) -> suzatary_core::held_value::HeldFile {
    suzatary_core::held_value::HeldFile::Mail {
        message: id.to_owned(),
        name: name.to_owned(),
    }
}

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment([ada()], [(ada(), common::actor("ada"))])
}

fn exchange(base: &str, said: &str) -> serde_json::Value {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "The offer letter"}),
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

/// Every `verb`-turn answer, in the order appended; two calls share a name, so the caller
/// picks its turn by position.
fn answers(payload: &serde_json::Value, verb: &str) -> Vec<(String, bool)> {
    payload["turns"]
        .as_array()
        .expect("turns")
        .iter()
        .filter(|turn| turn["who"] == "verb" && turn["verb"] == verb)
        .map(|turn| {
            (
                turn["text"].as_str().unwrap_or_default().to_owned(),
                turn["outcome"]["is"] == "refused",
            )
        })
        .collect()
}

/// Every file a turn the model was handed carries, across whole conversations and resumes.
fn handed(model: &FakeModel) -> Vec<suzatary_core::enclosed::Enclosed> {
    model
        .asked()
        .into_iter()
        .map(|conversation| conversation.turns)
        .chain(model.resumed_with())
        .flatten()
        .filter_map(|turn| match turn {
            Turn::Tool { files, .. } => Some(files),
            _ => None,
        })
        .flatten()
        .collect()
}

/// Listing names every attachment and reads none; naming one hands the model the file itself:
/// the PDF reaches it as a document, kept in the box's file store, with its text layer beside
/// it for a model that cannot read one.
#[test]
fn attachments_lists_then_reads_a_named_one() {
    let mailbox = holding("offer-msg")
        .with_attachments(
            "offer-msg",
            vec![
                AttachmentMeta {
                    name: "notes.txt".to_owned(),
                    mime: "text/plain".to_owned(),
                    size: 22,
                },
                AttachmentMeta {
                    name: "offer.pdf".to_owned(),
                    mime: "application/pdf".to_owned(),
                    size: 12_345,
                },
            ],
        )
        .with_attachment_read(
            "offer-msg",
            "offer.pdf",
            FileRead::of(
                &mail("offer-msg", "offer.pdf"),
                "offer.pdf",
                "application/pdf",
                PDF.to_vec(),
                Some("Base salary: USD 70,000 per year.\u{c}"),
            ),
        );
    let fakes = Fakes::new("attachments-read")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .mailbox(mailbox)
        .model(FakeModel::saying([
            r#"{"say": "Looking at the offer letter.", "calls": [
                {"verb": "attachments", "args": {"id": "offer-msg"}}
            ]}"#,
            r#"{"say": "Reading the PDF.", "calls": [
                {"verb": "attachments", "args": {"id": "offer-msg", "name": "offer.pdf"}}
            ]}"#,
            r#"{"say": "The base salary is USD 70,000."}"#,
        ]));
    let fakes = Arc::new(fakes);
    let base = common::start(Arc::clone(&fakes), deployment(), Vec::new());
    let payload = exchange(
        &base,
        "what does the offer letter attachment say about salary",
    );

    let calls = answers(&payload, "attachments");
    assert_eq!(calls.len(), 2, "{payload}");

    let (listed, refused) = &calls[0];
    assert!(!refused, "{listed}");
    assert!(
        listed.contains("notes.txt") && listed.contains("text/plain"),
        "{listed}"
    );
    assert!(
        listed.contains("offer.pdf") && listed.contains("application/pdf"),
        "{listed}"
    );
    assert!(listed.contains("12345"), "{listed}");
    // Listing reads no text — the size is a number, not the extracted salary.
    assert!(!listed.contains("70,000"), "{listed}");

    let (read, refused) = &calls[1];
    assert!(!refused, "{read}");
    assert!(read.contains("the file itself is attached"), "{read}");
    assert!(read.contains("application/pdf"), "{read}");

    let [file] = handed(&fakes.model)
        .try_into()
        .expect("one file was handed over");
    assert_eq!(file.name, "offer.pdf");
    assert_eq!(file.media, Enclosure::Pdf);
    assert_eq!(file.id, suzatary_core::attachment::AttachmentId::of(PDF));
    assert!(
        file.text.contains("70,000"),
        "the text layer rides beside it"
    );
    assert_eq!(
        *fakes.files.kept.lock().expect("not poisoned"),
        vec![PDF.to_vec()],
        "the file store holds the bytes the model reads"
    );

    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "reading an attachment asks nothing"
    );
}

/// An image reaches the model as an image; anything else that is not text is
/// refused by name, never opened.
#[test]
fn an_image_is_handed_over_and_an_unsupported_attachment_is_refused_naming_the_type() {
    let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
    let mailbox = holding("photo-msg")
        .with_attachments(
            "photo-msg",
            vec![AttachmentMeta {
                name: "scan.jpg".to_owned(),
                mime: "image/jpeg".to_owned(),
                size: 6,
            }],
        )
        .with_attachment_read(
            "photo-msg",
            "scan.jpg",
            FileRead::of(
                &mail("photo-msg", "scan.jpg"),
                "scan.jpg",
                "image/jpeg",
                jpeg.clone(),
                None,
            ),
        )
        .with_attachment_read(
            "photo-msg",
            "form.docx",
            FileRead::of(
                &mail("photo-msg", "form.docx"),
                "form.docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                b"PK\x03\x04".to_vec(),
                None,
            ),
        );
    let fakes = Fakes::new("attachments-unsupported")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .mailbox(mailbox)
        .model(FakeModel::saying([
            r#"{"say": "Checking.", "calls": [
                {"verb": "attachments", "args": {"id": "photo-msg", "name": "scan.jpg"}},
                {"verb": "attachments", "args": {"id": "photo-msg", "name": "form.docx"}}
            ]}"#,
            r#"{"say": "The scan shows a receipt; the form I cannot read."}"#,
        ]));
    let fakes = Arc::new(fakes);
    let base = common::start(Arc::clone(&fakes), deployment(), Vec::new());
    let payload = exchange(&base, "what does the attached scan say");

    let calls = answers(&payload, "attachments");
    assert_eq!(calls.len(), 2, "{payload}");
    let (image, refused) = &calls[0];
    assert!(!refused, "{image}");
    assert!(image.contains("as an image"), "{image}");
    let [file] = handed(&fakes.model)
        .try_into()
        .expect("one file was handed over");
    assert_eq!(
        file.media,
        Enclosure::Image(suzatary_core::attachment::MediaType::Jpeg)
    );
    let (docx, refused) = &calls[1];
    assert!(
        !refused,
        "a type refusal is an answer, not a tool refusal: {docx}"
    );
    assert!(docx.contains("nothing a model reads"), "{docx}");
}

/// An SSN in an attachment arrives `[redacted]`: a PDF whose text layer names one is not
/// handed over at all, and its text passes through the same `redact` a Drive document does
/// before the model or the transcript sees it.
#[test]
fn an_ssn_in_an_attachment_arrives_as_placeholders_naming_the_file() {
    let mailbox = holding("w2-msg")
        .with_attachments(
            "w2-msg",
            vec![AttachmentMeta {
                name: "w2.pdf".to_owned(),
                mime: "application/pdf".to_owned(),
                size: 9_000,
            }],
        )
        .with_attachment_read(
            "w2-msg",
            "w2.pdf",
            FileRead::of(
                &mail("w2-msg", "w2.pdf"),
                "w2.pdf",
                "application/pdf",
                PDF.to_vec(),
                Some("Employee SSN: 123-45-6789. Wages: 70,000. Receipt IOE0912345678.\u{c}"),
            ),
        );
    let fakes = Fakes::new("attachments-redacted")
        .whois(common::FakeWhois::new().naming(common::v4(), ada()))
        .mailbox(mailbox)
        .model(FakeModel::saying([
            r#"{"say": "Reading the W-2.", "calls": [
                {"verb": "attachments", "args": {"id": "w2-msg", "name": "w2.pdf"}}
            ]}"#,
            r#"{"say": "Wages were 70,000."}"#,
        ]));
    let fakes = Arc::new(fakes);
    let base = common::start(Arc::clone(&fakes), deployment(), Vec::new());
    let payload = exchange(&base, "what were my wages on the W-2");

    let calls = answers(&payload, "attachments");
    let (read, refused) = calls.first().expect("one read");
    assert!(!refused, "{read}");
    assert!(
        read.contains("SSN: [SSN ending 6789 · tap to show · held:file:"),
        "{read}"
    );
    assert!(
        read.contains("[file w2.pdf, under 1 KB · tap to show · held:file:"),
        "the file's own placeholder: {read}"
    );
    assert!(
        read.contains("70,000") && read.contains("IOE0912345678"),
        "{read}"
    );
    assert!(
        read.contains("identity number"),
        "the file was withheld: {read}"
    );
    assert!(!payload.to_string().contains("123-45-6789"), "{payload}");
    assert!(handed(&fakes.model).is_empty(), "no file was handed over");
}
