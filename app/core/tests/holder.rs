//! The holders an erasure and a reseal empty, as the sweep reads them.

use prodrome_core::literal::Datetime;

use suzatary_base::conversation_id::ConversationId;
use suzatary_core::secretary::conversations::InMemory;
use suzatary_core::secretary::line_event::LineEvent;
use suzatary_core::secretary::model::{SessionId, Turn};
use suzatary_core::secretary::store::{Entry, Kind};
use suzatary_core::secretary::transcript;
use suzatary_memory::secretary::erasure::Erasure;
use suzatary_memory::secretary::scrubbed_under::ScrubbedUnder;
use suzatary_memory::secretary::words::Words;
use suzatary_store::store::{Doc as _, Gets as _, Prodrome as _};

const SESSION: &str = "6f1c2b0e-8a4d-4c55-9f3e-2b7a1d9e0c41";

fn at(second: u32) -> Datetime {
    Datetime::new(2026, 11, 8, 9, 0, second, 0).expect("an instant")
}

/// A conversation the model held a session over, written under older rules, so a reseal
/// finds its owner's words stale; answers the key of those words.
fn spoken(
    held: &InMemory,
    id: &ConversationId,
) -> suzatary_memory::secretary::locked_text::KeyName {
    let entries = [
        Entry::Opened {
            at: at(0),
            title: Words::kept("Folders".to_owned()),
            model: "a-cli/small".to_owned(),
            kind: Kind::Topic,
        },
        Entry::Session {
            at: at(1),
            session: SessionId::new(SESSION).expect("a uuid"),
            model: "a-cli/small".to_owned(),
            cwd: "/srv/code".to_owned(),
            surface: None,
        },
        Entry::Said {
            at: at(2),
            turn: Turn::Person {
                text: "reset at https://example.test/r?token=s3cret".to_owned(),
                references: Vec::new(),
                attachments: Vec::new(),
            },
            by: Some("ada".into()),
        },
    ];
    let mut said = None;
    for (count, entry) in entries.iter().enumerate() {
        let (line, keys) = LineEvent::written_under(
            &ScrubbedUnder::Unversioned,
            id.clone(),
            transcript::writer(),
            u32::try_from(count).expect("a count"),
            entry,
            &held.entropy,
        )
        .expect("a line");
        for key in &keys {
            held.keys.set(&key.name(), key).expect("kept");
        }
        said = keys
            .last()
            .map(suzatary_memory::secretary::locked_text::TextKey::name);
        held.lines.append(line).expect("appended");
    }
    said.expect("the owner's words have a key")
}

/// Law: a migration forgets every block its old words were rendered to. A model's answer
/// written under older rules holds a code the current rules redact; while it stood, it was
/// rendered from its words as they were locked, so its blocks hold the code. `migrate-scrub`
/// seals the slot again, and the sweep that destroys the old key forgets each of those blocks,
/// not the blocks of the words as the current rules read them.
#[test]
fn a_migration_forgets_the_blocks_its_old_words_were_rendered_to() {
    let held = InMemory::default();
    let id = ConversationId::new("2026-11-12-1").expect("a slug");
    let text = "Your verification code is 482913. fee\n\nThe second block.";
    let entries = [
        Entry::Opened {
            at: at(0),
            title: Words::kept("Codes".to_owned()),
            model: "a-cli/small".to_owned(),
            kind: Kind::Topic,
        },
        Entry::Said {
            at: at(1),
            turn: Turn::Model {
                text: text.to_owned(),
                calls: Vec::new(),
            },
            by: None,
        },
    ];
    for (count, entry) in entries.iter().enumerate() {
        let (line, keys) = LineEvent::written_under(
            &ScrubbedUnder::Unversioned,
            id.clone(),
            transcript::writer(),
            u32::try_from(count).expect("a count"),
            entry,
            &held.entropy,
        )
        .expect("a line");
        for key in &keys {
            held.keys.set(&key.name(), key).expect("kept");
        }
        held.lines.append(line).expect("appended");
    }
    let rendered: Vec<String> = suzatary_base::answer::blocks(text)
        .iter()
        .map(|block| {
            suzatary_base::answer::Renderable::of(&block.source)
                .as_str()
                .to_owned()
        })
        .collect();
    assert!(
        rendered.iter().any(|block| block.contains("482913")),
        "the old words' blocks hold the code: {rendered:?}"
    );
    suzatary_core::secretary::rescrub::run(held.conversations(), at(9)).expect("migrated");
    let forgot = held.forgot.sources();
    for block in &rendered {
        assert!(
            forgot.contains(block),
            "{block:?} is still held; forgotten: {forgot:?}"
        );
    }
}

/// Law: each act that kills a slot, an erasure and a reseal by `migrate-scrub`, deletes its
/// conversation's model session in the same act, and the slot's key is gone after it.
#[test]
fn killing_a_slot_deletes_its_conversation_s_model_session() {
    let id = ConversationId::new("2026-11-08-1").expect("a slug");
    for act in ["erase", "migrate-scrub"] {
        let held = InMemory::default();
        let key = spoken(&held, &id);
        match act {
            "erase" => transcript::erase(
                held.conversations(),
                &id,
                [key.clone()].into(),
                Erasure::Asked {
                    by: transcript::writer(),
                },
                at(9),
            )
            .map(|_| ()),
            _ => suzatary_core::secretary::rescrub::run(held.conversations(), at(9)).map(|_| ()),
        }
        .expect("the act runs");
        assert!(
            held.forgot
                .sources()
                .contains(&format!("session {SESSION} in /srv/code")),
            "{act}: {:?}",
            held.forgot.sources()
        );
        assert!(held.keys.get(&key).expect("reads").is_none(), "{act}");
    }
}
