/// Typst would not render this fragment.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Typst is not available, or the compile failed; the frontend shows the source.
    #[error("typst did not render the fragment: {0}")]
    Refused(String),
    /// The fragment holds an element, an attribute or a URL typst never writes; refused whole.
    #[error("the rendered fragment is suspicious and was refused: {0}")]
    Suspicious(String),
}
