//! Turning markup into the HTML a browser is allowed to parse.

use prodrome_core::memo::Finding;

use crate::rendering::CLOSED;
use crate::template::{Called, named};
use suzatary_base::capability::{Rejected, RenderError, Typeset};
use suzatary_base::escape::Escaped;
use suzatary_base::table::Table;

/// Typst's own HTML for a fragment of markup; only a [`Render`] can make one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Html(String);

impl Html {
    /// The interpreter's word that typst produced this and the tripwire let it through.
    #[must_use]
    pub fn from_render(html: impl Into<String>) -> Self {
        Self(html.into())
    }

    /// The HTML, for the wire.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Turning markup into the HTML a browser is allowed to parse.
///
/// It takes an [`Escaped`], which only confirmed content yields, so rendered means confirmed.
/// A renderer is a [`Typeset`] too: a fragment is compiled in the closed world, the same one
/// the markup door admits a text through.
pub trait Render: Typeset {
    /// Render one fragment, compiling it if this box has never seen it.
    fn html(&self, markup: &Escaped) -> Result<Html, RenderError>;

    /// The rendered fragment if it is already known, never a compile, so a request never
    /// spawns one subprocess per todo.
    fn cached(&self, markup: &Escaped) -> Option<Html>;

    /// Render every fragment not cached yet, in this thread, and answer how many now have HTML.
    fn warm(&self, markup: &[Escaped]) -> usize;

    /// Render a page: a template called with the value it draws ([`crate::reading`]), compiled if unseen.
    fn page(&self, page: &Called) -> Result<Html, RenderError>;

    /// Every key this renderer's tabulation holds two values for: a render that was not a
    /// function of its source, or a value another writer left. None, for a renderer that keeps
    /// nothing.
    fn findings(&self) -> Vec<Finding> {
        Vec::new()
    }
}

/// A typesetter that compiles each text once while it keeps the answer: the closed world is a
/// function of the text alone ([`crate::rendering::CLOSED`]), so its answer is tabulated by the text.
#[derive(Debug)]
pub struct Remembered<T> {
    typeset: T,
    answers: Table<Result<String, Rejected>>,
}

impl<T: Typeset> Remembered<T> {
    /// `typeset`, keeping at most `capacity` answers.
    #[must_use]
    pub fn new(typeset: T, capacity: usize) -> Self {
        Self {
            typeset,
            answers: Table::new(capacity),
        }
    }

    /// Every text whose tabulation holds two answers.
    #[must_use]
    pub fn findings(&self) -> Vec<Finding> {
        self.answers.findings()
    }
}

impl<T: Typeset> Typeset for Remembered<T> {
    fn closed(&self, source: &str) -> Result<String, Rejected> {
        let key = prodrome_core::memo::Key {
            function: CLOSED.clone(),
            argument: named(source),
        };
        if let Some(known) = self.answers.kept(&key) {
            return known;
        }
        let answer = self.typeset.closed(source);
        // A refusal typst located is a fact about the text; one about nothing (a timeout, a
        // child that would not start) may be about the box, so it is asked again next time.
        if answer
            .as_ref()
            .map_or_else(|r| r.diagnostics.iter().any(|d| d.span.is_some()), |_| true)
        {
            let _ = self.answers.keep(key, answer.clone());
        }
        answer
    }
}
