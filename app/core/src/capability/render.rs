//! Turning markup into the HTML a browser is allowed to parse.

use prodrome_core::memo::Finding;

use super::Typeset;
use crate::escape::Escaped;
use crate::template::Called;

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

/// Typst would not render this fragment.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Typst is not available, or the compile failed; the frontend shows the source.
    #[error("typst did not render the fragment: {0}")]
    Refused(String),
    /// The fragment carries an element, an attribute or a URL typst never writes; refused,
    /// never edited.
    #[error("the rendered fragment is suspicious and was refused: {0}")]
    Suspicious(String),
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
