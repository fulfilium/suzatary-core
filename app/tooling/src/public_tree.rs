//! A commit's public tree, written out, and the leak guard over it.

use crate::leak_check::{CheckError, LeakCheck};
use crate::matcher::Matcher;
use crate::oid::Oid;
use crate::repo_path::RepoPath;
use crate::scratch::Scratch;
use crate::{Error, git, process};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// `scripts/public-tree.sh`, which writes a commit's public tree and drives the leak guard.
#[derive(Clone, Debug)]
pub struct Script(PathBuf);

/// The public tree of a commit, written out and not yet checked: its paths are read, never pushed.
#[derive(Debug)]
pub struct Written {
    scratch: Scratch,
    source: Oid,
}

/// A public tree its leak guard passed whole: the only tree a push takes.
#[derive(Debug)]
pub struct PublicTree(Written);

impl Script {
    /// The checkout's own.
    pub fn of_checkout() -> Result<Script, Error> {
        Ok(Script(git::top()?.join("scripts/public-tree.sh")))
    }

    #[cfg(feature = "seams")]
    #[must_use]
    pub fn stub(path: PathBuf) -> Script {
        Script(path)
    }

    /// `source`'s public tree, written out; its words go to stderr.
    pub fn write(&self, source: &Oid) -> Result<Written, CheckError> {
        let unwritten = |why: String| CheckError::Unwritten {
            source: source.clone(),
            why,
        };
        let scratch = Scratch::new().map_err(|e| unwritten(e.to_string()))?;
        let status = Command::new(&self.0)
            .arg(scratch.path().join("tree"))
            .arg(source.to_string())
            .stdout(Stdio::null())
            .status()
            .map_err(|e| unwritten(e.to_string()))?;
        if status.success() {
            Ok(Written {
                scratch,
                source: source.clone(),
            })
        } else {
            Err(unwritten(format!("{} {status}", self.0.display())))
        }
    }

    /// The leak guard, every rule and none skipped (`--check --strict`), over `source`'s public
    /// tree, read by `matcher`; its hits go to stderr.
    #[must_use]
    pub fn check(&self, source: &Oid, matcher: &Matcher) -> LeakCheck<PublicTree> {
        match self.write(source) {
            Ok(written) => self.guard(written, matcher),
            Err(why) => LeakCheck::CouldNotRun(why),
        }
    }

    /// The leak guard over a tree already written out.
    #[must_use]
    pub fn guard(&self, written: Written, matcher: &Matcher) -> LeakCheck<PublicTree> {
        let unrun = |status: String| {
            LeakCheck::CouldNotRun(CheckError::Unrun {
                source: written.source.clone(),
                status,
            })
        };
        let status = Command::new(&self.0)
            .args(["--check", "--strict"])
            .arg(written.dir())
            .env("SUZATARY_BIN", matcher.path())
            .stdout(std::io::stderr())
            .status();
        match status.map(|status| status.code()) {
            Ok(Some(0)) => LeakCheck::Clean(PublicTree(written)),
            Ok(Some(1)) => LeakCheck::Hits,
            Ok(Some(code)) => unrun(format!("exited {code}")),
            Ok(None) => unrun("was stopped by a signal".into()),
            Err(e) => unrun(format!("did not start: {e}")),
        }
    }
}

impl Written {
    #[must_use]
    pub fn dir(&self) -> PathBuf {
        self.scratch.path().join("tree")
    }

    /// Every file's path, as `find -type f` lists them.
    pub fn paths(&self) -> Result<Vec<RepoPath>, Error> {
        let mut paths = Vec::new();
        files(&self.dir(), "", &mut paths)?;
        Ok(paths)
    }
}

fn files(dir: &Path, prefix: &str, into: &mut Vec<RepoPath>) -> Result<(), Error> {
    let io = |source| Error::Io {
        path: dir.to_owned(),
        source,
    };
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let entry = entry.map_err(io)?;
        let name = entry.file_name().into_string().map_err(|name| {
            Error::Malformed(format!(
                "{} under {} is not UTF-8",
                name.display(),
                dir.display()
            ))
        })?;
        let path = format!("{prefix}{name}");
        let kind = entry.file_type().map_err(io)?;
        if kind.is_dir() {
            files(&entry.path(), &format!("{path}/"), into)?;
        } else if kind.is_file() {
            into.push(RepoPath::parse(&path)?);
        }
    }
    Ok(())
}

impl PublicTree {
    #[must_use]
    pub fn source(&self) -> &Oid {
        &self.0.source
    }

    /// The tree as a git tree of this repository, through an index of its own, past any
    /// `.gitignore`.
    pub fn write_tree(&self) -> Result<Oid, Error> {
        let git_dir = process::out(&mut git::cmd(&["rev-parse", "--absolute-git-dir"]))?;
        let in_tree = |args: &[&str]| {
            let mut cmd = git::cmd(args);
            cmd.current_dir(self.0.dir())
                .env("GIT_DIR", &git_dir)
                .env("GIT_WORK_TREE", ".")
                .env("GIT_INDEX_FILE", self.0.scratch.path().join("index"));
            process::out(&mut cmd)
        };
        in_tree(&["add", "--all", "--force", "."])?;
        Oid::parse(&in_tree(&["write-tree"])?)
    }
}
