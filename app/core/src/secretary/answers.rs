//! The secretary's answers rendered block by block as typst's HTML.

use thiserror::Error;

/// Typst would not render this block, with its own message; the block is drawn as source.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("typst did not render the answer: {0}")]
pub struct AnswerError(pub String);

/// Rendering a finished model turn's blocks.
pub trait Answers: Sync {
    /// One [`Renderable`] block of an answer as typst's HTML, through the render boundary.
    ///
    /// # Errors
    /// Typst refused the block, could not run, or the tripwire fired.
    ///
    /// [`Renderable`]: suzatary_base::answer::Renderable
    fn block(
        &self,
        block: &suzatary_base::answer::Renderable,
    ) -> Result<crate::render::Html, AnswerError>;
}
