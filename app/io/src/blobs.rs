//! Content-addressed files, one per content and named by its SHA-256, as a [`Doc`]: the owner's
//! uploads and the files a verb hands the model, box-local and git-ignored beside the
//! conversations. A turn names a file, never holds it.
//!
//! [`Doc`]: suzatary_core::store::Doc

use std::path::PathBuf;

use suzatary_core::attachment::AttachmentId;
use suzatary_core::store::{Doc as _, Gets as _, StoreError};

use crate::docs::{DocCodec, Docs};

/// Files by their digest.
pub type Blobs = Docs<AttachmentId, Vec<u8>>;

impl Blobs {
    /// The files under `dir`, made on the first write.
    #[must_use]
    pub fn open(dir: PathBuf) -> Self {
        Docs::new(
            "file store",
            move |id: &AttachmentId| dir.join(id.as_str()),
            &CODEC,
        )
    }

    /// Keep these bytes under their digest: keeping the same content twice is the same file.
    ///
    /// # Errors
    /// [`StoreError::Unwritable`].
    pub fn put(&self, bytes: &[u8]) -> Result<AttachmentId, StoreError> {
        let id = AttachmentId::of(bytes);
        self.set(&id, &bytes.to_vec())?;
        Ok(id)
    }

    /// The bytes by that name.
    ///
    /// # Errors
    /// [`StoreError::Absent`] for none, or for a file swapped under the store, and
    /// [`StoreError::Unreadable`].
    pub fn read(&self, id: &AttachmentId) -> Result<Vec<u8>, StoreError> {
        self.get(id)?.ok_or_else(|| StoreError::Absent {
            what: "file",
            name: id.to_string(),
        })
    }
}

/// The name is the bytes' digest, so a file swapped under the store reads as absent, never as
/// something else.
const CODEC: DocCodec<AttachmentId, Vec<u8>> = DocCodec {
    print: |id, bytes| {
        if &AttachmentId::of(bytes) == id {
            Ok(bytes.clone())
        } else {
            Err(format!("these bytes are not {id}"))
        }
    },
    parse: |id, bytes| Ok((&AttachmentId::of(&bytes) == id).then_some(bytes)),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_kept_once_and_read_back_by_its_digest() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = Blobs::open(dir.path().join("files"));
        let first = store.put(b"%PDF-1.7 a").expect("kept");
        assert_eq!(store.put(b"%PDF-1.7 a").expect("kept again"), first);
        assert_eq!(store.read(&first).expect("read back"), b"%PDF-1.7 a");
        let files = std::fs::read_dir(dir.path().join("files"))
            .expect("the store exists")
            .count();
        assert_eq!(files, 1, "one file, no temporaries left behind");
    }

    #[test]
    fn a_swapped_or_absent_file_is_absent() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = Blobs::open(dir.path().to_path_buf());
        let id = store.put(b"%PDF-1.7 a").expect("kept");
        std::fs::write(dir.path().join(id.as_str()), b"%PDF-1.7 b").expect("swapped");
        assert!(matches!(store.read(&id), Err(StoreError::Absent { .. })));
        let absent = AttachmentId::of(b"never kept");
        assert!(matches!(
            store.read(&absent),
            Err(StoreError::Absent { .. })
        ));
    }
}
