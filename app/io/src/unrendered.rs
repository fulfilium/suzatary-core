//! The render cache of a reader that draws no answer: it keeps none, so forgetting one succeeds,
//! and opening it touches no directory.

use suzatary_core::capability::RenderCache;
use suzatary_core::secretary::answer::Renderable;

/// Keeps nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unrendered;

impl RenderCache for Unrendered {
    fn forget(&self, _block: &Renderable) -> Result<(), String> {
        Ok(())
    }
}
