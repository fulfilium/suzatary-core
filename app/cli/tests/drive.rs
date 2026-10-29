//! The secretary reads the owner's documents by permission, over a real server with a scripted
//! model and the real Drive interpreter pointed at a fake CLI ([`common::FakeDrive`]) that fails
//! on any command but the two reads: a file in an allowed folder is read at once, any other
//! becomes a card naming the file, and every SSN or EIN arrives as a placeholder naming the file.

mod common;

use std::sync::Arc;

use common::{FakeDrive, FakeModel, Fakes};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

fn deployment() -> suzatary_core::deployment::Deployment {
    common::deployment([ada()], [(ada(), common::actor("ada"))])
}

fn read_call(path: &str) -> String {
    format!(
        r#"{{"say": "Let me read it.",
            "calls": [{{"verb": "drive_read", "args": {{
              "path": "{path}",
              "why": "Ada asked what the document says.",
              "sources": "todo:life/permitfiling"
            }}}}]}}"#
    )
}

/// Open a conversation, say something, and wait until nobody is answering.
fn exchange(base: &str, said: &str) -> (String, serde_json::Value) {
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "A document"}),
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
    let payload = settled(base, &id);
    (id, payload)
}

fn settled(base: &str, id: &str) -> serde_json::Value {
    for _ in 0..200 {
        let payload = common::reading(base, id, &[]);
        if payload["state"] == "idle" {
            return payload;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the exchange never finished");
}

/// Every answer of `verb`, with whether it was refused.
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

/// Every byte the conversation store holds, for "never written" checks.
fn stored(fakes: &Fakes) -> String {
    let mut all = String::new();
    for dir in [
        fakes.root.join("secretary"),
        fakes.root.join("secretary-lines/objects"),
    ] {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            if entry.path().is_file() {
                all.push_str(&std::fs::read_to_string(entry.path()).unwrap_or_default());
            }
        }
    }
    // And every line as read with its keys, so a word the door let through would be found.
    for (_, transcript) in
        suzatary_core::secretary::transcript::all(fakes.conversations.conversations())
            .expect("the conversations read")
    {
        all.push_str(&serde_json::to_string(&transcript.entries()).expect("prints"));
    }
    all
}

#[test]
fn a_folder_is_listed_with_no_tap() {
    let drive = FakeDrive::new("drive-list");
    let fakes = Arc::new(
        Fakes::new("drive-list")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .model(FakeModel::saying([
                r#"{"say": "Looking.", "calls": [
                    {"verb": "drive_list", "args": {"path": "/my-files/Docs"}}
                ]}"#,
                r#"{"say": "A letter and a folder."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "what is in my Docs folder");

    let listed = answers(&payload, "drive_list");
    assert_eq!(listed.len(), 1, "{payload}");
    let (text, refused) = &listed[0];
    assert!(!refused, "{text}");
    assert!(text.contains("/my-files/Docs/letter.txt"), "{text}");
    assert!(text.contains("/my-files/Docs/Tax") && text.contains("folder"));
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty()
    );
    assert_eq!(drive.calls(), ["filesystem list -j /my-files/Docs"]);
    drive.only_reads_ran();
}

/// The owner's private folder is never listed, even with every folder allowed: the fence refuses
/// before the Drive is asked.
#[test]
fn the_private_folder_is_never_listed() {
    let drive = FakeDrive::new("drive-private");
    let fakes = Arc::new(
        Fakes::new("drive-private")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .allowing_folder("/my-files")
            .model(FakeModel::saying([
                r#"{"say": "Looking.", "calls": [
                    {"verb": "drive_list", "args": {"path": "/my-files/Suzatary/Private"}}
                ]}"#,
                r#"{"say": "That folder is his alone."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "what is in the private folder");

    let listed = answers(&payload, "drive_list");
    let (text, refused) = listed.first().expect("one answer");
    assert!(refused, "{text}");
    assert!(text.contains("never listed"), "{text}");
    assert!(drive.calls().is_empty(), "{:?}", drive.calls());
}

#[test]
fn a_file_in_an_allowed_folder_is_read_at_once_and_asks_nothing() {
    let drive = FakeDrive::new("drive-allowed");
    let fakes = Arc::new(
        Fakes::new("drive-allowed")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .allowing_folder("/my-files/Docs")
            .model(FakeModel::saying([
                read_call("/my-files/Docs/letter.txt").leak(),
                r#"{"say": "The lease renews on 1 October."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "when does the lease renew");

    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty(),
        "an allowed read asks nothing: {payload}"
    );
    let read = answers(&payload, "drive_read");
    assert!(
        read.iter()
            .any(|(text, refused)| !refused && text.contains("The lease renews on 1 October")),
        "the text is the verb's answer: {read:?}"
    );
    assert!(
        drive
            .calls()
            .iter()
            .any(|call| call.starts_with("filesystem download /my-files/Docs/letter.txt ")),
        "{:?}",
        drive.calls()
    );
    drive.only_reads_ran();
}

/// An SSN and an EIN arrive as placeholders naming the file, in the answer the model reads and
/// in every byte the conversation store keeps; the phone number stays.
#[test]
fn an_ssn_in_a_document_arrives_masked_and_is_never_stored() {
    let drive = FakeDrive::new("drive-redacted");
    let fakes = Arc::new(
        Fakes::new("drive-redacted")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .allowing_folder("/shared-with-me/Ada Documents")
            .model(FakeModel::saying([
                read_call("/shared-with-me/Ada Documents/w2.txt").leak(),
                r#"{"say": "Wages were 95000; the SSN is in w2.txt."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "what were my wages");

    let read = answers(&payload, "drive_read");
    let (text, refused) = read.first().expect("one read");
    assert!(!refused, "{text}");
    assert!(text.contains("Wages 95000"), "{text}");
    assert!(
        text.contains("SSN [SSN ending 6789 · tap to show · held:file:"),
        "{text}"
    );
    assert!(
        text.contains("EIN [EIN ending 0001 · tap to show · held:file:"),
        "{text}"
    );
    assert!(text.contains("617-555-1234"), "a phone number is not an ID");
    let kept = stored(&fakes);
    assert!(!kept.is_empty(), "the conversation was stored");
    for number in ["123-45-6789", "00-0000001"] {
        assert!(
            !payload.to_string().contains(number),
            "{number} reached the wire"
        );
        assert!(!kept.contains(number), "{number} reached the transcript");
    }
    drive.only_reads_ran();
}

/// Any other folder is a card: the file named, its folder and size, one
/// listing to check it exists, and nothing downloaded.
#[test]
fn a_file_elsewhere_becomes_a_document_request_card() {
    let drive = FakeDrive::new("drive-asked");
    let fakes = Arc::new(
        Fakes::new("drive-asked")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .allowing_folder("/my-files/Docs")
            .model(FakeModel::saying([
                read_call("/shared-with-me/Ada Documents/w2.txt").leak(),
                r#"{"say": "I asked; tap Read if you want me to read it."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "what were my wages");

    let proposals = payload["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 1, "{payload}");
    let act = &proposals[0]["act"];
    assert_eq!(act["kind"], "drive");
    assert_eq!(act["path"], "/shared-with-me/Ada Documents/w2.txt");
    assert_eq!(act["name"], "w2.txt");
    assert_eq!(act["folder"], "/shared-with-me/Ada Documents");
    assert_eq!(act["size"], "48.1 KB");
    let card = &proposals[0]["card"];
    assert_eq!(card["title"], "Document request");
    assert_eq!(card["answers"][0]["label"], "Allow once");
    assert_eq!(
        drive.calls(),
        ["filesystem list -j /shared-with-me/Ada Documents"],
        "listed to check it, never downloaded"
    );
    drive.only_reads_ran();
}

/// Inside the private folder nothing is listed: any name answers with the same card, there or
/// not, so no answer tells the model which private files exist, and his tap is the only read.
#[test]
fn a_private_file_is_asked_for_without_a_listing_whatever_its_name() {
    let drive = FakeDrive::new("drive-private-read");
    let fakes = Arc::new(
        Fakes::new("drive-private-read")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .allowing_folder("/my-files")
            .model(FakeModel::saying([
                r#"{"say": "Asking for two.", "calls": [
                    {"verb": "drive_read", "args": {"path": "/my-files/Suzatary/Private/1a2b w2.pdf",
                      "why": "w", "sources": "todo:life/permitfiling"}},
                    {"verb": "drive_read", "args": {"path": "/my-files/Suzatary/Private/nobody.pdf",
                      "why": "w", "sources": "todo:life/permitfiling"}}
                ]}"#,
                r#"{"say": "Both asked."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "read my private files again");

    let read = answers(&payload, "drive_read");
    assert_eq!(read.len(), 2, "{payload}");
    let shapes: Vec<String> = read
        .iter()
        .map(|(text, refused)| {
            assert!(!refused, "{text}");
            // The name and the card's number aside, the answers are the same text.
            text.replace("1a2b w2.pdf", "NAME")
                .replace("nobody.pdf", "NAME")
                .chars()
                .map(|c| if c.is_ascii_digit() { '#' } else { c })
                .collect()
        })
        .collect();
    assert_eq!(
        shapes[0], shapes[1],
        "a name there and a name not answer alike"
    );
    let proposals = payload["proposals"].as_array().expect("proposals");
    assert_eq!(proposals.len(), 2, "{payload}");
    assert!(
        drive.calls().is_empty(),
        "nothing listed: {:?}",
        drive.calls()
    );
}

#[test]
fn a_folder_a_missing_file_and_a_relative_path_are_refused_before_asking() {
    let drive = FakeDrive::new("drive-refused");
    let fakes = Arc::new(
        Fakes::new("drive-refused")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .model(FakeModel::saying([
                r#"{"say": "Trying three.", "calls": [
                    {"verb": "drive_read", "args": {"path": "/my-files/Docs/Tax",
                      "why": "w", "sources": "todo:life/permitfiling"}},
                    {"verb": "drive_read", "args": {"path": "/my-files/Docs/absent.pdf",
                      "why": "w", "sources": "todo:life/permitfiling"}},
                    {"verb": "drive_read", "args": {"path": "my-files/../trash/x.pdf",
                      "why": "w", "sources": "todo:life/permitfiling"}}
                ]}"#,
                r#"{"say": "None of those can be read."}"#,
            ])),
    );
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .start()
        .base;
    let (_, payload) = exchange(&base, "read these");

    let read = answers(&payload, "drive_read");
    assert_eq!(read.len(), 3, "{payload}");
    assert!(read.iter().all(|(_, refused)| *refused), "{read:?}");
    assert!(
        payload["proposals"]
            .as_array()
            .expect("proposals")
            .is_empty()
    );
    assert!(!drive.calls().iter().any(|call| call.contains("download")));
    drive.only_reads_ran();
}

/// A scene where the secretary asks for the W-2 and the owner's tap answers,
/// through the CLI performer's fixture arm over the same fake Drive.
#[cfg(feature = "fixture")]
struct Tapped {
    drive: FakeDrive,
    edits: Arc<common::EditsRecorder>,
    status: u16,
    payload: serde_json::Value,
    fakes: Arc<Fakes>,
}

#[cfg(feature = "fixture")]
fn tapped(tag: &str, tap: &serde_json::Value) -> Tapped {
    let drive = FakeDrive::new(tag);
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .model(FakeModel::saying([
                read_call("/shared-with-me/Ada Documents/w2.txt").leak(),
                r#"{"say": "Asked."}"#,
                r#"{"say": "Wages were 95000."}"#,
            ])),
    );
    let edits = Arc::new(common::EditsRecorder::new());
    let base = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .recording_acts(common::again(&fakes.conversations))
        .reading(&drive)
        .settings(common::SettingsFixture::with_edits(Arc::clone(&edits) as _))
        .start()
        .base;
    let (id, _) = exchange(&base, "what were my wages");
    let response = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        tap,
    );
    let status = common::status(&response);
    let payload = settled(&base, &id);
    Tapped {
        drive,
        edits,
        status,
        payload,
        fakes,
    }
}

/// The tap reads it: the text lands in the conversation as a `drive_read`
/// answer, redacted, and the proposal reads acted.
#[cfg(feature = "fixture")]
#[test]
fn a_tapped_document_request_puts_the_masked_text_in_the_conversation() {
    let Tapped {
        drive,
        edits,
        status,
        payload,
        fakes,
    } = tapped("drive-tapped", &serde_json::json!({}));
    assert_eq!(status, 200);
    let read = answers(&payload, "drive_read");
    let text = read
        .iter()
        .find(|(text, refused)| !refused && text.contains("Wages 95000"))
        .map(|(text, _)| text)
        .expect("the tap's text is in the transcript");
    assert!(
        text.contains("SSN [SSN ending 6789 · tap to show · held:file:")
            && text.contains("EIN [EIN ending 0001 · tap to show · held:file:"),
        "{text}"
    );
    assert_eq!(payload["proposals"][0]["state"], "acted", "{payload}");
    assert!(stored(&fakes).contains("Wages 95000"));
    assert!(!stored(&fakes).contains("123-45-6789"));
    assert!(edits.folders().is_empty(), "Read allows nothing");
    assert!(
        drive
            .calls()
            .iter()
            .any(|call| call
                .starts_with("filesystem download /shared-with-me/Ada Documents/w2.txt ")),
        "{:?}",
        drive.calls()
    );
    drive.only_reads_ran();
}

/// "Always allow this folder" writes the file's folder, then reads the file.
#[cfg(feature = "fixture")]
#[test]
fn always_allow_writes_the_folder_and_reads_the_document() {
    let Tapped {
        drive,
        edits,
        status,
        payload,
        ..
    } = tapped("drive-always", &serde_json::json!({ "always": true }));
    assert_eq!(status, 200);
    assert_eq!(
        edits
            .folders()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["/shared-with-me/Ada Documents"],
        "the file's folder, and only it"
    );
    assert!(edits.allowed().is_empty(), "no web rule");
    assert!(
        answers(&payload, "drive_read")
            .iter()
            .any(|(text, refused)| !refused && text.contains("Wages 95000")),
        "and the document was read too"
    );
    drive.only_reads_ran();
}

/// A document the owner tapped Read on sits outside every allowed folder, and its held values
/// still reveal in the conversation that read it and send on the tap: the box reads it again as
/// the tap read it, never through the fence that refused it.
#[cfg(feature = "fixture")]
#[test]
fn a_tapped_document_s_number_reveals_and_sends_from_its_conversation() {
    use suzatary_core::held_value::{Handle, HeldFile, Holder, Place, Placeholder, Reach};
    use suzatary_core::redact::Kind;
    const PATH: &str = "/shared-with-me/Ada Documents/w2.txt";
    let place = Place::new(
        Holder::File(HeldFile::Drive {
            path: PATH.parse().expect("a path"),
        }),
        0,
    )
    .expect("a place");
    let ssn = Placeholder::of(Kind::Ssn, "123-45-6789", Reach::Revealable(place.clone())).print();
    let drive = FakeDrive::new("drive-tapped-held");
    let fakes = Arc::new(
        Fakes::new("drive-tapped-held")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .model(FakeModel::saying_owned([
                read_call(PATH),
                r#"{"say": "Asked."}"#.to_owned(),
                r#"{"say": "Wages were 95000."}"#.to_owned(),
                serde_json::json!({"say": "Drafted.", "calls": [
                    {"verb": "propose_email", "args": {
                        "account": "her@example.invalid",
                        "to": "counsel@example.test",
                        "subject": "The filing",
                        "body": format!("My SSN is {ssn}."),
                        "why": "Counsel asked for the number.",
                        "sources": "todo:life/permitfiling",
                    }},
                ]})
                .to_string(),
                r#"{"say": "Proposed."}"#.to_owned(),
            ])),
    );
    let running = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .recording_acts(common::again(&fakes.conversations))
        .reading(&drive)
        .start();
    let base = &running.base;
    let (id, _) = exchange(base, "what were my wages");
    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    settled(base, &id);

    let mut revealed = common::act_signed(
        &format!("{base}/api/held/reveal"),
        &serde_json::json!({ "handle": Handle::Value(place).print(), "conversation": id }),
        &common::phone(),
    );
    assert_eq!(common::status(&revealed), 200);
    assert_eq!(
        common::body_json(&mut revealed),
        serde_json::json!({ "type": "Shown", "value": "123-45-6789" })
    );

    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({ "say": "send counsel my SSN" }),
    );
    assert_eq!(common::status(&said), 202);
    let payload = settled(base, &id);
    let email = payload["proposals"]
        .as_array()
        .and_then(|rows| rows.iter().position(|row| row["act"]["kind"] == "email"))
        .expect("the email was proposed");
    let mut sent = common::act_signed(
        &format!("{base}/api/secretary/conversations/{id}/proposals/{email}/tap"),
        &serde_json::json!({}),
        &common::phone(),
    );
    assert_eq!(common::status(&sent), 200);
    assert_eq!(
        common::body_json(&mut sent)["proposals"][email]["state"],
        "sent"
    );
    assert_eq!(running.sent.sent()[0].body, "My SSN is 123-45-6789.");
}

/// A PDF in the private folder that the owner tapped Read on is withheld, and a proposal that
/// sends it reads nothing from Drive: it is recorded from what the tap read, the model is told
/// no digest of it, and the tap that sends it is the one read past the fence.
#[cfg(feature = "fixture")]
#[test]
fn a_tapped_private_file_is_proposed_without_a_read_and_sent_on_the_tap() {
    use suzatary_core::held_value::{HeldFile, Placeholder};
    const PATH: &str = "/my-files/Suzatary/Private/w2.pdf";
    let pdf = common::pdf_saying("SSN 123-45-6789");
    let digest = suzatary_core::attachment::AttachmentId::of(&pdf);
    let file = Placeholder::file(
        &HeldFile::Drive {
            path: PATH.parse().expect("a path"),
        },
        "w2.pdf",
        pdf.len() as u64,
    )
    .print();
    let drive = FakeDrive::new("drive-tapped-private").holding_private("w2.pdf", &pdf);
    let fakes = Arc::new(
        Fakes::new("drive-tapped-private")
            .whois(common::FakeWhois::new().naming(common::v4(), ada()))
            .drive(&drive)
            .model(FakeModel::saying_owned([
                read_call(PATH),
                r#"{"say": "Asked."}"#.to_owned(),
                r#"{"say": "Read."}"#.to_owned(),
                serde_json::json!({"say": "Drafted.", "calls": [
                    {"verb": "propose_email", "args": {
                        "account": "her@example.invalid",
                        "to": "counsel@example.test",
                        "subject": "The W-2",
                        "body": format!("Attached is {file}."),
                        "why": "Counsel asked for the W-2.",
                        "sources": "todo:life/permitfiling",
                    }},
                ]})
                .to_string(),
                r#"{"say": "Proposed."}"#.to_owned(),
            ])),
    );
    let running = common::Server::new(Arc::clone(&fakes), deployment(), Vec::new())
        .recording_acts(common::again(&fakes.conversations))
        .reading(&drive)
        .start();
    let base = &running.base;
    let (id, _) = exchange(base, "what is on my W-2");
    let tapped = common::act(
        &format!("{base}/api/secretary/conversations/{id}/proposals/0/tap"),
        &serde_json::json!({}),
    );
    assert_eq!(common::status(&tapped), 200);
    settled(base, &id);
    let downloads = || {
        drive
            .calls()
            .iter()
            .filter(|call| call.starts_with("filesystem download"))
            .count()
    };
    let read_on_tap = downloads();
    assert_eq!(read_on_tap, 1, "{:?}", drive.calls());

    let said = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({ "say": "send counsel my W-2" }),
    );
    assert_eq!(common::status(&said), 202);
    let payload = settled(base, &id);
    assert_eq!(
        downloads(),
        read_on_tap,
        "proposing read nothing from Drive"
    );
    let asked = format!("{:?}{:?}", fakes.model.asked(), fakes.model.resumed_with());
    assert!(
        !asked.contains(&digest.as_str()[..12]),
        "a model was told the file's digest"
    );
    let email = payload["proposals"]
        .as_array()
        .and_then(|rows| rows.iter().position(|row| row["act"]["kind"] == "email"))
        .expect("the email was proposed");

    let mut sent = common::act_signed(
        &format!("{base}/api/secretary/conversations/{id}/proposals/{email}/tap"),
        &serde_json::json!({}),
        &common::phone(),
    );
    assert_eq!(common::status(&sent), 200);
    assert_eq!(
        common::body_json(&mut sent)["proposals"][email]["state"],
        "sent"
    );
    assert_eq!(downloads(), read_on_tap + 1, "the send tap read it once");
    assert_eq!(running.sent.sent()[0].body, "Attached is w2.pdf.");
    assert_eq!(running.sent.parts()[0][0].content, pdf);
}
