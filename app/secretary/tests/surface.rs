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
