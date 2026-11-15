//! A transcript's person turn with and without photos, as `secretary::model::Turn` reads and
//! writes it; the attachment's own laws stay beside it in `suzatary_held::attachment`.

use suzatary_held::attachment::{Attachment, AttachmentId};

const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];

#[test]
fn a_turn_without_photos_reads_and_writes_as_it_always_did() {
    use suzatary_core::secretary::model::Turn;
    let old = r#"{"who":"person","text":"hi","references":[]}"#;
    let turn: Turn = serde_json::from_str(old).expect("a transcript from before photos");
    assert!(matches!(&turn, Turn::Person { attachments, .. } if attachments.is_empty()));
    assert_eq!(serde_json::to_string(&turn).expect("serialises"), old);

    let photo = Attachment::accept(PNG).expect("a png");
    let with = Turn::Person {
        text: "look".to_owned(),
        references: Vec::new(),
        attachments: vec![photo.clone()],
    };
    let line = serde_json::to_string(&with).expect("serialises");
    assert!(line.contains(r#""media_type":"image/png""#), "{line}");
    let back: Turn = serde_json::from_str(&line).expect("round trip");
    assert_eq!(back, with);
}

#[test]
fn a_stored_photo_turn_reads_and_writes_byte_for_byte() {
    use suzatary_core::secretary::model::Turn;
    let id = AttachmentId::of(PNG);
    let stored = format!(
        r#"{{"who":"person","text":"look","references":[],"attachments":[{{"id":"{id}","media_type":"image/png"}}]}}"#
    );
    let turn: Turn = serde_json::from_str(&stored).expect("a transcript line from before PDFs");
    assert_eq!(serde_json::to_string(&turn).expect("serialises"), stored);
}
