//! The owner's uploads: one file per photo or PDF, named by its SHA-256, beside the
//! conversations. Box-local and git-ignored like the transcripts; a turn names a file, never
//! holds it.

use std::path::PathBuf;

use suzatary_core::attachment::{Attachment, AttachmentError, AttachmentId};
use suzatary_core::capability::DriveFailure;
use suzatary_core::enclosed::Enclosure;
use suzatary_core::file_read::{FileRead, Withheld};
use suzatary_core::held_value::HeldFile;
use suzatary_core::vault::{PrivateCopy, Vault};

use suzatary_core::store::{Doc, Gets, StoreError};
use suzatary_core::upload::Upload;

use crate::blobs::Blobs;
use crate::docs::{DocCodec, Docs};
use crate::pdftotext::Pdftotext;

/// Where the owner's uploads live.
#[derive(Debug, Clone)]
pub struct AttachmentStore {
    files: Blobs,
    notes: Docs<AttachmentId, Attachment>,
    pdftotext: Pdftotext,
}

/// An upload as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    /// On the box, with its bytes.
    Here {
        /// What it is.
        attachment: Attachment,
        /// Its bytes.
        bytes: Vec<u8>,
    },
    /// In the owner's Drive; the box holds only where.
    Private {
        /// What it is.
        attachment: Attachment,
        /// Where it is kept, and its redacted text.
        copy: PrivateCopy,
    },
}

impl Stored {
    /// What it is, wherever it is kept.
    #[must_use]
    pub fn attachment(&self) -> &Attachment {
        match self {
            Stored::Here { attachment, .. } | Stored::Private { attachment, .. } => attachment,
        }
    }
}

/// Where the store keeps an upload, read without reading its bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Place {
    /// Its bytes are on the box.
    Here,
    /// It is in the owner's Drive, and the box holds a note of where.
    Private,
}

/// Why an upload was not kept.
#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    /// The bytes are not a file this app accepts.
    #[error(transparent)]
    Refused(#[from] AttachmentError),
    /// A PDF a model would not be handed as itself, so it is not kept either.
    #[error("this PDF is not sent to the secretary: {0}")]
    Withheld(Withheld),
    /// `pdftotext` could not read the PDF's text, which that rule needs.
    #[error("the PDF's text could not be read: {0}")]
    Unread(String),
    /// The owner's Drive did not keep a private file.
    #[error("your Drive did not keep it: {0}")]
    Drive(DriveFailure),
    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl AttachmentStore {
    /// A store rooted here, reading a PDF's text through `pdftotext`; the directory is made
    /// on the first write.
    #[must_use]
    pub fn new(root: PathBuf, pdftotext: Pdftotext) -> Self {
        Self {
            files: Blobs::open(root.clone()),
            notes: Docs::new(
                "upload store",
                move |id: &AttachmentId| root.join(format!("{id}.private.json")),
                &NOTE,
            ),
            pdftotext,
        }
    }

    /// Accept and keep these bytes, once: storing the same file twice is one file. A PDF is
    /// kept only when [`FileRead::of`] would hand it to a model as itself, its text layer
    /// read through `pdftotext`.
    ///
    /// # Errors
    /// [`UploadError::Refused`] for bytes that are not a photo or a PDF,
    /// [`UploadError::Withheld`] and [`UploadError::Unread`] for a PDF, and the store's
    /// failures.
    pub fn put(&self, bytes: &[u8]) -> Result<Attachment, UploadError> {
        let attachment = Attachment::accept(bytes)?;
        if attachment.media_type == Enclosure::Pdf {
            let layer = self.pdftotext.text(bytes).map_err(UploadError::Unread)?;
            if let FileRead::Withheld { why, .. } = FileRead::of(
                &HeldFile::Upload {
                    id: attachment.id.clone(),
                },
                &attachment.called(),
                Enclosure::Pdf.as_str(),
                bytes.to_vec(),
                Some(&layer),
            ) {
                return Err(UploadError::Withheld(why));
            }
        }
        self.files.put(bytes)?;
        Ok(attachment)
    }

    /// Keep a file the owner said to keep private: in his Drive through `vault`, and on the
    /// box only a note of where, which [`AttachmentStore::attachment`] reads back. A PDF's
    /// text is read now and kept masked, each number a placeholder naming the upload, which is
    /// all a model is ever given of it.
    ///
    /// # Errors
    /// As [`AttachmentStore::put`], less the rule it keeps, and [`UploadError::Drive`].
    pub fn keep_private(
        &self,
        bytes: &[u8],
        name: &str,
        vault: &dyn Vault,
    ) -> Result<Attachment, UploadError> {
        let attachment = Attachment::accept(bytes)?.named(name)?;
        let text = match attachment.media_type {
            Enclosure::Pdf => {
                let layer = self.pdftotext.text(bytes).map_err(UploadError::Unread)?;
                match FileRead::of(
                    &HeldFile::Upload {
                        id: attachment.id.clone(),
                    },
                    &attachment.called(),
                    Enclosure::Pdf.as_str(),
                    bytes.to_vec(),
                    Some(&layer),
                ) {
                    FileRead::File(readable) => readable.text().as_str().to_owned(),
                    FileRead::Text { text, .. } | FileRead::Withheld { text, .. } => {
                        text.into_string()
                    }
                    FileRead::Unsupported { .. } => String::new(),
                }
            }
            Enclosure::Image(_) => String::new(),
        };
        // The digest leads the name, so two files called the same are two files, and the
        // same file kept twice is the one Drive already holds.
        let called = attachment.called();
        let drive_name = format!(
            "{} {}",
            &attachment.id.as_str()[..8],
            called.replace('/', "-")
        );
        let path = vault.keep(&drive_name, bytes).map_err(UploadError::Drive)?;
        let kept = Attachment {
            private: Some(PrivateCopy {
                path,
                size: bytes.len() as u64,
                text,
            }),
            ..attachment
        };
        self.notes.set(&kept.id, &kept)?;
        Ok(kept)
    }

    /// The file, re-read and re-typed from its bytes, or a private file's note: a file
    /// swapped under the store is absent rather than served as something else.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`], for bytes that are no longer a file this app accepts too.
    pub fn attachment(&self, id: &AttachmentId) -> Result<Option<Stored>, StoreError> {
        if let Some(kept) = self.notes.get(id)? {
            return Ok(kept.private.clone().map(|copy| Stored::Private {
                attachment: kept,
                copy,
            }));
        }
        let Some(bytes) = self.files.get(id)? else {
            return Ok(None);
        };
        let attachment = Attachment::accept(&bytes)
            .map_err(|failed| StoreError::unreadable("upload store", format!("{id}: {failed}")))?;
        Ok(Some(Stored::Here { attachment, bytes }))
    }

    /// Where the store keeps the upload, `None` when it holds none: a private note is read first,
    /// as [`AttachmentStore::attachment`] reads it.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    pub fn place(&self, id: &AttachmentId) -> Result<Option<Place>, StoreError> {
        if self.notes.get(id)?.is_some() {
            return Ok(Some(Place::Private));
        }
        match self.files.size(id) {
            Ok(_) => Ok(Some(Place::Here)),
            Err(StoreError::Absent { .. }) => Ok(None),
            Err(
                failed @ (StoreError::Taken { .. }
                | StoreError::Unreadable { .. }
                | StoreError::NotAppended { .. }
                | StoreError::Unwritable { .. }),
            ) => Err(failed),
        }
    }

    /// A PDF's text layer, read as the store read it when it kept the PDF.
    ///
    /// # Errors
    /// `pdftotext`'s sentence.
    pub fn layer(&self, pdf: &[u8]) -> Result<String, String> {
        self.pdftotext.text(pdf)
    }

    /// The file's size, before its bytes are read.
    ///
    /// # Errors
    /// As [`Docs::size`].
    pub fn size(&self, id: &AttachmentId) -> Result<u64, StoreError> {
        self.files.size(id)
    }

    /// The file's bytes.
    ///
    /// # Errors
    /// [`StoreError::Absent`] when the store holds no such file, and the filesystem's others.
    pub fn bytes(&self, id: &AttachmentId) -> Result<Vec<u8>, StoreError> {
        self.files.get(id)?.ok_or_else(|| StoreError::Absent {
            what: "upload",
            name: id.to_string(),
        })
    }
}

/// The uploads as the secretary reads them; it keeps none, since an upload is kept by the
/// owner's route alone.
impl Gets<AttachmentId, Upload> for AttachmentStore {
    fn get(&self, id: &AttachmentId) -> Result<Option<Upload>, StoreError> {
        Ok(self.attachment(id)?.map(|stored| match stored {
            Stored::Here { attachment, bytes } => Upload::Here {
                media: attachment.media_type,
                bytes,
            },
            Stored::Private { attachment, copy } => Upload::Private {
                media: attachment.media_type,
                name: attachment.name.unwrap_or_default(),
                copy,
            },
        }))
    }
}

/// A private file's note, whose id must be its own and which must name where it is kept.
const NOTE: DocCodec<AttachmentId, Attachment> = DocCodec {
    print: |_, kept| serde_json::to_vec(kept).map_err(|failed| failed.to_string()),
    parse: |id, bytes| {
        let kept: Attachment =
            serde_json::from_slice(&bytes).map_err(|failed| failed.to_string())?;
        Ok((&kept.id == id && kept.private.is_some()).then_some(kept))
    },
};

/// A file sent as base64 text, which is how a JSON upload carries it.
///
/// # Errors
/// [`AttachmentError::Unreadable`] for text that is not base64.
pub fn decoded(text: &str) -> Result<Vec<u8>, AttachmentError> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|_| AttachmentError::Unreadable)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3];

    #[test]
    fn a_photo_is_stored_once_and_read_back_by_its_name() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = AttachmentStore::new(dir.path().join("attachments"), Pdftotext::new("."));
        let first = store.put(PNG).expect("stored");
        let again = store.put(PNG).expect("stored again");
        assert_eq!(first, again);
        assert_eq!(store.bytes(&first.id).expect("read back"), PNG);
        let files = std::fs::read_dir(dir.path().join("attachments"))
            .expect("the store exists")
            .count();
        assert_eq!(files, 1, "one file, no temporaries left behind");
    }

    #[test]
    fn a_refused_or_missing_photo_is_named_not_guessed() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = AttachmentStore::new(
            dir.path().to_path_buf(),
            crate::pdftotext::Pdftotext::new("."),
        );
        assert!(matches!(
            store.put(b"<svg/>"),
            Err(UploadError::Refused(AttachmentError::Unreadable))
        ));
        let absent = AttachmentId::of(b"never stored");
        assert!(matches!(
            store.bytes(&absent),
            Err(StoreError::Absent { .. })
        ));
    }

    #[test]
    fn a_pdf_is_kept_unless_a_model_would_not_be_handed_it() {
        if !crate::tool::on_path("pdftotext") {
            eprintln!("skipping: pdftotext is not on PATH");
            return;
        }
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = AttachmentStore::new(
            dir.path().to_path_buf(),
            crate::pdftotext::Pdftotext::new("."),
        );
        let plain = crate::pdftotext::minimal_pdf("Lease for unit 4");
        let kept = store.put(&plain).expect("an ordinary PDF");
        assert_eq!(kept.media_type, Enclosure::Pdf);
        let naming = crate::pdftotext::minimal_pdf("SSN 123-45-6789");
        assert!(matches!(
            store.put(&naming),
            Err(UploadError::Withheld(Withheld::HeldNumber))
        ));
        assert!(matches!(
            store.bytes(&AttachmentId::of(&naming)),
            Err(StoreError::Absent { .. })
        ));
    }

    /// A Drive that keeps what it is handed, in memory.
    #[derive(Default)]
    struct FakeVault(std::sync::Mutex<Vec<(String, Vec<u8>)>>);

    impl Vault for FakeVault {
        fn keep(
            &self,
            name: &str,
            bytes: &[u8],
        ) -> Result<suzatary_core::drive::DrivePath, DriveFailure> {
            self.0
                .lock()
                .expect("unpoisoned")
                .push((name.to_owned(), bytes.to_vec()));
            suzatary_core::vault::folder()
                .join(name)
                .map_err(|failed| DriveFailure::Refused(failed.to_string()))
        }
    }

    /// A private file leaves only a note on the box: where it is, and its text masked.
    #[test]
    fn a_private_file_goes_to_the_vault_and_the_box_keeps_only_where() {
        if !crate::tool::on_path("pdftotext") {
            eprintln!("skipping: pdftotext is not on PATH");
            return;
        }
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = AttachmentStore::new(dir.path().to_path_buf(), Pdftotext::new("."));
        let vault = FakeVault::default();
        // The test page holds one short line.
        let w2 = crate::pdftotext::minimal_pdf("SSN 123-45-6789");
        let kept = store
            .keep_private(&w2, "w2.pdf", &vault)
            .expect("kept private");
        let id = AttachmentId::of(&w2);
        let short = &id.as_str()[..8];
        assert_eq!(
            vault.0.lock().expect("unpoisoned").as_slice(),
            [(format!("{short} w2.pdf"), w2.clone())]
        );
        let copy = kept.private.clone().expect("private");
        assert_eq!(
            copy.path.to_string(),
            format!("/my-files/Suzatary/Private/{short} w2.pdf")
        );
        assert!(!copy.text.contains("123-45-6789"), "{}", copy.text);
        assert!(
            copy.text
                .contains("[SSN ending 6789 · tap to show · held:file:"),
            "{}",
            copy.text
        );
        assert_eq!(
            store.attachment(&id).expect("its note"),
            Some(Stored::Private {
                attachment: kept,
                copy
            })
        );
        assert!(
            matches!(store.bytes(&id), Err(StoreError::Absent { .. })),
            "no bytes on the box"
        );
        let on_disk = std::fs::read_to_string(dir.path().join(format!("{id}.private.json")))
            .expect("the note");
        assert!(!on_disk.contains("123-45-6789"), "{on_disk}");
    }
}
