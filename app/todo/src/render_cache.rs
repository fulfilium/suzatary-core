//! What the box keeps of the HTML a conversation's words were rendered to: a cache of a pure
//! function, emptied of a text whose words are erased.

use suzatary_base::answer::Renderable;

/// The cache of rendered answer blocks, as an erasure reaches it: a block's HTML is a function of
/// its source, so it is a copy of the words, and it goes before the key that opens them.
pub trait RenderCache: Sync {
    /// Forget whatever is kept of `block`, in memory and on disk, the removal synced; forgetting
    /// what is not kept succeeds.
    ///
    /// # Errors
    /// Why a copy on disk may still be there: the key that opens the words is then kept.
    fn forget(&self, block: &Renderable) -> Result<(), String>;
}
