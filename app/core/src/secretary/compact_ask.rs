//! What the compactor is asked: the log as it stands and one node's children, each rendered as
//! one line of text with no id and no object name, and every answer it gave that did not fit.
//!
//! A call renders as its verb's name and never its arguments ([`digest_view::rendered`]), so a
//! remember card's words, which a `remember` call carries and every later line names only by a
//! salted name, never reach a compactor. The log, while over its budget, keeps its earliest
//! parts and says how many later ones it left out, so its first lines, the prefix a model's cache
//! keeps, stay as they were. It hides the words of every tainted part, so a clean node's summary is
//! written from clean words alone, and for a node of nothing but the owner's words it hides every
//! other part too, so that summary is written from the owner's words alone: either way its label,
//! the join of its lines', is true of what the compactor read.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use prodrome_core::event::Hash;

use crate::secretary::digest::Digest;
use crate::secretary::digest_label::Label;
use crate::secretary::digest_line::NODE_BYTES;
use crate::secretary::digest_view::{self, Keep, VIEW_BYTES, rendered};
use crate::secretary::store::Entry;

/// The compactor's rules, the file the code sends: `app/core/src/secretary/compact.md`.
pub const PROMPT: &str = include_str!("compact.md");

/// One question to the compactor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    log: Vec<String>,
    span: Vec<String>,
    tries: Vec<Try>,
}

/// An answer that did not fit, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Try {
    /// What was said, its spaces joined.
    pub answer: String,
    /// Why it is no summary.
    pub unfit: Unfit,
}

/// Why an answer is no summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unfit {
    /// Longer than a node holds: its length, and the answer cut at the limit.
    Long {
        /// Its bytes.
        bytes: usize,
        /// Its first [`NODE_BYTES`] bytes, at a character's edge.
        cut: String,
    },
    /// No line of words: nothing, or what the redaction door refuses.
    NotALine,
}

impl Ask {
    /// The log `log` and the span `span`, each a line per element, nothing tried yet.
    #[must_use]
    pub fn new(log: Vec<String>, span: Vec<String>) -> Self {
        Self {
            log,
            span,
            tries: Vec::new(),
        }
    }

    /// The span's lines, in order.
    #[must_use]
    pub fn span(&self) -> &[String] {
        &self.span
    }

    /// The answers already given that did not fit, in order.
    #[must_use]
    pub fn tries(&self) -> &[Try] {
        &self.tries
    }

    /// The same question, with `answer` tried and found `unfit`.
    #[must_use]
    pub fn tried(mut self, answer: String, unfit: Unfit) -> Self {
        self.tries.push(Try { answer, unfit });
        self
    }

    /// What the compactor reads after [`PROMPT`], as plain text with no speaker labelled: the
    /// log and the span, then each answer that did not fit and why. The log comes first and
    /// changes least between questions, so it is the prefix a model's cache can keep.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = format!(
            "<log>\n{}\n</log>\n\n<span>\n{}\n</span>\n",
            self.log.join("\n"),
            self.span.join("\n")
        );
        for tried in &self.tries {
            let why = match &tried.unfit {
                Unfit::Long { bytes, cut } => format!(
                    "That line is {bytes} bytes, and a line holds at most {NODE_BYTES}. Cut at \
                     the limit, it reads:\n<cut>\n{cut}\n</cut>"
                ),
                Unfit::NotALine => "That is not one line of words.".to_owned(),
            };
            let _ = write!(
                text,
                "\nAn earlier answer was:\n<answer>\n{}\n</answer>\n{why}\nSay the whole span \
                 again in one line of at most {NODE_BYTES} bytes.\n",
                tried.answer
            );
        }
        text
    }

    /// What sending it costs: the bytes of [`PROMPT`] and of [`Ask::text`].
    #[must_use]
    pub fn bytes(&self) -> u64 {
        u64::try_from(PROMPT.len() + self.text().len()).unwrap_or(u64::MAX)
    }

    /// The question for `node` of `digest`, whose lines are `lines`, and what it is written
    /// from: each child's line, or the object of the summary of it that is shown. `None` for a
    /// node the tree does not hold or a child with no summary to show.
    #[must_use]
    pub fn of(
        digest: &Digest,
        entries: &std::collections::BTreeMap<&Hash, &crate::secretary::store::Entry>,
        node: &Hash,
    ) -> Option<(Self, Vec<Hash>)> {
        let tree = digest.tree()?;
        let asked = tree.node(node)?;
        // An owner's node is read beside the owner's words alone, so its summary, labelled as
        // nothing but theirs, is written from nothing but theirs.
        let reads: fn(&Label) -> bool = if asked.measure().is_owners() {
            Label::is_owners
        } else {
            Label::is_clean
        };
        let mut span = Vec::new();
        let mut over = Vec::new();
        for child in asked.children() {
            if child.is_leaf() {
                span.push(rendered(entries.get(digest.line(child.name()))?));
                over.push(child.name().clone());
            } else {
                span.push(digest.summary(child.name())?.as_str().to_owned());
                over.push(digest.candidates(child.name()).first()?.object.clone());
            }
        }
        Some((Self::new(log(digest, entries, reads), span), over))
    }
}

/// The view under [`VIEW_BYTES`] as [`digest_view::view`] shows it to a reader that `reads`, its
/// earliest parts kept while the rendering is over the budget.
fn log(
    digest: &Digest,
    entries: &BTreeMap<&Hash, &Entry>,
    reads: fn(&Label) -> bool,
) -> Vec<String> {
    let shown = digest_view::view(digest, entries, VIEW_BYTES, reads)
        .into_iter()
        .map(|shown| shown.body.text(shown.part.span.len()))
        .collect();
    digest_view::fit(shown, VIEW_BYTES, Keep::Earliest)
}
