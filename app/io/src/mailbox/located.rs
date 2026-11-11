//! Where each message the corpus has listed lives, by canonical hash: one file per hash, in a
//! directory named by the hash's first two characters, holding the notmuch id the message was
//! listed under. `Notmuch::load_messages` records every hash a verb prints before it answers, so
//! a printed hash resolves by reading one directory, without rehashing the corpus.
//!
//! Nothing removes an entry: a message deleted since it was listed keeps an id notmuch no longer
//! holds, and showing it answers that the box does not hold it.

use std::path::PathBuf;

use suzatary_core::mail::hash_prefix::{HashPrefix, Resolved};
use suzatary_core::mail_error::MailError;
use suzatary_core::store::{Doc as _, Gets as _};

use crate::docs::{DocCodec, Docs, shard};
use crate::present::Present as _;

/// The locators under one directory.
#[derive(Debug, Clone)]
pub struct Located {
    dir: PathBuf,
    docs: Docs<String, String>,
}

impl Located {
    /// The locators under `dir`, made on the first record.
    #[must_use]
    pub fn open(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        Self {
            docs: Docs::sharded("mail locators", dir.clone(), &CODEC),
            dir,
        }
    }

    #[must_use]
    pub(crate) fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// Record that the message whose canonical hash is `hash` was listed as `native`; a hash
    /// already recorded as `native` is not written again.
    ///
    /// # Errors
    /// The file could not be read or written.
    pub fn record(&self, hash: &str, native: &str) -> Result<(), MailError> {
        let hash = hash.to_owned();
        let held = self
            .docs
            .get(&hash)
            .map_err(|failed| unreachable(&failed))?;
        if held.as_deref() == Some(native) {
            return Ok(());
        }
        self.docs
            .set(&hash, &native.to_owned())
            .map_err(|failed| unreachable(&failed))
    }

    /// The notmuch id recorded for the one hash `prefix` begins.
    ///
    /// # Errors
    /// The directory or a file could not be read.
    pub fn resolve(&self, prefix: &HashPrefix) -> Result<Resolved<String>, MailError> {
        let shard = self.dir.join(shard(prefix.as_str()));
        let Some(entries) = std::fs::read_dir(&shard).present().map_err(|failed| {
            MailError::Unreachable(format!("reading {}: {failed}", shard.display()))
        })?
        else {
            return Ok(Resolved::Unlisted);
        };
        let mut hashes = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|failed| {
                MailError::Unreachable(format!("reading {}: {failed}", shard.display()))
            })?;
            // A file being written is a sibling with a temporary name, never a hash.
            if let Some(name) = entry.file_name().to_str().and_then(HashPrefix::parse) {
                hashes.push((name.as_str().to_owned(), ()));
            }
        }
        Ok(match prefix.resolve(hashes) {
            Resolved::Unlisted => Resolved::Unlisted,
            Resolved::Ambiguous(ambiguous) => Resolved::Ambiguous(ambiguous),
            Resolved::One(hash, ()) => match self
                .docs
                .get(&hash)
                .map_err(|failed| unreachable(&failed))?
            {
                Some(native) => Resolved::One(hash, native),
                None => Resolved::Unlisted,
            },
        })
    }
}

fn unreachable(failed: &suzatary_core::store::StoreError) -> MailError {
    MailError::Unreachable(failed.to_string())
}

/// The file is the notmuch id, as notmuch printed it.
const CODEC: DocCodec<String, String> = DocCodec {
    print: |_, native| Ok(native.clone().into_bytes()),
    parse: |_, bytes| {
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|failed| failed.to_string())
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A hash resolves by any unique prefix of it, two hashes sharing a prefix are ambiguous by
    /// it, and a hash recorded again under another id answers the later one.
    #[test]
    fn a_recorded_hash_resolves_by_its_prefix() {
        let dir = tempfile::tempdir().expect("scratch");
        let located = Located::open(dir.path().join("located"));
        let prefix = |text: &str| HashPrefix::parse(text).expect("a prefix");
        let (one, two) = (
            format!("abcdef1{}", "0".repeat(57)),
            format!("abcdef2{}", "0".repeat(57)),
        );
        assert_eq!(
            located.resolve(&prefix("abcdef")).expect("reads"),
            Resolved::Unlisted
        );
        located.record(&one, "one@example.test").expect("records");
        located.record(&two, "two@example.test").expect("records");
        located.record(&one, "again@example.test").expect("records");
        assert_eq!(
            located.resolve(&prefix("abcdef1")).expect("reads"),
            Resolved::One(one.clone(), "again@example.test".to_owned())
        );
        let Resolved::Ambiguous(ambiguous) = located.resolve(&prefix("abcdef")).expect("reads")
        else {
            panic!("two hashes begin abcdef");
        };
        assert_eq!(ambiguous.matches, vec![one, two]);
    }
}
