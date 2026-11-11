use std::collections::{BTreeMap, BTreeSet};

use crate::attachment::AttachmentId;
use crate::file_read::FileRead;
use crate::secretary::upload_standing::UploadStanding;
use crate::store::StoreError;

/// The upload index: the words of each upload a live turn names, kept beside the uploads and
/// rebuilt from them, so deleting it loses nothing. The interpreter is
/// `suzatary_io::upload_index::UploadIndexFiles`.
pub trait UploadIndex: Sync {
    /// Bring the index to the uploads `named`: each one's standing, read from its entry or, when
    /// the index holds none for the store's current copy, from the store and written back; every
    /// entry for an upload not named, or no longer in the store, is removed.
    ///
    /// # Errors
    /// [`IndexError`], with every entry already written left standing.
    fn refresh(
        &self,
        named: &BTreeSet<AttachmentId>,
    ) -> Result<BTreeMap<AttachmentId, UploadStanding>, IndexError>;

    /// One upload read for a model: a file on the box as itself, a private one as its redacted
    /// text, withheld; `None` when the store holds none.
    ///
    /// # Errors
    /// [`IndexError`].
    fn open(&self, id: &AttachmentId) -> Result<Option<FileRead>, IndexError>;
}

/// Why the index could not answer.
#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    /// The upload store or the index's own files failed.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A PDF's text could not be read, so whether it names an identity number is unknown.
    #[error("the text of upload {id} could not be read: {why}")]
    Unread {
        /// The upload.
        id: AttachmentId,
        /// Why.
        why: String,
    },
}
