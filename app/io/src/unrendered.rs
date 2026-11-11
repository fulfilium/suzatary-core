//! The render cache of a reader that draws no answer: it keeps none, so forgetting one succeeds,
//! and opening it touches no directory.

use suzatary_base::answer::Renderable;
use suzatary_core::render_cache::RenderCache;

/// Keeps nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unrendered;

impl RenderCache for Unrendered {
    fn forget(&self, _block: &Renderable) -> Result<(), String> {
        Ok(())
    }
}
