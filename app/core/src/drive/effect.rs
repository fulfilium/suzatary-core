//! The owner's Drive for the secretary: the port, the fence a round reads it through, and one
//! file read for the model.

use crate::drive::{Fenced, Port};
pub use crate::drive_path::DrivePath;

/// Why a read produced nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DriveFailure {
    /// No such file or folder.
    #[error("not found in Drive: {0}")]
    Missing(String),
    /// A folder where a file was wanted, or a file too large to read.
    #[error("{0}")]
    Refused(String),
    /// The CLI could not be run, timed out, or failed, in its own words.
    #[error("Drive did not answer: {0}")]
    Unreachable(String),
    /// The CLI answered that its session expired or was never begun: it needs its sign-in at a
    /// terminal, in its own words.
    #[error("the Drive needs its sign-in: {0}")]
    SignedOut(String),
}

/// The owner's Drive, as the secretary's rounds and the owner's taps reach it.
pub trait Drive: Sync {
    /// The folders whose files are read without a tap, read fresh so an allowance counts at once.
    fn allowed_folders(&self) -> Vec<DrivePath>;

    /// The Drive itself, unfenced: for what a tap reads (the tap is the owner's consent) and for
    /// the entry a card names. What a round reads unasked goes through [`Drive::fenced`].
    fn port(&self) -> &dyn Port;

    /// One file read for the model, which the caller has already decided may be read; the
    /// download is deleted.
    ///
    /// # Errors
    /// [`DriveFailure`].
    fn read(&self, file: &DrivePath) -> Result<crate::file_read::FileRead, DriveFailure>;

    /// The Drive as a round reaches it unasked: downloads inside the allowed folders alone, the
    /// private folder never listed, nothing written.
    fn fenced(&self) -> Fenced<&dyn Port> {
        Fenced::new(self.port(), self.allowed_folders())
    }
}
