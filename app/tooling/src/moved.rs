//! Admitted bytes at a new path, which need no second word.

use crate::Error;
use crate::admission::{Admission, Digest, Vouch};
use crate::repo_path::{Area, RepoPath};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Each admitted digest's paths in one tree, a path counted only where its file holds those bytes.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Standing(BTreeMap<Digest, BTreeSet<RepoPath>>);

impl Standing {
    pub(crate) fn of(
        admissions: &BTreeSet<Admission>,
        mut digest_at: impl FnMut(&RepoPath) -> Result<Option<Digest>, Error>,
    ) -> Result<Standing, Error> {
        let mut standing = Standing::default();
        for Admission { path, vouch } in admissions {
            if let Vouch::Bytes(digest) = vouch
                && digest_at(path)?.as_ref() == Some(digest)
            {
                standing.place(digest.clone(), path.clone());
            }
        }
        Ok(standing)
    }

    pub(crate) fn place(&mut self, digest: Digest, path: RepoPath) {
        self.0.entry(digest).or_default().insert(path);
    }

    pub(crate) fn paths(&self, digest: &Digest) -> impl Iterator<Item = &RepoPath> {
        self.0.get(digest).into_iter().flatten()
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) struct Moved {
    pub(crate) from: RepoPath,
    pub(crate) to: RepoPath,
}

impl fmt::Display for Moved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {}", self.from, self.to)
    }
}

/// The moves among `fresh` (new admissions of bytes) against `before`; `tree` maps each file a
/// move could name to its digest. `d` at `to` moved from `from` when the tree's file at `to`
/// holds `d`, `before` holds `d` at `from`, the tree holds no file of `d` at `from`, and both share
/// an `Area` (the test trees of a top-level directory, or one directory); each `from` carries one `to`, so a copy moves none.
#[must_use]
pub(crate) fn moves(
    before: &Standing,
    fresh: &BTreeSet<(RepoPath, Digest)>,
    tree: &BTreeMap<RepoPath, Digest>,
) -> Vec<Moved> {
    let mut arrived: BTreeMap<(&Digest, Area<'_>), Vec<&RepoPath>> = BTreeMap::new();
    for (to, digest) in fresh {
        if tree.get(to) == Some(digest) {
            arrived.entry((digest, to.area())).or_default().push(to);
        }
    }
    let mut moved = Vec::new();
    for ((digest, area), tos) in arrived {
        let left: Vec<&RepoPath> = before
            .paths(digest)
            .filter(|from| from.area() == area && tree.get(*from) != Some(digest))
            .collect();
        if tos.len() <= left.len() {
            moved.extend(left.into_iter().zip(tos).map(|(from, to)| Moved {
                from: from.clone(),
                to: to.clone(),
            }));
        }
    }
    moved.sort();
    moved
}

/// Whether `finding`, a fingerprint at `to`, is the published one at `from` but for the path.
#[must_use]
pub(crate) fn carries(moved: &Moved, finding: &str, before: &BTreeSet<Admission>) -> bool {
    let rest = finding.strip_prefix(moved.to.as_str());
    rest.filter(|rest| rest.starts_with(':'))
        .is_some_and(|rest| {
            before.contains(&Admission {
                path: moved.from.clone(),
                vouch: Vouch::Finding(format!("{}{rest}", moved.from)),
            })
        })
}

#[cfg(test)]
mod tests {
    use super::{Standing, moves};
    use crate::admission::Digest;
    use crate::repo_path::RepoPath;
    use proptest::prelude::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn path(name: &str) -> RepoPath {
        RepoPath::parse(name).expect("a path")
    }

    fn digest(bytes: &str) -> Digest {
        Digest::of(bytes.as_bytes())
    }

    /// Where the published tree's admitted bytes stand, each `(path, bytes)`.
    fn standing(admitted: &[(&str, &str)]) -> Standing {
        let mut standing = Standing::default();
        for (at, bytes) in admitted {
            standing.place(digest(bytes), path(at));
        }
        standing
    }

    /// The moves of a tree whose files are `files` and whose manifest adds `fresh`, each line naming
    /// the bytes it says, beside `before`, as `from -> to`.
    fn moved(
        before: &[(&str, &str)],
        files: &[(&str, &str)],
        fresh: &[(&str, &str)],
    ) -> Vec<String> {
        let tree: BTreeMap<RepoPath, Digest> = files
            .iter()
            .map(|(at, bytes)| (path(at), digest(bytes)))
            .collect();
        let fresh: BTreeSet<(RepoPath, Digest)> = fresh
            .iter()
            .map(|(at, bytes)| (path(at), digest(bytes)))
            .collect();
        moves(&standing(before), &fresh, &tree)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// AN ADMISSION FOLLOWS ITS BYTES, and only its bytes: a move is the same bytes, admitted in the
    /// published tree, standing at a new path while their old path no longer holds them.
    #[test]
    fn an_admission_moves_with_its_bytes_and_nothing_else_rides_on_it() {
        let before = [("a.png", "A"), ("b.png", "B")];
        let none: Vec<String> = Vec::new();
        // Moved: the old path gone, or holding other bytes.
        assert_eq!(
            moved(
                &before,
                &[("n.png", "A"), ("b.png", "B")],
                &[("n.png", "A")]
            ),
            ["a.png -> n.png"]
        );
        assert_eq!(
            moved(
                &before,
                &[("a.png", "C"), ("n.png", "A"), ("b.png", "B")],
                &[("n.png", "A")]
            ),
            ["a.png -> n.png"]
        );
        // Swapped: two moves.
        assert_eq!(
            moved(
                &before,
                &[("a.png", "B"), ("b.png", "A")],
                &[("a.png", "B"), ("b.png", "A")]
            ),
            ["a.png -> b.png", "b.png -> a.png"]
        );
        // A copy beside the original is a second place.
        assert_eq!(
            moved(
                &before,
                &[("a.png", "A"), ("n.png", "A")],
                &[("n.png", "A")]
            ),
            none
        );
        // Two new places for one left: neither moved.
        assert_eq!(
            moved(
                &before,
                &[("n.png", "A"), ("m.png", "A")],
                &[("n.png", "A"), ("m.png", "A")]
            ),
            none
        );
        // A rename that also edits: other bytes.
        assert_eq!(moved(&before, &[("n.png", "C")], &[("n.png", "C")]), none);
        // A line that names bytes its file does not hold.
        assert_eq!(moved(&before, &[("n.png", "C")], &[("n.png", "A")]), none);
        // Bytes the published tree does not hold admitted: long deleted, or a line its file belied.
        assert_eq!(moved(&[], &[("n.png", "A")], &[("n.png", "A")]), none);
        // Among a top-level directory's test trees, a move; out of them, or to another directory, none.
        let fixture = [("app/io-mail/tests/fixtures/k.pem", "K")];
        assert_eq!(
            moved(
                &fixture,
                &[("app/server/tests/k.pem", "K")],
                &[("app/server/tests/k.pem", "K")]
            ),
            ["app/io-mail/tests/fixtures/k.pem -> app/server/tests/k.pem"]
        );
        assert_eq!(
            moved(
                &fixture,
                &[("app/server/static/k.pem", "K")],
                &[("app/server/static/k.pem", "K")]
            ),
            none
        );
        assert_eq!(
            moved(
                &fixture,
                &[("other/test/k.pem", "K")],
                &[("other/test/k.pem", "K")]
            ),
            none
        );
        // Two copies published, one moved and one deleted: one move.
        assert_eq!(
            moved(
                &[("a.png", "A"), ("c.png", "A")],
                &[("n.png", "A")],
                &[("n.png", "A")]
            ),
            ["a.png -> n.png"]
        );
    }

    /// The same rule over generated trees: every move carries bytes the published tree held admitted
    /// at its `from`, which the tree no longer holds there and does hold at its `to`, in the same
    /// top-level directory, and no `from` carries two.
    #[test]
    fn a_move_never_carries_bytes_no_person_admitted() {
        let names = ["a", "b", "x/c", "x/d", "y/e", "p/tests/f", "p/q/fixtures/g"];
        let bytes = ["A", "B", "C"];
        let placed = || prop::collection::vec((0..names.len(), 0..bytes.len()), 0..5);
        proptest!(|(before in placed(), files in placed(), fresh in placed())| {
            let pick = |picked: &[(usize, usize)]| -> Vec<(&str, &str)> {
                picked.iter().map(|&(n, b)| (names[n], bytes[b])).collect()
            };
            let (before, files, fresh) = (pick(&before), pick(&files), pick(&fresh));
            let held: BTreeMap<&str, &str> = files.iter().copied().collect();
            let files: Vec<(&str, &str)> = held.iter().map(|(n, b)| (*n, *b)).collect();
            let mut froms = BTreeSet::new();
            for line in &moved(&before, &files, &fresh) {
                let (from, to) = line.split_once(" -> ").expect("from -> to");
                let carried = held[to];
                prop_assert!(before.contains(&(from, carried)), "{line}: never admitted at {from}");
                prop_assert!(held.get(from) != Some(&carried), "{line}: a copy");
                prop_assert!(fresh.contains(&(to, carried)), "{line}: no line at {to}");
                prop_assert!(froms.insert(from), "{line}: {from} carried twice");
                let (from_path, to_path) = (path(from), path(to));
                prop_assert_eq!(from_path.area(), to_path.area(), "{}: out of its directory", line);
            }
        });
    }
}
