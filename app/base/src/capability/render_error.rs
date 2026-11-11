//! Why typst would not render a fragment: the base's, since narrowing a document refuses with it.

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
