//! What the model's CLI keeps of each sessioned round, as an erasure reaches it.

use crate::capability::SessionId;

/// The model CLI's session files: every turn a session was sent and every answer, in the clear,
/// so a session is a copy of its conversation's words and goes before the keys that open them.
pub trait ModelSessions: Sync {
    /// Delete every file the CLI keeps for session `id`, opened in directory `cwd`; deleting a
    /// session it keeps nothing of succeeds.
    ///
    /// # Errors
    /// Why a file may still be there: the keys of the erased words are then kept.
    fn forget(&self, id: &SessionId, cwd: &str) -> Result<(), String>;
}
