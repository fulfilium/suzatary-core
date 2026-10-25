//! The box's recorded frontiers on disk (`suzatary_core::frontier`), under the instance's own
//! directory in the state directory beside the epochs: box-local, never pushed and never backed
//! up, since a lost record costs a replica one pull from nothing. Each store has a directory, its
//! name the store's with `/` spelled `-`; each frontier is a file named by its digest holding one
//! head per line, and `ring` lists the store's digests, newest last.

use std::collections::BTreeSet;
use std::path::PathBuf;

use prodrome_core::event::Hash;
use suzatary_core::frontier::{Digest, Frontiers, Records, Unrecorded};
use suzatary_core::store::Doc as _;

use crate::docs::{DocCodec, Docs};

/// The frontiers under `root`.
#[must_use]
pub fn open(root: PathBuf) -> Frontiers {
    let rings = root.clone();
    Frontiers::over(OnDisk {
        sets: Docs::new(
            "frontiers",
            move |kept: &Kept| directory(&root, &kept.store).join(kept.digest.as_str()),
            &SETS,
        ),
        rings: Docs::new(
            "frontiers",
            move |store: &String| directory(&rings, store).join("ring"),
            &RINGS,
        ),
    })
}

/// The key of one recorded frontier on disk: its store and its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Kept {
    store: String,
    digest: Digest,
}

/// The records as files: each set by its key, each store's ring beside them.
struct OnDisk {
    sets: Docs<Kept, BTreeSet<Hash>>,
    rings: Docs<String, Vec<Digest>>,
}

fn kept(store: &str, digest: &Digest) -> Kept {
    Kept {
        store: store.to_owned(),
        digest: digest.clone(),
    }
}

fn said(failed: impl std::fmt::Display) -> Unrecorded {
    Unrecorded(failed.to_string())
}

impl Records for OnDisk {
    fn heads(&self, store: &str, digest: &Digest) -> Result<Option<BTreeSet<Hash>>, Unrecorded> {
        self.sets.get(&kept(store, digest)).map_err(said)
    }

    fn keep(&self, store: &str, digest: &Digest, heads: &BTreeSet<Hash>) -> Result<(), Unrecorded> {
        self.sets.set(&kept(store, digest), heads).map_err(said)
    }

    fn let_go(&self, store: &str, digest: &Digest) -> Result<(), Unrecorded> {
        self.sets.remove(&kept(store, digest)).map_err(said)
    }

    fn ring(&self, store: &str) -> Result<Vec<Digest>, Unrecorded> {
        Ok(self
            .rings
            .get(&store.to_owned())
            .map_err(said)?
            .unwrap_or_default())
    }

    fn keep_ring(&self, store: &str, ring: &[Digest]) -> Result<(), Unrecorded> {
        self.rings
            .set(&store.to_owned(), &ring.to_vec())
            .map_err(said)
    }
}

fn directory(root: &std::path::Path, store: &str) -> PathBuf {
    root.join(store.replace('/', "-"))
}

/// Each name on a line of its own.
fn one_per_line<'a>(names: impl Iterator<Item = &'a str>) -> Vec<u8> {
    let mut text = String::new();
    for name in names {
        text.push_str(name);
        text.push('\n');
    }
    text.into_bytes()
}

/// One name per line, each parsed by `parse`.
fn lines<T>(bytes: Vec<u8>, parse: impl Fn(&str) -> Result<T, String>) -> Result<Vec<T>, String> {
    String::from_utf8(bytes)
        .map_err(|failed| failed.to_string())?
        .lines()
        .map(parse)
        .collect()
}

const SETS: DocCodec<Kept, BTreeSet<Hash>> = DocCodec {
    print: |_, heads| Ok(one_per_line(heads.iter().map(Hash::as_str))),
    // A file whose heads are not what its name is the digest of is no record of that frontier.
    parse: |kept, bytes| {
        let heads: BTreeSet<Hash> = lines(bytes, |line| {
            Hash::new(line).map_err(|failed| failed.to_string())
        })?
        .into_iter()
        .collect();
        Ok((Digest::of(&heads) == kept.digest).then_some(heads))
    },
};

const RINGS: DocCodec<String, Vec<Digest>> = DocCodec {
    print: |_, ring| Ok(one_per_line(ring.iter().map(Digest::as_str))),
    parse: |_, bytes| {
        lines(bytes, |line| {
            Digest::parse(line).map_err(|failed| failed.to_string())
        })
        .map(Some)
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use suzatary_core::frontier::{Read, Since};

    /// A frontier recorded by one process reads back in the next, and a file whose heads are not
    /// its name's reads as unknown.
    #[test]
    fn a_frontier_outlives_the_process_that_recorded_it() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let heads: BTreeSet<Hash> = [Hash::new("a".repeat(64)).expect("a name")].into();
        let digest = open(dir.path().to_path_buf())
            .record("store/lines", &heads)
            .expect("recorded");
        let since = Since::Frontier(digest.clone());
        assert_eq!(
            open(dir.path().to_path_buf())
                .read("store/lines", &since)
                .expect("reads"),
            Read::Known(heads)
        );
        std::fs::write(
            dir.path().join("store-lines").join(digest.as_str()),
            format!("{}\n", "b".repeat(64)),
        )
        .expect("rewritten");
        assert_eq!(
            open(dir.path().to_path_buf())
                .read("store/lines", &since)
                .expect("reads"),
            Read::Unknown
        );
    }
}
