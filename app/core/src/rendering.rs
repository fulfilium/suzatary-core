//! The templates that turn values into HTML, and the names of the code every key is made from.
//!
//! A rendered value is kept by (its template's function, its value's name), on the box and in a
//! tab alike ([`crate::template`]). A template's function is named by its files, its type's
//! schema, the typesetter and the code that narrows a document, so a change to any of them is a
//! new function whose values nothing has yet, never a second value under an old name.

use std::sync::LazyLock;

use prodrome_core::event::Hash;
use prodrome_core::memo;
use schemars::JsonSchema;
use serde::Serialize;

use crate::secretary::answer::Renderable;
use crate::template::{Checked, Kept, Library, Output, Template, named};

/// The typesetter every template compiles with: the Prodrome's `prodrome-typst-wasm` at the
/// revision `app/Cargo.lock` links, and the typst it links. A test,
/// `the_typesetter_is_the_one_cargo_lock_links`, fails when the lock moves without this, and
/// prints what it must say.
pub const LINKED: &str =
    "prodrome-typst-wasm a0e99598eb61866c447f386dcb75557b0520dad4, typst 0.15.1";

/// [`LINKED`]'s name, which every template's function and the closed world's answers carry.
pub static TYPESETTER: LazyLock<Hash> = LazyLock::new(|| named(LINKED));

/// The tripwire's code: what a document must pass to be kept or written.
pub static TRIPWIRE: LazyLock<Hash> = LazyLock::new(|| named(include_str!("tripwire.rs")));

/// How a compiled document becomes the body that is kept ([`crate::narrowing`]): named by its
/// code and the tripwire's, so an edit to either is a new function.
pub static NARROWING: LazyLock<Hash> = LazyLock::new(|| {
    memo::name(
        b"suzatary: narrowing",
        [&named(include_str!("narrowing.rs")), &*TRIPWIRE],
    )
});

/// The closed world's answer to a lone text, its refusal included.
pub static CLOSED: LazyLock<Hash> =
    LazyLock::new(|| memo::name(b"suzatary: the closed world's answer", [&*TYPESETTER]));

/// Markup a trusted writer typed: a todo's field, a saved briefing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Markup {
    /// The markup, as the door admitted it.
    pub source: crate::escape::Escaped,
}

/// A confirmed text's markup, compiled in the closed world.
pub static MARKUP: Template<Markup> = Template::new(
    "/markup.typ",
    &[("/markup.typ", include_str!("rendering/markup.typ"))],
    Library::Closed,
    Output::Body,
    Kept::Disk,
    &[],
);

/// An answer block, compiled in the closed world. Named by the code that splits an answer into
/// blocks too: an erasure finds the blocks of erased words by splitting them again, so a split
/// that moved would name blocks no erasure finds under the same function.
pub static ANSWER: Template<Renderable> = Template::new(
    "/answer.typ",
    &[("/answer.typ", include_str!("rendering/answer.typ"))],
    Library::Closed,
    Output::Body,
    Kept::Disk,
    &[include_str!("secretary/answer.rs")],
);

/// The functions whose values are kept beyond a process, on the box's disk and on a device. A
/// value kept under any other was named by another build, whose names an erasure in this one
/// never computes, so neither replica keeps it.
#[must_use]
pub fn kept() -> [&'static Hash; 2] {
    [MARKUP.function(), ANSWER.function()]
}

/// A todo's reading part.
pub static TODO: Template<crate::reading::TodoPage> = Template::new(
    "/todo.typ",
    &[
        ("/todo.typ", include_str!("rendering/todo.typ")),
        ("/page.typ", include_str!("rendering/page.typ")),
    ],
    Library::Open,
    Output::Body,
    Kept::Process,
    &[],
);

/// The calendar's days.
pub static CALENDAR: Template<crate::reading::CalendarPage> = Template::new(
    "/calendar.typ",
    &[
        ("/calendar.typ", include_str!("rendering/calendar.typ")),
        ("/page.typ", include_str!("rendering/page.typ")),
    ],
    Library::Open,
    Output::Body,
    Kept::Process,
    &[],
);

/// A page of the static site, the whole document, written once to a file.
pub static SITE: Template<crate::site::page::SitePage> = Template::new(
    "/site.typ",
    &[("/site.typ", include_str!("rendering/site.typ"))],
    Library::Open,
    Output::Document,
    Kept::Nowhere,
    &[],
);

/// The recorded decisions, the whole document, in the closed world, since a body is a writer's
/// markup.
pub static DECISIONS: Template<crate::decisions::DecisionsPage> = Template::new(
    "/decisions.typ",
    &[("/decisions.typ", include_str!("rendering/decisions.typ"))],
    Library::Closed,
    Output::Document,
    Kept::Nowhere,
    &[],
);

/// Every template, for the build to compile against its type's examples.
#[must_use]
pub fn all() -> [&'static dyn Checked; 6] {
    [&MARKUP, &ANSWER, &TODO, &CALENDAR, &SITE, &DECISIONS]
}

#[cfg(test)]
mod tests {
    use super::LINKED;

    /// The typesetter named here is the one `app/Cargo.lock` links, so a bump of either moves
    /// every template's key and never leaves a stale value under the old one.
    #[test]
    fn the_typesetter_is_the_one_cargo_lock_links() {
        let lock = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../Cargo.lock"))
            .expect("app/Cargo.lock");
        let field = |name: &str, field: &str| {
            lock.split("[[package]]")
                .find(|p| p.contains(&format!("\nname = \"{name}\"\n")))
                .and_then(|p| p.split(&format!("{field} = \"")).nth(1))
                .and_then(|s| s.split('"').next())
                .unwrap_or_else(|| panic!("{name} has a {field} in the lock"))
                .to_owned()
        };
        let source = field("prodrome-typst-wasm", "source");
        let rev = source
            .rsplit('#')
            .next()
            .expect("a git source names its commit");
        let linked = format!(
            "prodrome-typst-wasm {rev}, typst {}",
            field("typst", "version")
        );
        assert_eq!(LINKED, linked, "set rendering::LINKED to {linked:?}");
    }
}
