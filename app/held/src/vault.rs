//! The owner's private files: an upload whose text names an identity number is kept in his
//! Drive, never on the box, and only his confirm puts it there. A model reads its text with
//! each number a placeholder; reading the file again or sending it is a tap.

use serde::{Deserialize, Serialize};

use suzatary_base::drive_path::DriveFailure;
use suzatary_base::drive_path::DrivePath;

/// Where a private file is kept, and what a model may read of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivateCopy {
    /// Its path in the owner's Drive, inside [`FOLDER`].
    pub path: DrivePath,
    /// Its size in bytes.
    pub size: u64,
    /// Its text with each identity number a placeholder naming the upload (`crate::held_value`);
    /// `""` for an image.
    pub text: String,
}

/// The folder private files are kept in. `suzatary_services::drive::allowed` never answers yes inside
/// it, whatever `drive_allow` says, so reading one is always a tap.
pub const FOLDER: &str = "/my-files/Suzatary/Private";

/// The folder, parsed.
///
/// # Panics
/// Never: [`FOLDER`] is a valid path, which a test pins.
#[must_use]
pub fn folder() -> DrivePath {
    FOLDER.parse().expect("FOLDER is a Drive path")
}

/// Whether `path` is the private folder or under it, its names compared without case: a Drive
/// that matches names case-insensitively would read `/my-files/Suzatary/PRIVATE` as the folder,
/// so a check that keeps the model out of it folds case, and refuses more, never less.
#[must_use]
pub fn holds(path: &DrivePath) -> bool {
    path.is_under_ignoring_case(&folder())
}

/// What keeps a file in the owner's Drive. Held in no record: the owner's confirm on an upload
/// is its one caller.
pub trait Vault: Send + Sync {
    /// Keep `bytes` in [`FOLDER`] under `name`, one path segment, answering where.
    ///
    /// # Errors
    /// [`DriveFailure`].
    fn keep(&self, name: &str, bytes: &[u8]) -> Result<DrivePath, DriveFailure>;
}
