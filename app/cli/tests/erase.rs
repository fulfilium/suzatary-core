//! The owner erases a conversation's words over a real server: a turn, or the whole of it, read
//! back as erased with every line's shape standing, and gone from every file the box keeps.

mod common;

use std::sync::Arc;

use common::{FakeModel, Fakes};

fn ada() -> suzatary_core::capability::Login {
    common::login("ada@example.com")
}

/// A conversation with one round in it, its words under `words`, and the server's base.
fn one_round(tag: &str, words: &str) -> (Arc<Fakes>, String, String) {
    let whois = common::FakeWhois::new().naming(common::v4(), ada());
    let fakes = Arc::new(
        Fakes::new(tag)
            .whois(whois)
            .model(FakeModel::saying([r#"{"say": "Noted, the blue folder."}"#])),
    );
    let deployment = common::deployment([ada()], [(ada(), common::actor("ada"))]);
    let base = common::Server::new(Arc::clone(&fakes), deployment, Vec::new())
        .start()
        .base;
    let mut opened = common::act(
        &format!("{base}/api/secretary/conversations"),
        &serde_json::json!({"title": "Folders"}),
    );
    assert_eq!(common::status(&opened), 200);
    let id = common::body_json(&mut opened)["id"]
        .as_str()
        .expect("an id")
        .to_owned();
    let say = common::act(
        &format!("{base}/api/secretary/conversations/{id}/say"),
        &serde_json::json!({"say": words}),
    );
    assert_eq!(common::status(&say), 202);
    settled(&base, &id);
    (fakes, base, id)
}

fn settled(base: &str, id: &str) -> serde_json::Value {
    for _ in 0..200 {
        let payload = common::reading(base, id, &[]);
        if payload["state"] == "idle" {
            return payload;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    panic!("the round never finished");
}

/// Whether any file under `root` holds `words`.
fn held(root: &std::path::Path, words: &str) -> bool {
    std::fs::read_dir(root).expect("a directory").any(|entry| {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            held(&path, words)
        } else {
            std::fs::read(&path).is_ok_and(|bytes| {
                bytes
                    .windows(words.len())
                    .any(|window| window == words.as_bytes())
            })
        }
    })
}

#[test]
fn the_owner_erases_one_turn_and_its_neighbours_stand() {
    let (_, base, id) = one_round("erase-turn", "the spare key is in the blue folder");
    let mut erased = common::act(
        &format!("{base}/api/secretary/conversations/{id}/erase"),
        &serde_json::json!({"turns": [0]}),
    );
    assert_eq!(common::status(&erased), 200);
    let erased = common::body_json(&mut erased);
    assert_eq!(erased["turns"][0]["who"], "person");
    assert_eq!(erased["turns"][0]["text"], "[Ada erased these words]");
    assert_eq!(erased["turns"][1]["text"], "Noted, the blue folder.");

    let nowhere = common::act(
        &format!("{base}/api/secretary/conversations/{id}/erase"),
        &serde_json::json!({"turns": [9]}),
    );
    assert_eq!(common::status(&nowhere), 404);
    let empty = common::act(
        &format!("{base}/api/secretary/conversations/{id}/erase"),
        &serde_json::json!({"turns": []}),
    );
    assert_eq!(common::status(&empty), 400);
}

#[test]
fn the_owner_erases_a_whole_conversation_and_no_file_keeps_it() {
    let words = "the spare key is in the blue folder";
    let (fakes, base, id) = one_round("erase-whole", words);
    let keys = fakes.root.join("secretary-lines-keys");
    let before = std::fs::read_dir(&keys).expect("keys").count();
    assert!(before >= 3, "a title, a question and an answer: {before}");
    let mut erased = common::act(
        &format!("{base}/api/secretary/conversations/{id}/erase"),
        &serde_json::json!({"whole": true}),
    );
    assert_eq!(common::status(&erased), 200);
    let erased = common::body_json(&mut erased);
    let turns = erased["turns"].as_array().expect("turns");
    assert_eq!(turns.len(), 2, "every turn stands, as a shape: {erased}");
    assert!(
        turns
            .iter()
            .all(|turn| turn["text"] == "[Ada erased these words]"),
        "{erased}"
    );
    assert!(!held(&fakes.root, words), "no file holds the words");
    assert!(!held(&fakes.root, "Noted, the blue folder."));
    assert!(
        std::fs::read_dir(&keys).expect("keys").count() < before,
        "the keys are destroyed"
    );
}

/// A summary that read erased words is a copy of them: the erasure's route runs the memory's
/// sweep after the lines', so no digest text the box keeps holds them.
#[test]
fn an_erasure_takes_the_summaries_that_read_it() {
    let words = "the spare key is in the blue folder";
    let (fakes, base, id) = one_round("erase-summary", words);
    let summary = format!("Ada said {words}");
    let memories = fakes.memories();
    let digest = memories.digests.read().expect("the digest reads");
    let root = digest.tree().expect("lines").root();
    // Written as the compactor writes it, over the root, whichever node is pending: which is,
    // is a function of the lines' names, and every key is fresh.
    let text = suzatary_core::secretary::salted_text::Text::new(
        suzatary_core::secretary::digest_line::Line::new(&summary).expect("a line"),
        [7; suzatary_core::secretary::salted_text::SALT_BYTES],
    );
    memories
        .digests
        .texts
        .set(&text.name(), &text)
        .expect("kept");
    memories
        .digests
        .history
        .append(
            suzatary_core::secretary::digest_event::DigestEvent::summarized(
                root.name().clone(),
                prodrome_core::literal::Datetime::new(2026, 10, 9, 9, 0, 0, 0).expect("an instant"),
                suzatary_core::secretary::digests::compactor(),
                &text,
                suzatary_core::secretary::digest_event::Attempt {
                    over: root
                        .children()
                        .iter()
                        .map(|child| child.name().clone())
                        .collect(),
                    model: suzatary_core::model_name::ModelName::parse("claude-sonnet-5-5")
                        .expect("a model"),
                    taint: root.measure().taint,
                },
            )
            .expect("a summary"),
        )
        .expect("written");
    let texts = fakes.root.join("secretary-digest-texts");
    assert!(
        held_in(&texts, &summary),
        "the summary is kept while its words stand"
    );
    let erased = common::act(
        &format!("{base}/api/secretary/conversations/{id}/erase"),
        &serde_json::json!({"whole": true}),
    );
    assert_eq!(common::status(&erased), 200);
    assert!(
        !held_in(&texts, &summary),
        "no summary that read them is kept"
    );
}

/// Whether any file under `dir`, which may not exist, holds `words`.
fn held_in(dir: &std::path::Path, words: &str) -> bool {
    dir.exists() && held(dir, words)
}
