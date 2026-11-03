//! One page of the static site as the value its template (`rendering::SITE`) draws, and the
//! vocabulary it is built in.

use prodrome_core::event::Hash;
use schemars::JsonSchema;
use serde::Serialize;

/// A page of the site: its title, the way back to the site's root, its body and its footer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SitePage {
    /// The title.
    pub title: String,
    /// What leads from the page back to the site's root: "", or "../" per level.
    pub prefix: String,
    /// The line it ends on.
    pub footer: String,
    /// What it shows.
    pub body: Vec<Expr>,
}

impl SitePage {
    /// `body` inside the shell every page shares.
    pub(super) fn of(title: &str, prefix: &str, footer: &str, body: Vec<Expr>) -> Self {
        Self {
            title: title.to_owned(),
            prefix: prefix.to_owned(),
            footer: footer.to_owned(),
            body,
        }
    }

    /// The page that stands where the tripwire refused one: it names the refusal instead of
    /// dropping the page.
    #[must_use]
    pub fn refused(path: &str, reason: &str, footer: &str) -> Self {
        let prefix = "../".repeat(path.matches('/').count());
        Self::of(
            path,
            &prefix,
            footer,
            vec![
                tag(Tag::H1, [lit(path)]),
                tag(
                    Tag::P,
                    [lit(&format!(
                        "Refused: typst's HTML for this page carried {reason}, so it was not written. \
                         Its objects are under objects/, and the chain page lists them."
                    ))],
                ),
            ],
        )
    }
}

/// The elements a page is built of. A page has no stylesheet and no script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Tag {
    /// `h1`.
    H1,
    /// `h2`.
    H2,
    /// `h3`.
    H3,
    /// `p`.
    P,
    /// `span`, children side by side.
    Span,
    /// `pre`.
    Pre,
    /// `code`.
    Code,
    /// `ul`.
    Ul,
    /// `li`.
    Li,
    /// `article`.
    Article,
}

/// One name and its value in a definition list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Fact {
    /// The name.
    pub name: String,
    /// The value.
    pub value: Expr,
}

/// A part of a page. Every string is data, drawn as text: the one way a stranger's words enter
/// a page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Expr {
    /// Text.
    Text {
        /// The words.
        text: String,
    },
    /// An element around its children.
    Tag {
        /// The element.
        tag: Tag,
        /// Its children.
        kids: Vec<Expr>,
    },
    /// Strong text.
    Strong {
        /// What is strong.
        kids: Vec<Expr>,
    },
    /// A link.
    Link {
        /// Where to, relative to the page.
        href: String,
        /// Its label.
        kids: Vec<Expr>,
    },
    /// A hash, broken every eight characters with a `<wbr>`, which copies as nothing, so it fits
    /// a phone.
    Hash {
        /// The hash.
        hash: String,
    },
    /// A definition list.
    Facts {
        /// Its names and values.
        pairs: Vec<Fact>,
    },
    /// A table with a header row.
    Rows {
        /// The header.
        head: Vec<String>,
        /// The rows, a cell each column.
        rows: Vec<Vec<Expr>>,
    },
}

/// A string, as text.
pub(super) fn lit(s: &str) -> Expr {
    Expr::Text { text: s.to_owned() }
}

/// `<tag>` around its children.
pub(super) fn tag(tag: Tag, kids: impl IntoIterator<Item = Expr>) -> Expr {
    Expr::Tag {
        tag,
        kids: kids.into_iter().collect(),
    }
}

/// Children side by side, in a `<span>`.
pub(super) fn span(kids: impl IntoIterator<Item = Expr>) -> Expr {
    tag(Tag::Span, kids)
}

pub(super) fn strong(kid: Expr) -> Expr {
    Expr::Strong { kids: vec![kid] }
}

pub(super) fn link(href: &str, label: Expr) -> Expr {
    Expr::Link {
        href: href.to_owned(),
        kids: vec![label],
    }
}

pub(super) fn hash(h: &Hash) -> Expr {
    Expr::Hash {
        hash: h.as_str().to_owned(),
    }
}

/// Hashes separated by commas.
pub(super) fn hashes(hs: &[Hash]) -> Expr {
    span(
        hs.iter()
            .enumerate()
            .flat_map(|(i, h)| (i > 0).then(|| lit(", ")).into_iter().chain([hash(h)])),
    )
}

/// A definition list.
pub(super) fn facts(pairs: Vec<(&str, Expr)>) -> Expr {
    Expr::Facts {
        pairs: pairs
            .into_iter()
            .map(|(name, value)| Fact {
                name: name.to_owned(),
                value,
            })
            .collect(),
    }
}

/// A table with a header row.
pub(super) fn rows(head: &[&str], body: impl IntoIterator<Item = Vec<Expr>>) -> Expr {
    Expr::Rows {
        head: head.iter().map(|h| (*h).to_owned()).collect(),
        rows: body.into_iter().collect(),
    }
}
