//! What the tool asks git, as a subprocess in the working directory: one reading of the
//! repository, git's own.

use crate::oid::Oid;
use crate::{Error, process};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// `git <args>`, ready for an environment or a directory before [`process::out`] runs it.
pub fn cmd<S: AsRef<OsStr>>(args: &[S]) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args);
    cmd
}

/// `git <args>`'s stdout; a failure names the command and git's words.
pub fn out<S: AsRef<OsStr>>(args: &[S]) -> Result<String, Error> {
    process::out(&mut cmd(args))
}

/// Whether `git <args>` exits 0, for a question git answers by its status alone (`rev-parse
/// --verify`, `cat-file -e`, `remote get-url`).
pub fn holds<S: AsRef<OsStr>>(args: &[S]) -> Result<bool, Error> {
    process::succeeds(&mut cmd(args), Stdio::null())
}

/// Whether `git <args>` did what it was asked, git's words on stderr shown: a fetch or a push.
pub fn act<S: AsRef<OsStr>>(args: &[S]) -> Result<bool, Error> {
    process::succeeds(&mut cmd(args), Stdio::inherit())
}

/// The repository's `.git`, shared by its worktrees.
pub fn common_dir() -> Result<PathBuf, Error> {
    out(&["rev-parse", "--path-format=absolute", "--git-common-dir"]).map(PathBuf::from)
}

pub fn top() -> Result<PathBuf, Error> {
    out(&["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

pub fn commit(name: &str) -> Result<Oid, Error> {
    Oid::parse(&out(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{name}^{{commit}}"),
    ])?)
}

/// The commit `name` names, or none where `rev-parse` says so (exit 1); any other failure is an
/// error.
pub fn commit_if(name: &str) -> Result<Option<Oid>, Error> {
    let mut rev_parse = cmd(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{name}^{{commit}}"),
    ]);
    let out = process::output(&mut rev_parse)?;
    match out.status.code() {
        Some(0) => Oid::parse(String::from_utf8_lossy(&out.stdout).trim()).map(Some),
        Some(1) if out.stdout.is_empty() => Ok(None),
        _ => Err(Error::Run {
            command: format!("git rev-parse {name}"),
            why: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        }),
    }
}

pub fn tree(name: &str) -> Result<Oid, Error> {
    Oid::parse(&out(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{name}^{{tree}}"),
    ])?)
}

/// The name git gives the file at `path` as a blob, its bytes as they are.
pub fn hash_object(path: &Path) -> Result<Oid, Error> {
    Oid::parse(&out(&[
        OsStr::new("hash-object"),
        OsStr::new("--no-filters"),
        path.as_os_str(),
    ])?)
}

/// The first-parent chain of `name`, newest first.
pub fn first_parents(name: &str) -> Result<Vec<Oid>, Error> {
    out(&["rev-list", "--first-parent", name])?
        .lines()
        .map(Oid::parse)
        .collect()
}
