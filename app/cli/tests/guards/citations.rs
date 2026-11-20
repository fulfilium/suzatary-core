//! A document's `(→ id)` is a link to a decision of `memory/`, and a link to one that is
//! superseded, withdrawn or absent is dead: every one in `CLAUDE.md`, `AGENTS.md`, `OPERATOR.md`,
//! `SECURITY.md` and `docs/` names a decision in force.

use super::repo;
use std::path::PathBuf;
use suzatary_io::objects::Objects;
use suzatary_todo::decisions::{DecisionId, Decisions};

/// The ids `text` cites: each backticked id after a `→` that opens a parenthesis or follows
/// `; `, and the ids listed after it (``(→ `a`, `b`)``).
fn cited(text: &str) -> Vec<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut ids = Vec::new();
    for (at, _) in text.match_indices("→ `") {
        if !(text[..at].ends_with('(') || text[..at].ends_with("; ")) {
            continue;
        }
        let mut rest = &text[at + "→ ".len()..];
        while let Some((id, tail)) = rest.strip_prefix('`').and_then(|r| r.split_once('`')) {
            ids.push(id.to_owned());
            rest = tail.strip_prefix(", ").unwrap_or_default();
        }
    }
    ids
}

/// The documents whose citations are checked: the four at the root and every `.md` under `docs/`.
fn documents() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = ["CLAUDE.md", "AGENTS.md", "OPERATOR.md", "SECURITY.md"]
        .map(|name| repo().join(name))
        .into();
    let mut dirs = vec![repo().join("docs")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("docs/ reads") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                found.push(path);
            }
        }
    }
    found
}

#[test]
fn every_citation_names_a_decision_in_force() {
    assert_eq!(
        cited("a (→\n  `x`, `y`) b; → `z` c → `w` (→ id)"),
        ["x", "y", "z"]
    );
    let store = Objects::open(repo().join("memory"));
    let ledger = Decisions { store: &store }.ledger().expect("memory/ reads");
    let mut checked = 0;
    let mut dead = Vec::new();
    for path in documents() {
        let text = std::fs::read_to_string(&path).expect("a document reads");
        for id in cited(&text) {
            checked += 1;
            if !DecisionId::new(id.as_str()).is_ok_and(|id| ledger.in_force(&id)) {
                let rel = path.strip_prefix(repo()).expect("in the repository");
                dead.push(format!("{}: {id}", rel.display()));
            }
        }
    }
    assert!(checked > 0, "the documents cite decisions");
    assert!(
        dead.is_empty(),
        "cited decisions that are not in force (`suzatary decision list --store memory` names what stands): {dead:#?}"
    );
}
