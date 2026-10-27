//! An inbox object is named by its bytes, so every object must reprint as it was stored, and a
//! proposal is named by its act as first asked, so no id names two acts. An id is not re-derived
//! from a migrated act: a proposal carried into the literal form keeps the id its act's JSON
//! hashed to, since a conversation's lines name it.
//!
//! `fixtures/inbox-proposals` was written once and carried by two migrations since, its
//! readings unmoved: its one serving asking holds no spec, and its one dismissal is a person's
//! no. `SUZATARY_INBOX_ROUND_TRIP` names a live inbox directory to check the same way.

use std::path::Path;

use prodrome_core::event::{Envelope, canonical_envelope, parse_envelope};
use prodrome_core::literal::codec::Literal as _;
use prodrome_core::literal::print_literal;
use suzatary_core::secretary::inbox_event::{InboxEvent, InboxVocabulary, Said};

/// What reading `inbox` found: how many objects, and one line per object that does not reprint.
fn round_trip(inbox: &Path) -> (usize, Vec<String>) {
    let mut paths: Vec<_> = std::fs::read_dir(inbox.join("objects"))
        .unwrap_or_else(|failed| panic!("{} has no objects: {failed}", inbox.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "py"))
        .collect();
    paths.sort();
    let mut faults = Vec::new();
    let mut acts = std::collections::BTreeMap::new();
    for path in &paths {
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        let stored = std::fs::read_to_string(path).expect("an object is text");
        let envelope: Envelope<InboxEvent> = match parse_envelope(&InboxVocabulary, &stored) {
            Ok(envelope) => envelope,
            Err(failed) => {
                faults.push(format!("{name}: does not decode: {failed}"));
                continue;
            }
        };
        if canonical_envelope(&envelope) != stored {
            faults.push(format!("{name}: reprints as other bytes"));
        }
        if let Some(InboxEvent {
            proposal,
            said: Said::Asked(asked),
            ..
        }) = envelope.event()
        {
            let act = print_literal(&asked.proposal.to_value());
            if let Some(other) = acts.insert(proposal.clone(), act.clone())
                && other != act
            {
                faults.push(format!("{name}: {proposal} names two acts"));
            }
        }
    }
    (paths.len(), faults)
}

#[test]
fn the_frozen_inbox_reprints_as_stored() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inbox-proposals");
    let (read, faults) = round_trip(&dir);
    assert!(read > 0, "the frozen inbox is empty");
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn the_inbox_named_by_the_environment_reprints_as_stored() {
    let Some(dir) = std::env::var_os("SUZATARY_INBOX_ROUND_TRIP") else {
        eprintln!("skipped: SUZATARY_INBOX_ROUND_TRIP names no inbox directory");
        return;
    };
    let (read, faults) = round_trip(Path::new(&dir));
    eprintln!("{read} objects read");
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}
