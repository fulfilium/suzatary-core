//! The photos and PDFs one request carries, as the `claude` CLI's stream-json blocks.

use serde_json::{Value, json};
use suzatary_core::attachment::{Attachment, AttachmentId};
use suzatary_core::enclosed::{Enclosed, Enclosure};
use suzatary_core::for_model::ForModel;
use suzatary_core::secretary::model::{ModelError, Turn};

use crate::attachments::AttachmentStore;
use crate::blobs::Blobs;

pub const PHOTOS_PER_EXCHANGE: usize = 8;

pub const FILES_PER_EXCHANGE: usize = 4;

/// Bytes before base64: the API takes 32 MB a request, and base64 grows the bytes by a third.
pub const BYTES_PER_EXCHANGE: u64 = 20 * 1024 * 1024;

/// Where an interpreter reads the bytes a turn names.
#[derive(Debug, Clone, Default)]
pub struct Stores {
    pub uploads: Option<AttachmentStore>,
    /// The files verbs handed the model.
    pub files: Option<Blobs>,
}

/// One request's blocks, oldest first.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Blocks(Vec<Value>);

enum Named<'a> {
    Upload(&'a Attachment),
    File(&'a Enclosed),
}

impl Named<'_> {
    fn is_photo(&self) -> bool {
        matches!(self, Named::Upload(upload) if matches!(upload.media_type, Enclosure::Image(_)))
    }

    fn size(&self, stores: &Stores) -> Result<u64, ModelError> {
        match self {
            Named::Upload(upload) if matches!(upload.media_type, Enclosure::Pdf) => {
                uploads(stores)?
                    .size(&upload.id)
                    .map_err(|failed| ModelError::Failed(failed.to_string()))
            }
            Named::Upload(_) => Ok(0),
            Named::File(file) => Ok(file.size),
        }
    }
}

impl Blocks {
    /// The blocks for `turns`: the latest photos and files within the counts and the byte
    /// budget, in the order they were said. A turn naming something no store holds fails.
    pub fn of(stores: &Stores, turns: &[Turn]) -> Result<Self, ModelError> {
        let (mut photos, mut files, mut bytes) = (0, 0, 0u64);
        let mut named: Vec<Named<'_>> = Vec::new();
        for turn in turns.iter().rev() {
            let on_turn: Vec<Named<'_>> = match turn {
                // A private file is never a block: its redacted text is in the transcript.
                Turn::Person { attachments, .. } => attachments
                    .iter()
                    .filter(|upload| upload.private.is_none())
                    .map(Named::Upload)
                    .collect(),
                Turn::Tool { files, .. } => files.iter().map(Named::File).collect(),
                // A turn whose words are gone hands over no file: its names went with them.
                Turn::Model { .. } | Turn::Act { .. } | Turn::Erased { .. } => Vec::new(),
            };
            for one in on_turn.into_iter().rev() {
                let size = one.size(stores)?;
                let fits = if one.is_photo() {
                    photos < PHOTOS_PER_EXCHANGE
                } else {
                    files < FILES_PER_EXCHANGE && bytes + size <= BYTES_PER_EXCHANGE
                };
                if fits {
                    if one.is_photo() {
                        photos += 1;
                    } else {
                        files += 1;
                    }
                    bytes += size;
                    named.push(one);
                }
            }
        }
        named
            .into_iter()
            .rev()
            .map(|one| block(stores, &one))
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The content array: every block, then the text.
    #[must_use]
    pub fn content(self, text: &str) -> Vec<Value> {
        let mut content = self.0;
        content.push(json!({ "type": "text", "text": text }));
        content
    }
}

fn block(stores: &Stores, named: &Named<'_>) -> Result<Value, ModelError> {
    let (media, title, bytes) = match named {
        Named::Upload(upload) => (
            upload.media_type,
            upload.name.as_deref().unwrap_or("attachment.pdf"),
            uploads(stores)?
                .bytes(&upload.id)
                .map_err(|failed| ModelError::Failed(failed.to_string()))?,
        ),
        Named::File(file) => {
            let store = stores.files.as_ref().ok_or_else(|| {
                ModelError::Failed(
                    "a verb handed over a file and no file store is configured".to_owned(),
                )
            })?;
            (file.media, file.name.as_str(), read(store, &file.id)?)
        }
    };
    let data = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    };
    Ok(match media {
        Enclosure::Pdf => json!({
            "type": "document",
            "source": { "type": "base64", "media_type": "application/pdf", "data": data },
            "title": ForModel::file_name(title).as_str(),
        }),
        Enclosure::Image(image) => json!({
            "type": "image",
            "source": { "type": "base64", "media_type": image.as_str(), "data": data },
        }),
    })
}

fn uploads(stores: &Stores) -> Result<&AttachmentStore, ModelError> {
    stores.uploads.as_ref().ok_or_else(|| {
        ModelError::Failed("a turn carries an upload and no upload store is configured".to_owned())
    })
}

fn read(store: &Blobs, id: &AttachmentId) -> Result<Vec<u8>, ModelError> {
    store
        .read(id)
        .map_err(|failed| ModelError::Failed(failed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::attachment::MediaType;

    const PDF: &[u8] = b"%PDF-1.7\n%%EOF\n";
    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1];

    fn stores(dir: &std::path::Path) -> Stores {
        Stores {
            uploads: Some(AttachmentStore::new(
                dir.join("uploads"),
                crate::pdftotext::Pdftotext::new("."),
            )),
            files: Some(Blobs::open(dir.join("files"))),
        }
    }

    fn handed(stores: &Stores, name: &str, bytes: &[u8]) -> Turn {
        let media = Enclosure::sniff(bytes).expect("a pdf or an image");
        let id = stores
            .files
            .as_ref()
            .expect("a file store")
            .put(bytes)
            .expect("kept");
        Turn::Tool {
            verb: "attachments".into(),
            answer: "{}".to_owned(),
            outcome: suzatary_core::secretary::outcome::Outcome::Answered,
            files: vec![Enclosed {
                id,
                name: name.to_owned(),
                media,
                size: bytes.len() as u64,
                text: String::new(),
            }],
        }
    }

    #[test]
    fn a_pdf_is_a_document_block_and_an_image_an_image_block() {
        use base64::Engine as _;
        let dir = tempfile::tempdir().expect("scratch");
        let stores = stores(dir.path());
        let turns = [
            handed(&stores, "notice.pdf", PDF),
            handed(&stores, "scan.png", PNG),
        ];
        let content = Blocks::of(&stores, &turns)
            .expect("both read")
            .content("what do these say");
        assert_eq!(content.len(), 3);
        assert_eq!(content[0]["type"], "document");
        assert_eq!(content[0]["source"]["type"], "base64");
        assert_eq!(content[0]["source"]["media_type"], "application/pdf");
        assert_eq!(content[0]["title"], "notice.pdf");
        assert_eq!(
            content[0]["source"]["data"],
            base64::engine::general_purpose::STANDARD.encode(PDF)
        );
        assert_eq!(content[1]["type"], "image");
        assert_eq!(content[1]["source"]["media_type"], MediaType::Png.as_str());
        assert_eq!(
            content[2],
            json!({"type": "text", "text": "what do these say"})
        );
    }

    #[test]
    fn a_photo_and_a_file_come_back_in_the_order_they_were_said() {
        let dir = tempfile::tempdir().expect("scratch");
        let stores = stores(dir.path());
        let photo = stores
            .uploads
            .as_ref()
            .expect("a photo store")
            .put(PNG)
            .expect("kept");
        let turns = [
            Turn::Person {
                text: "look".to_owned(),
                references: Vec::new(),
                attachments: vec![photo],
            },
            handed(&stores, "notice.pdf", PDF),
        ];
        let content = Blocks::of(&stores, &turns).expect("read").content("x");
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[1]["type"], "document");
    }

    #[test]
    fn an_uploaded_pdf_is_a_document_titled_as_it_was_picked_and_counts_as_a_file() {
        let dir = tempfile::tempdir().expect("scratch");
        let stores = stores(dir.path());
        // Written around `put`: these bytes are no PDF pdftotext could read.
        let beside = Blobs::open(dir.path().join("uploads"));
        let upload = |n: u8| {
            let bytes = [PDF, &[n]].concat();
            beside.put(&bytes).expect("kept");
            let pdf = Attachment::accept(&bytes).expect("a pdf");
            Turn::Person {
                text: String::new(),
                references: Vec::new(),
                attachments: vec![pdf.named(&format!("lease {n}.pdf")).expect("a name")],
            }
        };
        let turns: Vec<Turn> = (0..6u8).map(upload).collect();
        let content = Blocks::of(&stores, &turns).expect("read").content("x");
        assert!(
            content[..FILES_PER_EXCHANGE]
                .iter()
                .all(|block| block["type"] == "document")
        );
        let titles: Vec<&str> = content
            .iter()
            .filter_map(|block| block["title"].as_str())
            .collect();
        assert_eq!(
            titles,
            ["lease 2.pdf", "lease 3.pdf", "lease 4.pdf", "lease 5.pdf"]
        );
    }

    /// Law: no document title carries an identity number from a file's name.
    #[test]
    fn a_number_in_a_file_s_name_never_reaches_the_model() {
        let dir = tempfile::tempdir().expect("scratch");
        let stores = stores(dir.path());
        let turns = [handed(&stores, "W-2 SSN 000-12-3456.pdf", PDF)];
        let content = Blocks::of(&stores, &turns).expect("read").content("x");
        assert_eq!(content[0]["title"], "W-2 SSN ….pdf");
    }

    #[test]
    fn only_the_latest_files_within_the_count_cross() {
        let dir = tempfile::tempdir().expect("scratch");
        let stores = stores(dir.path());
        let turns: Vec<Turn> = (0..6u8)
            .map(|n| handed(&stores, &format!("{n}.pdf"), &[PDF, &[n]].concat()))
            .collect();
        let content = Blocks::of(&stores, &turns).expect("read").content("x");
        let titles: Vec<&str> = content
            .iter()
            .filter_map(|block| block["title"].as_str())
            .collect();
        assert_eq!(titles, ["2.pdf", "3.pdf", "4.pdf", "5.pdf"]);
    }

    #[test]
    fn only_the_latest_photos_cross_in_order() {
        use base64::Engine as _;
        let dir = tempfile::tempdir().expect("scratch");
        let stores = stores(dir.path());
        let turns: Vec<Turn> = (0..10u8)
            .map(|n| {
                let photo = stores
                    .uploads
                    .as_ref()
                    .expect("a photo store")
                    .put(&[PNG, &[n]].concat())
                    .expect("kept");
                Turn::Person {
                    text: String::new(),
                    references: Vec::new(),
                    attachments: vec![photo],
                }
            })
            .collect();
        let content = Blocks::of(&stores, &turns).expect("read").content("x");
        assert_eq!(content.len(), PHOTOS_PER_EXCHANGE + 1);
        let last = base64::engine::general_purpose::STANDARD.encode([PNG, &[9]].concat());
        assert_eq!(content[PHOTOS_PER_EXCHANGE - 1]["source"]["data"], last);
    }

    #[test]
    fn a_file_no_store_holds_is_refused_not_dropped() {
        let dir = tempfile::tempdir().expect("scratch");
        let turn = handed(&stores(dir.path()), "notice.pdf", PDF);
        let refused = Blocks::of(&Stores::default(), std::slice::from_ref(&turn));
        assert!(matches!(refused, Err(ModelError::Failed(why)) if why.contains("no file store")));
        let empty = Stores {
            files: Some(Blobs::open(dir.path().join("elsewhere"))),
            ..Stores::default()
        };
        assert!(Blocks::of(&empty, &[turn]).is_err());
    }
}
