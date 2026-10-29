//! The files the owner tapped Read on outside the allowed Drive folders, each as that tap read
//! it: its source, name, type, size and SHA-256. Box-local, and never told to a model, which
//! could test guesses of a number against the digest. A proposal that sends such a file records
//! it from here, so the tap that sends it is the one read past the fence.

use std::path::PathBuf;

use suzatary_core::attachment::AttachmentId;
use suzatary_core::held_value::HeldFile;
use suzatary_core::secretary::attached::Attached;

use crate::docs::{DocCodec, Docs};

/// The tapped files kept beside `kept`, the files verbs hand the model, in `tapped/` inside its
/// directory and so in its tier: one document each, readable by this user alone.
#[must_use]
pub fn beside(kept: &crate::blobs::Blobs) -> Docs<HeldFile, Attached> {
    let dir = kept
        .path(&AttachmentId::of(b""))
        .parent()
        .map_or_else(|| PathBuf::from("tapped"), |dir| dir.join("tapped"));
    Docs::new(
        "tapped files",
        move |file: &HeldFile| {
            dir.join(format!(
                "{}.json",
                AttachmentId::of(file.print().as_bytes())
            ))
        },
        &RECORD,
    )
    .private()
}

/// A record as JSON, which must be of the file its name was made from.
const RECORD: DocCodec<HeldFile, Attached> = DocCodec {
    print: |_, record| serde_json::to_vec(record).map_err(|failed| failed.to_string()),
    parse: |file, bytes| {
        let record: Attached =
            serde_json::from_slice(&bytes).map_err(|failed| failed.to_string())?;
        Ok((record.source == file.source()).then_some(record))
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::store::{Doc as _, Gets as _};

    #[test]
    fn a_record_is_read_back_only_under_its_own_file() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let store = beside(&crate::blobs::Blobs::open(dir.path().to_path_buf()));
        let w2 = HeldFile::Drive {
            path: "/my-files/Suzatary/Private/w2.pdf".parse().expect("a path"),
        };
        let record = Attached::known(
            w2.source(),
            "w2.pdf",
            "application/pdf",
            4096,
            AttachmentId::of(b"w2"),
        )
        .expect("a record");
        store.set(&w2, &record).expect("kept");
        assert_eq!(store.get(&w2).expect("read"), Some(record));
        let other = HeldFile::Drive {
            path: "/my-files/Suzatary/Private/w3.pdf".parse().expect("a path"),
        };
        assert_eq!(store.get(&other).expect("read"), None);
    }
}
