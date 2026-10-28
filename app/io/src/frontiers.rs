//! The box's recorded frontiers on disk (`prodrome_core::store::frontier`), under the instance's
//! own directory in the state directory beside the epochs: box-local, never pushed and never
//! backed up, since a lost record costs a replica one pull from nothing. Each store has a
//! directory, its name the store's with `/` spelled `-`; each frontier is a file named by its
//! token holding one tip per line, and `ring` lists the store's tokens, newest last.

use std::collections::BTreeSet;
use std::path::PathBuf;

use prodrome_core::event::Hash;
use prodrome_core::store::frontier::{Frontiers, KEPT, Records, Token, Unrecorded};
use suzatary_core::store::{Doc as _, Gets as _};

use crate::docs::{DocCodec, Docs};

/// The frontiers under `root`.
#[must_use]
pub fn open(root: PathBuf) -> Frontiers {
    let rings = root.clone();
    Frontiers::over(
        OnDisk {
            sets: Docs::new(
                "frontiers",
                move |kept: &Kept| directory(&root, &kept.store).join(kept.token.as_str()),
                &SETS,
            ),
            rings: Docs::new(
                "frontiers",
                move |store: &String| directory(&rings, store).join("ring"),
                &RINGS,
            ),
        },
        KEPT,
    )
}

/// The key of one recorded frontier on disk: its store and its token.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Kept {
    store: String,
    token: Token,
}

/// The records as files: each set by its key, each store's ring beside them.
struct OnDisk {
    sets: Docs<Kept, BTreeSet<Hash>>,
    rings: Docs<String, Vec<Token>>,
}

fn kept(store: &str, token: &Token) -> Kept {
    Kept {
        store: store.to_owned(),
        token: token.clone(),
    }
}

fn said(failed: impl std::fmt::Display) -> Unrecorded {
    Unrecorded(failed.to_string())
}

impl Records for OnDisk {
    fn tips(&self, store: &str, token: &Token) -> Result<Option<BTreeSet<Hash>>, Unrecorded> {
        self.sets.get(&kept(store, token)).map_err(said)
    }

    fn keep(&self, store: &str, token: &Token, tips: &BTreeSet<Hash>) -> Result<(), Unrecorded> {
        self.sets.set(&kept(store, token), tips).map_err(said)
    }

    fn let_go(&self, store: &str, token: &Token) -> Result<(), Unrecorded> {
        self.sets.remove(&kept(store, token)).map_err(said)
    }

    fn ring(&self, store: &str) -> Result<Vec<Token>, Unrecorded> {
        Ok(self
            .rings
            .get(&store.to_owned())
            .map_err(said)?
            .unwrap_or_default())
    }

    fn keep_ring(&self, store: &str, ring: &[Token]) -> Result<(), Unrecorded> {
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
    print: |_, tips| Ok(one_per_line(tips.iter().map(Hash::as_str))),
    // A file whose tips are not what its name is the token of is no record of that frontier.
    parse: |kept, bytes| {
        let tips: BTreeSet<Hash> = lines(bytes, |line| {
            Hash::new(line).map_err(|failed| failed.to_string())
        })?
        .into_iter()
        .collect();
        Ok((Token::of(&tips) == kept.token).then_some(tips))
    },
};

const RINGS: DocCodec<String, Vec<Token>> = DocCodec {
    print: |_, ring| Ok(one_per_line(ring.iter().map(Token::as_str))),
    parse: |_, bytes| {
        lines(bytes, |line| {
            Token::parse(line).map_err(|failed| failed.to_string())
        })
        .map(Some)
    },
};

#[cfg(test)]
mod tests {
    use super::*;
    use prodrome_core::dag::Dag;
    use prodrome_core::event::{Envelope, seal_hash};
    use prodrome_core::genesis::mk_genesis;
    use prodrome_core::store::frontier::{Reached, Since};
    use suzatary_core::secretary::topics::reading::MembershipEvent;

    /// The token the box names a frontier by, and so each record file by, is the sha256 of the
    /// tips, one on each line: the names of the files the box has already written.
    #[test]
    fn a_token_is_the_name_the_box_s_files_carry() {
        let tips: BTreeSet<Hash> = [Hash::new("a".repeat(64)).expect("a name")].into();
        assert_eq!(
            Token::of(&tips).as_str(),
            "44c2336fedab8ff6a85c74c2b94165377b0981f526adb9487895ca6314165e86"
        );
    }

    /// A frontier recorded by one process reads back in the next, as every object where its tips
    /// are the store's, and a file whose tips are not its name's reads as unknown.
    #[test]
    fn a_frontier_outlives_the_process_that_recorded_it() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let genesis: Envelope<MembershipEvent> =
            Envelope::Genesis(mk_genesis("topics", &"0".repeat(32)).expect("a genesis"));
        let name = seal_hash(&genesis);
        let dag: Dag<MembershipEvent> = [(name.clone(), genesis)].into_iter().collect();
        let token = open(dir.path().to_path_buf())
            .answer("store/topics", &Since::Nothing, &dag)
            .expect("answered")
            .record(1)
            .expect("recorded")
            .token;
        assert_eq!(token, Token::of(&[name.clone()].into()));
        let since = Since::Frontier(token.clone());
        let reached = |frontiers: Frontiers| {
            frontiers
                .answer("store/topics", &since, &dag)
                .expect("answered")
                .reached()
                .clone()
        };
        assert_eq!(reached(open(dir.path().to_path_buf())), Reached::Everything);
        std::fs::write(
            dir.path().join("store-topics").join(token.as_str()),
            format!("{}\n", "b".repeat(64)),
        )
        .expect("rewritten");
        assert_eq!(reached(open(dir.path().to_path_buf())), Reached::Unknown);
    }
}
