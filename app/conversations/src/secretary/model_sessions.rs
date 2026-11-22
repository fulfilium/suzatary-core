//! What the model's CLI keeps of each sessioned round, as an erasure reaches it.

use crate::secretary::model::SessionId;

/// The model CLI's session files: every turn and answer in the clear, so a session goes before
/// the keys that open its conversation's words.
pub trait ModelSessions: Sync {
    /// Delete every file the CLI keeps for session `id`, opened in directory `cwd`; deleting a
    /// session it keeps nothing of succeeds.
    ///
    /// # Errors
    /// Why a file may still be there: the keys of the erased words are then kept.
    fn forget(&self, id: &SessionId, cwd: &str) -> Result<(), String>;
}
