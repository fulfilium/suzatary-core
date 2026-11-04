//! The text files of a checkout, or of a commit, by path: what the measures read.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context as _;
use suzatary_io::run::{Cmd, succeed};

/// Every text file of a tree, by its path from the root. A file that is not UTF-8 is left out:
/// an agent does not read an image or a font.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    files: BTreeMap<String, String>,
}

impl Tree {
    /// The files git sees in the checkout at `root`: tracked, and untracked but not ignored.
    ///
    /// # Errors
    /// When git fails, or a file it lists cannot be read.
    pub fn checkout(root: &Path) -> anyhow::Result<Self> {
        let listed = git(
            root,
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
        )?;
        let mut files = BTreeMap::new();
        for path in listed.split('\0').filter(|path| !path.is_empty()) {
            let full = root.join(path);
            // A file deleted but not yet staged is still listed; it is not in the tree.
            if !full.is_file() {
                continue;
            }
            let bytes = std::fs::read(&full).with_context(|| format!("reading {path}"))?;
            if let Ok(text) = String::from_utf8(bytes) {
                files.insert(path.to_owned(), text);
            }
        }
        Ok(Self { files })
    }

    /// The files of commit `rev` in the repository at `root`.
    ///
    /// # Errors
    /// When git fails or prints what `git cat-file --batch` does not.
    pub fn commit(root: &Path, rev: &str) -> anyhow::Result<Self> {
        let listed = git(root, &["ls-tree", "-r", "-z", "--name-only", rev])?;
        let paths: Vec<&str> = listed.split('\0').filter(|path| !path.is_empty()).collect();
        let asked = paths.iter().fold(String::new(), |mut asked, path| {
            let _ = writeln!(asked, "{rev}:{path}");
            asked
        });
        let ran = succeed(
            &Cmd::new("git")
                .arg("-C")
                .arg(root)
                .args(["cat-file", "--batch"])
                .stdin(asked),
        )?;
        let mut rest = ran.stdout.as_slice();
        let mut files = BTreeMap::new();
        for path in paths {
            let end = rest
                .iter()
                .position(|byte| *byte == b'\n')
                .context("git cat-file ended early")?;
            let header = String::from_utf8_lossy(&rest[..end]).into_owned();
            rest = &rest[end + 1..];
            // `<object> <type> <size>`, or `<name> missing` for a submodule's commit.
            let Some(size) = header
                .split_whitespace()
                .nth(2)
                .and_then(|size| size.parse::<usize>().ok())
            else {
                continue;
            };
            let bytes = rest
                .get(..size)
                .context("git cat-file cut an object short")?;
            if let Ok(text) = std::str::from_utf8(bytes) {
                files.insert(path.to_owned(), text.to_owned());
            }
            rest = rest.get(size + 1..).unwrap_or_default();
        }
        Ok(Self { files })
    }

    /// The text of the file at `path`.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    /// Every file at `path` or under it, with its text; the empty path is the whole tree.
    pub fn under<'a>(&'a self, path: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        let directory = format!("{}/", path.trim_end_matches('/'));
        self.files
            .range::<str, _>((std::ops::Bound::Included(path), std::ops::Bound::Unbounded))
            .take_while(move |(file, _)| file.starts_with(path))
            .filter(move |(file, _)| {
                path.is_empty() || file.as_str() == path || file.starts_with(&directory)
            })
            .map(|(file, text)| (file.as_str(), text.as_str()))
    }
}

/// What `git -C root <args>` printed.
///
/// # Errors
/// When it cannot run or fails.
pub fn git(root: &Path, args: &[&str]) -> anyhow::Result<String> {
    let ran = succeed(&Cmd::new("git").arg("-C").arg(root).args(args))?;
    Ok(String::from_utf8(ran.stdout)?)
}
