//! The upload index on disk: one entry per upload a live turn names, `<id>.json` in its own
//! directory beside the uploads. It is box-local and never pushed, as the uploads are, and it is
//! a cache: deleting it loses nothing, because the next refresh reads every upload again.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use suzatary_core::attachment::AttachmentId;
use suzatary_core::capability::{IndexError, UploadIndex};
use suzatary_core::enclosed::Enclosure;
use suzatary_core::file_read::{FileRead, Withheld};
use suzatary_core::secretary::upload_standing::UploadStanding;
use suzatary_core::store::{Doc as _, Gets as _, StoreError};

use crate::attachments::{AttachmentStore, Place, Stored};
use crate::docs::{DocCodec, Docs};
use crate::present::Present as _;

/// The index over one upload store.
#[derive(Debug, Clone)]
pub struct UploadIndexFiles {
    store: AttachmentStore,
    dir: PathBuf,
    entries: Docs<AttachmentId, Entry>,
}

/// One upload's words, and where the store kept it when they were read: an upload the owner
/// later keeps private is read again from its private note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    place: Place,
    text: String,
}

impl UploadIndexFiles {
    /// The index of `store`, its entries under `dir`, which the first write makes.
    #[must_use]
    pub fn new(dir: PathBuf, store: AttachmentStore) -> Self {
        let root = dir.clone();
        Self {
            store,
            dir,
            entries: Docs::new(
                "upload index",
                move |id: &AttachmentId| root.join(format!("{id}.json")),
                &DocCodec::JSON,
            )
            .private(),
        }
    }

    /// Every upload the index holds an entry for; a file of another name is not one.
    fn indexed(&self) -> Result<Vec<AttachmentId>, StoreError> {
        let unreadable = |failed: std::io::Error| {
            StoreError::unreadable("upload index", format!("{}: {failed}", self.dir.display()))
        };
        let Some(listing) = std::fs::read_dir(&self.dir).present().map_err(unreadable)? else {
            return Ok(Vec::new());
        };
        let mut ids = Vec::new();
        for entry in listing {
            let name = entry.map_err(unreadable)?.file_name();
            if let Some(id) = name
                .to_str()
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(|id| id.parse().ok())
            {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// One upload's standing, its entry written when it is indexed and removed when it is not.
    fn standing(&self, id: &AttachmentId) -> Result<UploadStanding, IndexError> {
        let Some(place) = self.store.place(id)? else {
            self.entries.remove(id)?;
            return Ok(UploadStanding::Erased);
        };
        match self.entries.get(id)? {
            Some(entry) if entry.place == place => return Ok(UploadStanding::Indexed(entry.text)),
            Some(_) | None => {}
        }
        let standing = UploadStanding::of(self.store.get(id)?, |pdf| self.store.layer(pdf))
            .map_err(|why| IndexError::Unread {
                id: id.clone(),
                why,
            })?;
        match &standing {
            UploadStanding::Indexed(text) => self.entries.set(
                id,
                &Entry {
                    place,
                    text: text.clone(),
                },
            )?,
            UploadStanding::Held | UploadStanding::Erased => self.entries.remove(id)?,
        }
        Ok(standing)
    }
}

impl UploadIndex for UploadIndexFiles {
    fn refresh(
        &self,
        named: &BTreeSet<AttachmentId>,
    ) -> Result<BTreeMap<AttachmentId, UploadStanding>, IndexError> {
        for id in self.indexed()? {
            if !named.contains(&id) {
                self.entries.remove(&id)?;
            }
        }
        named
            .iter()
            .map(|id| Ok((id.clone(), self.standing(id)?)))
            .collect()
    }

    fn open(&self, id: &AttachmentId) -> Result<Option<FileRead>, IndexError> {
        let unread = |why| IndexError::Unread {
            id: id.clone(),
            why,
        };
        Ok(match self.store.attachment(id)? {
            None => None,
            Some(Stored::Here { attachment, bytes }) => {
                let layer = match attachment.media_type {
                    Enclosure::Pdf => Some(self.store.layer(&bytes).map_err(unread)?),
                    Enclosure::Image(_) => None,
                };
                Some(FileRead::of(
                    attachment.media_type.as_str(),
                    bytes,
                    layer.as_deref(),
                ))
            }
            // The owner kept it private because its text names a held number.
            Some(Stored::Private { copy, .. }) => Some(FileRead::Text {
                text: suzatary_core::redact::redact(&copy.text),
                truncated: false,
                withheld: Some(Withheld::HeldNumber),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdftotext::{Pdftotext, minimal_pdf};
    use suzatary_core::capability::DriveFailure;
    use suzatary_core::vault::Vault;

    /// A Drive that keeps nothing and answers where it would have.
    struct Kept;

    impl Vault for Kept {
        fn keep(
            &self,
            name: &str,
            _bytes: &[u8],
        ) -> Result<suzatary_core::drive::DrivePath, DriveFailure> {
            suzatary_core::vault::folder()
                .join(name)
                .map_err(|failed| DriveFailure::Refused(failed.to_string()))
        }
    }

    /// An index over a scratch upload store, or `None` where `pdftotext` is missing.
    fn index() -> Option<(tempfile::TempDir, AttachmentStore, UploadIndexFiles)> {
        if !crate::tool::on_path("pdftotext") {
            eprintln!("skipping: pdftotext is not on PATH");
            return None;
        }
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = AttachmentStore::new(dir.path().join("attachments"), Pdftotext::new("."));
        let index = UploadIndexFiles::new(dir.path().join("upload-index"), store.clone());
        Some((dir, store, index))
    }

    fn one(id: &AttachmentId) -> BTreeSet<AttachmentId> {
        BTreeSet::from([id.clone()])
    }

    fn text(standing: Option<&UploadStanding>) -> &str {
        match standing {
            Some(UploadStanding::Indexed(text)) => text,
            other => panic!("not indexed: {other:?}"),
        }
    }

    /// A PDF on the box is indexed by its text, read once: a second refresh answers the entry.
    #[test]
    fn a_pdf_is_indexed_once_and_answered_from_its_entry() {
        let Some((dir, store, index)) = index() else {
            return;
        };
        let id = store
            .put(&minimal_pdf("Lease for unit 4"))
            .expect("kept")
            .id;
        let first = index.refresh(&one(&id)).expect("refreshed");
        assert!(text(first.get(&id)).contains("Lease for unit 4"));
        let entry = dir.path().join("upload-index").join(format!("{id}.json"));
        let written = std::fs::read_to_string(&entry).expect("an entry");
        std::fs::write(&entry, written.replace("unit 4", "unit 9")).expect("rewritten");
        let second = index.refresh(&one(&id)).expect("refreshed again");
        assert!(
            text(second.get(&id)).contains("unit 9"),
            "the entry answers, the PDF is not read again"
        );
        assert!(matches!(
            index.open(&id).expect("opened"),
            Some(FileRead::File(_))
        ));
    }

    /// A PDF naming a held number that reached the store is held, with no entry, until the
    /// owner keeps it private; then it is indexed by its redacted text.
    #[test]
    fn a_held_pdf_is_not_indexed_until_it_is_kept_private() {
        let Some((dir, store, index)) = index() else {
            return;
        };
        // The test page holds one short line.
        let w2 = minimal_pdf("SSN 123-45-6789");
        // As a store holds one kept before the rule it breaks.
        let id = crate::blobs::Blobs::open(dir.path().join("attachments"))
            .put(&w2)
            .expect("kept");
        let held = index.refresh(&one(&id)).expect("refreshed");
        assert_eq!(held.get(&id), Some(&UploadStanding::Held));
        assert!(
            !dir.path()
                .join("upload-index")
                .join(format!("{id}.json"))
                .exists(),
            "nothing of a held upload is written"
        );

        store
            .keep_private(&w2, "w2.pdf", &Kept)
            .expect("kept private");
        let released = index.refresh(&one(&id)).expect("refreshed");
        let words = text(released.get(&id));
        assert!(words.contains("SSN [redacted]"), "{words}");
        assert!(!words.contains("6789"), "{words}");
        match index.open(&id).expect("opened") {
            Some(FileRead::Text {
                text,
                withheld: Some(Withheld::HeldNumber),
                ..
            }) => assert!(!text.as_str().contains("6789")),
            other => panic!("a private file is its redacted text: {other:?}"),
        }
    }

    /// Two refreshes at once, as two rounds searching together run them, answer the same and
    /// leave one entry per upload: an entry is a function of its upload, replaced whole.
    #[test]
    fn two_refreshes_at_once_agree() {
        let Some((_dir, store, index)) = index() else {
            return;
        };
        let named: BTreeSet<AttachmentId> = (0..8)
            .map(|n| {
                store
                    .put(&minimal_pdf(&format!("Lease number {n}")))
                    .expect("kept")
                    .id
            })
            .collect();
        let (first, second) = std::thread::scope(|scope| {
            let first = scope.spawn(|| index.refresh(&named).expect("refreshed"));
            let second = scope.spawn(|| index.refresh(&named).expect("refreshed"));
            (
                first.join().expect("the first refresh"),
                second.join().expect("the second refresh"),
            )
        });
        assert_eq!(first, second);
        assert_eq!(first.len(), 8);
        let mut indexed = index.indexed().expect("listed");
        indexed.sort();
        assert_eq!(indexed, named.into_iter().collect::<Vec<_>>());
    }

    /// An upload no live turn names leaves the index, and so does one the store no longer holds.
    #[test]
    fn an_erased_or_deleted_upload_leaves_the_index() {
        let Some((dir, store, index)) = index() else {
            return;
        };
        let lease = store
            .put(&minimal_pdf("Lease for unit 4"))
            .expect("kept")
            .id;
        let deed = store
            .put(&minimal_pdf("Deed of the house"))
            .expect("kept")
            .id;
        let both = BTreeSet::from([lease.clone(), deed.clone()]);
        index.refresh(&both).expect("refreshed");
        assert_eq!(index.indexed().expect("listed").len(), 2);

        let after = index.refresh(&one(&deed)).expect("refreshed");
        assert_eq!(index.indexed().expect("listed"), vec![deed.clone()]);
        assert!(
            !after.contains_key(&lease),
            "an erased upload has no standing"
        );

        std::fs::remove_file(dir.path().join("attachments").join(deed.as_str())).expect("deleted");
        let gone = index.refresh(&one(&deed)).expect("refreshed");
        assert_eq!(gone.get(&deed), Some(&UploadStanding::Erased));
        assert!(index.indexed().expect("listed").is_empty());
        assert_eq!(index.open(&deed).expect("opened"), None);
    }
}
