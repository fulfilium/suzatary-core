//! A snapshot of the verb surfaces the version number governs.
//!
//! Failing this is not a bug: decide whether the change is a minor (a verb or argument added) or
//! a major (one removed or renamed), bump the version, write the CHANGELOG entry, and rewrite the
//! snapshot in the same commit with
//! `SUZATARY_UPDATE_SURFACE=1 cargo test -p suzatary-secretary --test all surface::`.
//!
//! The snapshot is the whole `tools/list` answer, not the names: a stateless agent re-binds these
//! every run, so a changed argument, default or description is a change somebody must look at.

use std::path::PathBuf;

fn fixture() -> PathBuf {
    named("mcp-surface.json")
}

fn named(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(file)
}

/// One comparison for both surfaces, so failing either means the same thing.
fn snapshot(listed: &serde_json::Value, file: &str, whose: &str) {
    let printed = format!(
        "{}\n",
        serde_json::to_string_pretty(listed).expect("the surface serialises")
    );
    if std::env::var_os("SUZATARY_UPDATE_SURFACE").is_some() {
        std::fs::write(named(file), &printed).expect("the snapshot is writable");
        return;
    }
    let stored = std::fs::read_to_string(named(file)).expect("the snapshot is committed");
    assert_eq!(
        stored, printed,
        "the {whose} verb surface changed. Decide MINOR (added) or MAJOR (removed \
         or renamed), bump the version, write the CHANGELOG entry, and rerun \
         with SUZATARY_UPDATE_SURFACE=1 in the same commit."
    );
}

#[test]
fn the_verb_surface_is_the_committed_snapshot() {
    snapshot(
        &suzatary_secretary::agent::tools(),
        "mcp-surface.json",
        "MCP",
    );
}

/// The secretary's surface is a separate contract from the agent's: the two have different
/// authority, and one snapshot would be one file to widen both.
#[test]
fn the_secretarys_verb_surface_is_the_committed_snapshot() {
    snapshot(
        &suzatary_secretary::secretary::tools(),
        "secretary-surface.json",
        "secretary",
    );
}

/// Neither surface may grow a send or a confirm, checked against the snapshot a reader reads.
#[test]
fn the_secretarys_snapshot_holds_no_send_and_no_confirm() {
    let stored = std::fs::read_to_string(named("secretary-surface.json"))
        .expect("the snapshot is committed");
    let listed: serde_json::Value = serde_json::from_str(&stored).expect("valid JSON");
    let names: Vec<&str> = listed
        .as_array()
        .expect("an array of tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();
    assert!(
        names.contains(&"propose_email"),
        "the secretary's whole answer to sending is missing"
    );
    for forbidden in ["confirm", "append", "delete", "amend", "undo", "redo"] {
        assert!(
            !names.iter().any(|name| name.contains(forbidden)),
            "{forbidden} is on the secretary's surface"
        );
    }
    // No name here claims to send; `propose_email` only stores a message.
    assert!(
        !names.iter().any(|name| name.contains("send")),
        "sending is structurally human-only, on this surface too"
    );
}

/// `record` is the only mutator, and the forbidden names are pinned against the snapshot as well
/// as the table it came from.
#[test]
fn the_snapshot_holds_no_confirm_and_no_send() {
    let stored = std::fs::read_to_string(fixture()).expect("the snapshot is committed");
    let listed: serde_json::Value = serde_json::from_str(&stored).expect("valid JSON");
    let names: Vec<&str> = listed
        .as_array()
        .expect("an array of tools")
        .iter()
        .map(|tool| tool["name"].as_str().expect("a name"))
        .collect();
    assert!(names.contains(&"record"), "the one write is missing");
    // `propose` and `repo` too: attaching a repository's file and opening a pull request are the
    // secretary's, behind the owner's tap; the agent reads attacker-controlled mail with neither.
    for forbidden in [
        "confirm", "send", "reply", "append", "delete", "amend", "undo", "redo", "propose", "repo",
    ] {
        assert!(
            !names.iter().any(|name| name.contains(forbidden)),
            "{forbidden} is on the agent's surface"
        );
    }
}

/// No call keeps a session, so no text a model is sent names one: the manual and the verb list
/// (its system text), the conversation's head and the box's last sentence, the record's head as
/// the secretary and the agent read it, and every kind's word.
#[test]
fn what_a_model_reads_names_no_session() {
    let manual =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../SECRETARY.md"))
            .expect("the manual reads");
    let call = every_marker(manual);
    let prompt = suzatary_io_model::prompt::prompt_of(
        &call,
        &suzatary_secretary::secretary::surface::tools(),
        suzatary_io_model::prompt::Reads::Files,
    );
    let memory = suzatary_memory::secretary::memory::Memory {
        facts: Vec::new(),
        forgotten: Vec::new(),
        merged: Vec::new(),
        version: 0,
    };
    let topics = suzatary_topics::secretary::topics::reading::Reading::default();
    let secretary = suzatary_digest::secretary::told::of(
        suzatary_digest::secretary::told::Reader::Secretary,
        Ok(&memory),
        Ok(&topics),
        Ok(Vec::new()),
    )
    .text();
    let agent = suzatary_digest::secretary::shown_record::told(&memory, Ok(&[]), 0).text();
    let kinds: Vec<&str> = suzatary_digest::secretary::digest_label::Kind::ALL
        .into_iter()
        .map(suzatary_digest::secretary::digest_view::word)
        .collect();
    let forms = record_forms();
    for (what, text) in [
        ("the system text", prompt.system.as_str().to_owned()),
        ("the transcript", prompt.transcript()),
        ("the secretary's record", secretary),
        ("the agent's record", agent),
        ("the kinds", kinds.join(", ")),
        ("the record's line forms", forms.join("\n")),
    ] {
        let text = text.to_lowercase();
        assert!(!text.contains("your_memory"), "{what} names your_memory");
        let rest = [
            "coding sessions",
            "coding session",
            "coding_session",
            "codingsession",
        ]
        .iter()
        .fold(text, |text, coding| text.replace(coding, ""));
        assert!(!rest.contains("session"), "{what} names a session");
    }
}

/// A call holding every marker: an earlier long answer with a call and without, an earlier file
/// with a call and without, an earlier upload, and the round's own file and upload.
fn every_marker(manual: String) -> suzatary_conversations::secretary::model::Conversation {
    use suzatary_conversations::secretary::model::{Earlier, ToolCall, Turn};
    let file = suzatary_held::enclosed::Enclosed {
        id: suzatary_held::attachment::AttachmentId::of(b"a file"),
        name: "notice.pdf".to_owned(),
        media: suzatary_held::enclosed::Enclosure::Pdf,
        size: 3,
        text: String::new(),
    };
    let handed = Turn::Tool {
        verb: "attachments".into(),
        answer: "{}".to_owned(),
        outcome: suzatary_conversations::secretary::outcome::Outcome::Answered,
        files: vec![file],
    };
    let show = ToolCall {
        verb: "show".into(),
        args: [("id".to_owned(), serde_json::json!("a"))].into(),
    };
    let uploaded = Turn::Person {
        text: "this".to_owned(),
        references: Vec::new(),
        attachments: vec![suzatary_held::attachment::Attachment {
            id: suzatary_held::attachment::AttachmentId::of(b"a photo"),
            media_type: suzatary_held::enclosed::Enclosure::Image(
                suzatary_held::attachment::MediaType::Png,
            ),
            name: None,
            private: None,
        }],
    };
    let mut call = suzatary_conversations::secretary::model::Conversation::bare(
        manual,
        vec![uploaded.clone(), handed.clone()],
    );
    call.earlier = vec![
        Earlier::Said(uploaded),
        Earlier::Long {
            verb: "show".into(),
            call: Some(show.clone()),
            bytes: Some(9000),
        },
        Earlier::Long {
            verb: "reference".into(),
            call: None,
            bytes: None,
        },
        Earlier::Answered {
            call: Some(show),
            turn: handed.clone(),
        },
        Earlier::Answered {
            call: None,
            turn: handed,
        },
    ];
    call
}

/// The record's line forms of the box's own lines, and a part by its kinds, as a record and a
/// zoom's answer show them.
fn record_forms() -> Vec<String> {
    use suzatary_conversations::secretary::store::{Entry, Retired, Rewind, Scope};
    use suzatary_digest::secretary::digest_label::{Kind, Kinds, Label};
    use suzatary_digest::secretary::digest_view::{Body, rendered};
    let at =
        suzatary_base::capability::Datetime::new(2026, 11, 25, 9, 0, 0, 0).expect("an instant");
    let mut forms: Vec<String> = [
        Entry::Stopped {
            at,
            by: "ada".to_owned(),
        },
        Entry::Rewound {
            to: 0,
            at,
            by: "ada".to_owned(),
        },
        Entry::Again {
            at,
            question: prodrome_data::event::Hash::new("0".repeat(64)).expect("a name"),
            how: Rewind::Edit,
            by: "ada".to_owned(),
        },
        Entry::Retired {
            at,
            retired: Retired::Session,
        },
        Entry::Retired {
            at,
            retired: Retired::MemorySeen,
        },
        Entry::Retired {
            at,
            retired: Retired::Unheard {
                model: "m".to_owned(),
            },
        },
        Entry::Scoped {
            at,
            scope: Scope::Window,
        },
        Entry::Scoped {
            at,
            scope: Scope::Transcript,
        },
    ]
    .iter()
    .map(rendered)
    .collect();
    let every = Kind::ALL
        .into_iter()
        .fold(Kinds::one(Kind::Person), |kinds, kind| {
            kinds.join(&Kinds::one(kind))
        });
    for taint in suzatary_conversations::secretary::taint::Taint::ALL {
        for label in [
            Label {
                taint,
                kinds: every,
            },
            Label {
                taint,
                kinds: Kinds::one(Kind::Model),
            },
        ] {
            forms.push(Body::Hidden(label).text(3));
            forms.push(Body::Unsummarised(label).text(3));
        }
    }
    forms
}
