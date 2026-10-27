//! The functions that turn markup into HTML, each named by the code that computes it.
//!
//! A rendered value is kept by (its function's name, its source's name), on the box and in a tab
//! alike, so a function's name covers everything its value depends on: the typesetter (the
//! closed world's crate at its pinned revision and the typst it links, pinned to `Cargo.lock`
//! by a test), the narrowing of a document to its body past the tripwire (by its code), and an answer block's
//! preamble. A change to any of them is a new function, whose values nothing has yet, never a
//! second value under an old name.

use std::sync::LazyLock;

use prodrome_core::event::Hash;
use prodrome_core::memo;

/// The closed world's typesetter: the Prodrome's `prodrome-typst-wasm` at the revision
/// `app/Cargo.toml` pins, and the typst it links. A test reads `app/Cargo.lock` and fails when
/// either moves without this.
pub const TYPESETTER: &str =
    "prodrome-typst-wasm fcf061508b94a32630d159f0ccf7e9763d2005d1, typst 0.15.1";

/// How a compiled document becomes what is kept ([`crate::narrowing`]): named by its code and
/// the tripwire's, so an edit to either is a new function.
pub static NARROWING: LazyLock<Hash> = LazyLock::new(|| {
    memo::name(
        b"suzatary: narrowing",
        [
            &named(include_str!("narrowing.rs")),
            &named(include_str!("tripwire.rs")),
        ],
    )
});

/// What an answer block may call bare: the names of the owner's design system (typst-design),
/// whose own versions draw with shapes HTML export drops (a diamond, a capsule outline).
/// Compiled in the closed world with the block, so they are drawn with typst's own elements (a
/// panel is a block quote).
pub const ANSWER_PREAMBLE: &str = "\
#let sep(..items) = items.pos().join([ ◆ ])
#let panel(body, ..rest) = quote(block: true, body)
#let note(body, title: none, ..rest) = quote(block: true, {
  if title != none { strong(title); parbreak() }
  body
})
#let notice(body, ..rest) = note([_*Notice:*_ #body])
#let dated(left, right) = [#left — #right]
#let minor(body) = smallcaps(lower(body))
#let accent(body) = emph(body)
";

fn named(text: &str) -> Hash {
    memo::name(text.as_bytes(), [])
}

/// A confirmed text's markup, compiled in the closed world and narrowed to its body.
pub static MARKUP: LazyLock<Hash> = LazyLock::new(|| {
    memo::name(
        b"suzatary: markup to html in the closed world",
        [&named(TYPESETTER), &*NARROWING],
    )
});

/// An answer block, compiled in the closed world after [`ANSWER_PREAMBLE`] and narrowed to its
/// body. Named by the code that splits an answer into blocks too: an erasure finds the blocks of
/// erased words by splitting them again, so a split that moved would name blocks no erasure finds
/// under the same function.
pub static ANSWER: LazyLock<Hash> = LazyLock::new(|| {
    memo::name(
        b"suzatary: an answer block to html in the closed world",
        [
            &named(TYPESETTER),
            &*NARROWING,
            &named(ANSWER_PREAMBLE),
            &named(include_str!("secretary/answer.rs")),
        ],
    )
});

/// The closed world's answer to a source, its refusal included.
pub static CLOSED: LazyLock<Hash> = LazyLock::new(|| {
    memo::name(
        b"suzatary: the closed world's answer to a source",
        [&named(TYPESETTER)],
    )
});

/// A page this box printed, compiled by the `typst` binary and narrowed to its body. The
/// binary is the shell's, so its values are kept in the process only.
pub static PAGE: LazyLock<Hash> = LazyLock::new(|| {
    memo::name(
        b"suzatary: a printed page to html by the typst binary",
        [&*NARROWING],
    )
});

/// The functions whose values are kept beyond a process, on the box's disk and on a device. A value
/// kept under any other was named by another build, whose names an erasure in this one never
/// computes, so neither replica keeps it.
#[must_use]
pub fn kept() -> [&'static Hash; 2] {
    [&MARKUP, &ANSWER]
}

/// The name of a source, as a memo node is named by its bytes: the argument of every key.
#[must_use]
pub fn source(text: &str) -> Hash {
    named(text)
}

/// The key `function`'s value at `source` is kept under, printed as both tables print one (the function's name, a
/// space, the source's): what the box sends beside its HTML, so a tab files it under the box's own key.
#[must_use]
pub fn key(function: &Hash, text: &str) -> String {
    format!("{} {}", function.as_str(), source(text).as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The typesetter named here is the one `app/Cargo.lock` links, so a bump of either is a new
    /// function and never a stale value under the old one.
    #[test]
    fn the_typesetter_is_the_one_cargo_lock_links() {
        let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.lock"))
            .expect("app/Cargo.lock");
        let package = |name: &str| {
            lock.split("[[package]]")
                .find(|p| p.contains(&format!("name = \"{name}\"\n")))
                .unwrap_or_else(|| panic!("{name} is in the lock"))
                .to_owned()
        };
        let wasm = package("prodrome-typst-wasm");
        let rev = wasm
            .split('#')
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("a git source names its commit");
        let typst = package("typst");
        let version = typst
            .split("version = \"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("a version");
        assert_eq!(
            TYPESETTER,
            format!("prodrome-typst-wasm {rev}, typst {version}")
        );
    }

    #[test]
    fn every_function_has_its_own_name() {
        let names = [&*MARKUP, &*ANSWER, &*CLOSED, &*PAGE];
        for (i, a) in names.iter().enumerate() {
            for b in &names[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
