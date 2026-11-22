//! The sources `rendering.rs` hashes into the name of every kept render, pinned by digest
//! (`guards/rendering.toml`): a change to one renames every kept render, so it says so where it is
//! reviewed.

use super::repo;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
struct Hashed {
    file: String,
    sha256: String,
}

fn pinned() -> Vec<Hashed> {
    #[derive(serde::Deserialize)]
    struct File {
        sources: Vec<Hashed>,
    }
    let text = std::fs::read_to_string(repo().join("app/cli/tests/guards/rendering.toml"))
        .expect("rendering.toml");
    toml::from_str::<File>(&text)
        .expect("guards/rendering.toml")
        .sources
}

/// A change to a hashed source renames every kept render, so the pin shows it where it is reviewed.
#[test]
fn the_sources_a_render_is_named_by_are_pinned() {
    let src = repo().join("app/todo/src");
    let text = std::fs::read_to_string(src.join("rendering.rs")).expect("rendering.rs");
    let mut hashed = BTreeMap::new();
    for (at, _) in text.match_indices("include_str!(\"") {
        let rest = &text[at + "include_str!(\"".len()..];
        let literal = &rest[..rest.find('"').expect("a closed literal")];
        let bytes = std::fs::read(src.join(literal)).expect("a hashed source");
        hashed.insert(literal.to_owned(), format!("{:x}", Sha256::digest(bytes)));
    }
    assert_eq!(
        hashed,
        pinned()
            .iter()
            .map(|h| (h.file.clone(), h.sha256.clone()))
            .collect::<BTreeMap<_, _>>(),
        "a source rendering.rs hashes changed, or the list did: every kept render is renamed. \
         Say so in the pull request, then set guards/rendering.toml's `sources` to the left side"
    );
}
