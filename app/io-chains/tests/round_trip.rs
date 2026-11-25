//! An object is named by its bytes, so every whole object a store holds must reprint as it was
//! stored: a printer that printed one otherwise would give it another name. A file that does not
//! hash to its name is no whole (a husk, or a tampered file), and `verify` checks it. A proposal
//! is named by its act as first asked, so no id names two acts; a migrated act keeps the id its
//! act's JSON hashed to, since a conversation's lines name it.
//!
//! `SUZATARY_ROUND_TRIP` names a data directory, a box's or a fixture's, to check the same way.

use std::path::Path;

use prodrome_data::event::{Envelope, canonical_envelope, parse_envelope};
use prodrome_data::literal::codec::Literal as _;
use prodrome_data::literal::print_literal;
use prodrome_data::schema::Schema;
use sha2::Digest as _;
use suzatary_digest::secretary::digest_event::DigestEvent;
use suzatary_inbox::secretary::inbox_event::{InboxEvent, Said};
use suzatary_lines::secretary::line_event::LineEvent;
use suzatary_memory::secretary::memory_event::MemoryEvent;
use suzatary_record::record::TodoEvent;
use suzatary_stores::asks::AskEvent;
use suzatary_todo::decisions::DecisionEvent;
use suzatary_topics::secretary::topics::reading::{AliasEvent, MembershipEvent};

use crate::stores;

type Check = fn(&Path) -> Read;

const STORES: [(&str, Check); 10] = [
    ("events", faults::<TodoEvent>),
    ("events-suzatary", faults::<TodoEvent>),
    (stores::INBOX, inbox),
    (stores::LINES, faults::<LineEvent>),
    (stores::MEMORY, faults::<MemoryEvent>),
    (stores::DIGESTS, faults::<DigestEvent>),
    (stores::DECISIONS, faults::<DecisionEvent>),
    (stores::TOPICS, faults::<MembershipEvent>),
    (stores::TOPIC_ALIASES, faults::<AliasEvent>),
    (stores::ASKS, faults::<AskEvent>),
];

struct Read {
    wholes: usize,
    faults: Vec<String>,
}

/// Each whole object of the store at `store` with its name, and one line for each that does not
/// decode or does not reprint as its file.
fn wholes<E: Schema<Vocabulary: Default>>(
    store: &Path,
) -> (Vec<(String, Envelope<E>)>, Vec<String>) {
    let vocabulary = E::Vocabulary::default();
    let mut paths: Vec<_> = std::fs::read_dir(store.join("objects"))
        .unwrap_or_else(|failed| panic!("{} has no objects: {failed}", store.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "py"))
        .collect();
    paths.sort();
    let mut read = Vec::new();
    let mut faults = Vec::new();
    for path in &paths {
        let stored = std::fs::read(path).expect("an object reads");
        let name = hex::encode(sha2::Sha256::digest(&stored));
        if path.file_stem().is_none_or(|stem| *stem != *name) {
            continue;
        }
        let Ok(stored) = String::from_utf8(stored) else {
            faults.push(format!("{name}: is not text"));
            continue;
        };
        match parse_envelope::<E>(&vocabulary, &stored) {
            Ok(envelope) if canonical_envelope(&envelope) == stored => read.push((name, envelope)),
            Ok(_) => faults.push(format!("{name}: reprints as other bytes")),
            Err(failed) => faults.push(format!("{name}: does not decode: {failed}")),
        }
    }
    (read, faults)
}

fn faults<E: Schema<Vocabulary: Default>>(store: &Path) -> Read {
    let (read, faults) = wholes::<E>(store);
    Read {
        wholes: read.len(),
        faults,
    }
}

/// [`faults`] at the inbox's schema, and a line for each proposal id that names two acts.
fn inbox(store: &Path) -> Read {
    let (read, mut faults) = wholes::<InboxEvent>(store);
    let mut acts = std::collections::BTreeMap::new();
    for (name, envelope) in &read {
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
    Read {
        wholes: read.len(),
        faults,
    }
}

/// Every store of [`STORES`] that `root` holds, checked: how many wholes, and each fault under
/// its store's directory.
fn every_store(root: &Path) -> Read {
    let mut wholes = 0;
    let mut faults = Vec::new();
    for (directory, check) in STORES {
        let store = root.join(directory);
        if store.join("objects").is_dir() {
            let read = check(&store);
            eprintln!("{directory}: {} wholes reprint", read.wholes);
            wholes += read.wholes;
            faults.extend(
                read.faults
                    .into_iter()
                    .map(|fault| format!("{directory} {fault}")),
            );
        }
    }
    Read { wholes, faults }
}

#[test]
fn the_frozen_stores_reprint_as_stored() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let proposals = inbox(&tests.join("fixtures/inbox-proposals"));
    let stores = every_store(&tests.join("stores-changes"));
    assert!(proposals.wholes > 0, "the frozen inbox is empty");
    assert_eq!(
        stores.wholes, 35,
        "every object of stores-changes/ is a whole"
    );
    let faults = [proposals.faults, stores.faults].concat();
    assert!(faults.is_empty(), "{}", faults.join("\n"));
}

#[test]
fn the_stores_named_by_the_environment_reprint_as_stored() {
    let Some(root) = std::env::var_os("SUZATARY_ROUND_TRIP") else {
        eprintln!("skipped: SUZATARY_ROUND_TRIP names no data directory");
        return;
    };
    let read = every_store(Path::new(&root));
    eprintln!("{} objects reprint", read.wholes);
    assert!(read.faults.is_empty(), "{}", read.faults.join("\n"));
}
