//! What core's rows already hold: every path, and the newest tree's admissions and admitted bytes.

use crate::admission::{Admission, Digest, Vouch};
use crate::core_rows::Core;
use crate::fresh::{Compared, Fresh};
use crate::matcher::{ADMISSION_FILES, Matcher};
use crate::moved::{Standing, carries, moves};
use crate::oid::Oid;
use crate::public_tree::Written;
use crate::repo_path::RepoPath;
use crate::scratch::Scratch;
use crate::{Error, git, process};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Stdio;

/// What a push is compared with to find what no person has looked at yet.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Known {
    /// Core is unborn: the operator's first push is the baseline, and nothing is held against it.
    Baseline,
    Held(Ledger),
}

/// Every path any row held, and the newest tree's admissions and admitted bytes: an admission
/// stands only while the tree before holds it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Ledger {
    paths: BTreeSet<RepoPath>,
    admissions: BTreeSet<Admission>,
    standing: Standing,
}

impl Ledger {
    /// After a tree vouching `admissions` with files `digest_at` reads; `paths`, every row's.
    pub(crate) fn after(
        paths: BTreeSet<RepoPath>,
        admissions: BTreeSet<Admission>,
        digest_at: impl FnMut(&RepoPath) -> Result<Option<Digest>, Error>,
    ) -> Result<Ledger, Error> {
        let standing = Standing::of(&admissions, digest_at)?;
        Ok(Ledger {
            paths,
            admissions,
            standing,
        })
    }

    /// What a tree (as `after` takes one) holds that is not known, and the admitted files it moved
    /// with their fingerprints; the ledger then stands after it.
    pub(crate) fn compare(
        &mut self,
        paths: Vec<RepoPath>,
        theirs: BTreeSet<Admission>,
        mut digest_at: impl FnMut(&RepoPath) -> Result<Option<Digest>, Error>,
    ) -> Result<Compared, Error> {
        let new_bytes: BTreeSet<(RepoPath, Digest)> = theirs
            .iter()
            .filter(|admission| !self.admissions.contains(*admission))
            .filter_map(|Admission { path, vouch }| match vouch {
                Vouch::Bytes(digest) => Some((path.clone(), digest.clone())),
                Vouch::Finding(_) | Vouch::Rules(_) => None,
            })
            .collect();
        let mut files = BTreeMap::new();
        for (to, digest) in &new_bytes {
            for path in std::iter::once(to).chain(self.standing.paths(digest)) {
                if let Some(held) = digest_at(path)? {
                    files.insert(path.clone(), held);
                }
            }
        }
        let moved = moves(&self.standing, &new_bytes, &files);
        let arrived = |path: &RepoPath| moved.iter().find(|m| m.to == *path);
        let mut fresh: Vec<Fresh> = Vec::new();
        for path in paths {
            if !self.paths.contains(&path) && arrived(&path).is_none() {
                fresh.push(Fresh::Path(path.clone()));
            }
            self.paths.insert(path);
        }
        for admission in &theirs {
            let carried = arrived(&admission.path).is_some_and(|m| match &admission.vouch {
                Vouch::Bytes(digest) => files.get(&admission.path) == Some(digest),
                Vouch::Finding(finding) => carries(m, finding, &self.admissions),
                Vouch::Rules(_) => false,
            });
            if !self.admissions.contains(admission) && !carried {
                fresh.push(Fresh::Admission(admission.path.clone()));
            }
        }
        fresh.sort();
        fresh.dedup();
        *self = Ledger::after(std::mem::take(&mut self.paths), theirs, digest_at)?;
        Ok(Compared { fresh, moved })
    }
}

impl Known {
    pub(crate) fn read(core: &Core, matcher: &Matcher) -> Result<Known, Error> {
        let Some(tip) = core.tip() else {
            return Ok(Known::Baseline);
        };
        let tip_word = tip.to_string();
        let paths = process::out(&mut git::cmd(&[
            "log",
            "-z",
            "--format=",
            "--name-only",
            &tip_word,
        ]))?
        .split('\0')
        .map(|path| path.trim_start_matches('\n'))
        .filter(|path| !path.is_empty())
        .map(RepoPath::parse)
        .collect::<Result<_, _>>()?;
        let admissions = admissions_at(&tip_word, matcher)?;
        Ledger::after(paths, admissions, |path| blob_digest(tip, path)).map(Known::Held)
    }

    /// What `tree` holds that is not known, and the admitted files it moved; the ledger then
    /// stands after it, so a later tree is compared with this one.
    pub(crate) fn fresh(&mut self, tree: &Written, matcher: &Matcher) -> Result<Compared, Error> {
        let Known::Held(ledger) = self else {
            return Ok(Compared::default());
        };
        let dir = tree.dir();
        let theirs = matcher.admissions(&dir)?;
        ledger.compare(tree.paths()?, theirs, |path| file_digest(&dir, path))
    }
}

/// What the admission files of `commit` vouch for.
fn admissions_at(commit: &str, matcher: &Matcher) -> Result<BTreeSet<Admission>, Error> {
    let row = Scratch::new()?;
    for file in ADMISSION_FILES {
        let at = format!("{commit}:{file}");
        if !git::holds(&["cat-file", "-e", &at])? {
            continue;
        }
        let path = row.path().join(file);
        let io = |source| Error::Io {
            path: path.clone(),
            source,
        };
        std::fs::create_dir_all(path.parent().unwrap_or(row.path())).map_err(io)?;
        let out = std::fs::File::create(&path).map_err(io)?;
        let mut show = git::cmd(&["cat-file", "blob", &at]);
        show.stdout(Stdio::from(out));
        process::out(&mut show)?;
    }
    matcher.admissions(row.path())
}

/// The digest of the blob at `path` in `commit`, or none where it holds no blob there.
fn blob_digest(commit: &Oid, path: &RepoPath) -> Result<Option<Digest>, Error> {
    let at = format!("{commit}:{path}");
    if !git::holds(&["cat-file", "-e", &at])? || git::out(&["cat-file", "-t", &at])? != "blob" {
        return Ok(None);
    }
    let out = process::output(&mut git::cmd(&["cat-file", "blob", &at]))?;
    if out.status.success() {
        Ok(Some(Digest::of(&out.stdout)))
    } else {
        Err(Error::Run {
            command: format!("git cat-file blob {at}"),
            why: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        })
    }
}

/// The digest of the regular file at `path` under `dir`, or none where there is none: a link, on
/// the way or at the end, is not the bytes it points at.
fn file_digest(dir: &Path, path: &RepoPath) -> Result<Option<Digest>, Error> {
    let mut at = dir.to_path_buf();
    for part in path.as_str().split('/') {
        at.push(part);
        let io = |source| Error::Io {
            path: at.clone(),
            source,
        };
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => return Ok(None),
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(io(e)),
        }
    }
    if !at.is_file() {
        return Ok(None);
    }
    std::fs::read(&at)
        .map(|bytes| Some(Digest::of(&bytes)))
        .map_err(|source| Error::Io { path: at, source })
}

#[cfg(test)]
mod tests {
    use super::Ledger;
    use crate::admission::{Admission, Digest, Vouch};
    use crate::fresh::Fresh;
    use crate::repo_path::RepoPath;
    use std::collections::{BTreeMap, BTreeSet};

    fn path(name: &str) -> RepoPath {
        RepoPath::parse(name).expect("a path")
    }

    fn digest(bytes: &str) -> Digest {
        Digest::of(bytes.as_bytes())
    }

    /// A public tree for the ledger: its files' bytes, the bytes its manifest admits at each path, and
    /// its fingerprints.
    struct Tree {
        files: BTreeMap<RepoPath, Digest>,
        admissions: BTreeSet<Admission>,
    }

    fn tree(files: &[(&str, &str)], lines: &[(&str, &str)], findings: &[&str]) -> Tree {
        let vouch = |at: &str, vouch| Admission {
            path: path(at),
            vouch,
        };
        Tree {
            files: files
                .iter()
                .map(|(at, bytes)| (path(at), digest(bytes)))
                .collect(),
            admissions: lines
                .iter()
                .map(|(at, bytes)| vouch(at, Vouch::Bytes(digest(bytes))))
                .chain(findings.iter().map(|fingerprint| {
                    let at = fingerprint.split(':').next().expect("a path");
                    vouch(at, Vouch::Finding((*fingerprint).to_owned()))
                }))
                .collect(),
        }
    }

    /// The ledger after `published`, as core's tip.
    fn ledger(published: &Tree) -> Ledger {
        let paths = published.files.keys().cloned().collect();
        Ledger::after(paths, published.admissions.clone(), |at| {
            Ok(published.files.get(at).cloned())
        })
        .expect("a ledger")
    }

    /// `next` against the ledger, which then stands after it: what is held, and what moved.
    fn step(ledger: &mut Ledger, next: &Tree) -> (Vec<String>, Vec<String>) {
        let paths = next.files.keys().cloned().collect();
        let compared = ledger
            .compare(paths, next.admissions.clone(), |at| {
                Ok(next.files.get(at).cloned())
            })
            .expect("a comparison");
        let fresh = compared
            .fresh
            .iter()
            .map(|fresh| match fresh {
                Fresh::Path(at) => format!("path {at}"),
                Fresh::Admission(at) => format!("admission {at}"),
            })
            .collect();
        (
            fresh,
            compared.moved.iter().map(ToString::to_string).collect(),
        )
    }

    /// The ledger over a publish of several steps, each compared with the one before: a move carries
    /// its bytes and its fingerprint and suppresses its new path; anything else waits.
    #[test]
    fn the_ledger_carries_a_move_step_by_step_and_holds_the_rest() {
        let none: Vec<String> = Vec::new();
        let a = tree(&[("t/a.png", "A")], &[("t/a.png", "A")], &[]);
        // The old place reused by other bytes under a new line: the move, and the new line held.
        let mut l = ledger(&a);
        let reused = tree(
            &[("t/b.png", "A"), ("t/a.png", "C")],
            &[("t/b.png", "A"), ("t/a.png", "C")],
            &[],
        );
        assert_eq!(
            step(&mut l, &reused),
            (
                vec!["admission t/a.png".into()],
                vec!["t/a.png -> t/b.png".into()]
            )
        );
        // A to B, then B to C: each step from the one before.
        let mut l = ledger(&a);
        let b = tree(&[("t/b.png", "A")], &[("t/b.png", "A")], &[]);
        assert_eq!(
            step(&mut l, &b),
            (none.clone(), vec!["t/a.png -> t/b.png".into()])
        );
        let c = tree(&[("t/c.png", "A")], &[("t/c.png", "A")], &[]);
        assert_eq!(
            step(&mut l, &c),
            (none.clone(), vec!["t/b.png -> t/c.png".into()])
        );
        // A to B, then B's bytes edited.
        let mut l = ledger(&a);
        step(&mut l, &b);
        let edited = tree(&[("t/b.png", "C")], &[("t/b.png", "C")], &[]);
        assert_eq!(
            step(&mut l, &edited),
            (vec!["admission t/b.png".into()], none.clone())
        );
        // A to B, then other bytes at A under A's old line: the line names bytes its file does not
        // hold, and no published tree holds it now.
        let mut l = ledger(&a);
        step(&mut l, &b);
        let old_line = tree(
            &[("t/b.png", "A"), ("t/a.png", "C")],
            &[("t/b.png", "A"), ("t/a.png", "A")],
            &[],
        );
        assert_eq!(
            step(&mut l, &old_line),
            (vec!["admission t/a.png".into()], none.clone())
        );
        // A to B, then A's bytes and line put back beside B: a copy at a place an older row held.
        let mut l = ledger(&a);
        step(&mut l, &b);
        let back = tree(
            &[("t/b.png", "A"), ("t/a.png", "A")],
            &[("t/b.png", "A"), ("t/a.png", "A")],
            &[],
        );
        assert_eq!(
            step(&mut l, &back),
            (vec!["admission t/a.png".into()], none.clone())
        );
        // Out of its top-level directory: no move.
        let mut l = ledger(&a);
        let away = tree(&[("w/a.png", "A")], &[("w/a.png", "A")], &[]);
        assert_eq!(
            step(&mut l, &away),
            (
                vec!["path w/a.png".into(), "admission w/a.png".into()],
                none.clone()
            )
        );
    }

    /// A fingerprint follows its file's move when it is the published one in everything but the
    /// path; another fingerprint at the new place waits for a person.
    #[test]
    fn a_moved_file_carries_its_fingerprint_and_no_other() {
        let key = "-----BEGIN KEY-----";
        let published = tree(
            &[("app/a/tests/k.pem", key)],
            &[("app/a/tests/k.pem", key)],
            &["app/a/tests/k.pem:private-key:1"],
        );
        let moved = tree(
            &[("app/b/tests/k.pem", key)],
            &[("app/b/tests/k.pem", key)],
            &["app/b/tests/k.pem:private-key:1"],
        );
        assert_eq!(
            step(&mut ledger(&published), &moved),
            (
                Vec::<String>::new(),
                vec!["app/a/tests/k.pem -> app/b/tests/k.pem".into()]
            )
        );
        let other = tree(
            &[("app/b/tests/k.pem", key)],
            &[("app/b/tests/k.pem", key)],
            &[
                "app/b/tests/k.pem:private-key:1",
                "app/b/tests/k.pem:generic-api-key:2",
            ],
        );
        assert_eq!(
            step(&mut ledger(&published), &other),
            (
                vec!["admission app/b/tests/k.pem".into()],
                vec!["app/a/tests/k.pem -> app/b/tests/k.pem".into()]
            )
        );
    }
}
