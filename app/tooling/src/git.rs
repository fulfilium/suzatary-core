//! What the tool asks git, as a subprocess in the working directory: one reading of the
//! repository, git's own.

use crate::Error;
use crate::oid::Oid;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

/// `git <args>`'s stdout without its last newline.
fn git<S: AsRef<OsStr>>(args: &[S]) -> Result<String, Error> {
    let shown = || {
        args.iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| Error::Git {
            args: shown(),
            why: e.to_string(),
        })?;
    if !out.status.success() {
        let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(Error::Git {
            args: shown(),
            why: if said.is_empty() {
                format!("exited {}", out.status)
            } else {
                said
            },
        });
    }
    let text = String::from_utf8(out.stdout).map_err(|_| Error::Git {
        args: shown(),
        why: "its answer is not UTF-8".into(),
    })?;
    Ok(text.strip_suffix('\n').unwrap_or(&text).to_owned())
}

/// The repository's `.git`, shared by its worktrees.
pub fn common_dir() -> Result<PathBuf, Error> {
    git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]).map(PathBuf::from)
}

/// The commit `name` names.
pub fn commit(name: &str) -> Result<Oid, Error> {
    Oid::parse(&git(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{name}^{{commit}}"),
    ])?)
}

/// The tree of the commit `name` names.
pub fn tree(name: &str) -> Result<Oid, Error> {
    Oid::parse(&git(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{name}^{{tree}}"),
    ])?)
}

/// The name git gives the file at `path` as a blob.
#[cfg(feature = "seams")]
pub fn hash_object(path: &OsStr) -> Result<Oid, Error> {
    Oid::parse(&git(&[OsStr::new("hash-object"), path])?)
}
