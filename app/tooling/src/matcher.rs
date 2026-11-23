//! The program that reads a public tree for leaks.

use crate::Error;
use crate::admission::{Admission, Vouch};
use crate::repo_path::RepoPath;
use crate::{git, process};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The admitted files, the findings `.gitleaksignore` vouches for, and the secret rules, each at
/// its path in a public tree.
pub const MANIFEST: &str = "scripts/public-admitted.sha256";
pub const IGNORE: &str = ".gitleaksignore";
pub const RULES: &str = "scripts/secret-rules.toml";
pub const ADMISSION_FILES: [&str; 3] = [MANIFEST, IGNORE, RULES];

/// The `suzatary` binary that reads a tree for leaks: the deployed one, never one a tree it reads
/// built or chose, so a train cannot weaken the guard over itself.
#[derive(Clone, Debug)]
pub struct Matcher(PathBuf);

impl Matcher {
    /// The app the box runs, as this checkout's `scripts/app-bin.sh --deployed` finds it.
    pub fn deployed() -> Result<Matcher, Error> {
        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/app-bin.sh");
        let out = Command::new(script)
            .arg("--deployed")
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|e| Error::Unrun(format!("{script} did not run: {e}")))?;
        let dir = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !out.status.success() || dir.is_empty() {
            return Err(Error::Unrun(
                "there is no deployed app to read the tree with (scripts/deploy.sh <rev> first)"
                    .into(),
            ));
        }
        Ok(Matcher(Path::new(&dir).join("bin/suzatary")))
    }

    #[cfg(feature = "seams")]
    #[must_use]
    pub fn stub(path: PathBuf) -> Matcher {
        Matcher(path)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// What the admission files under `dir` vouch for; a file absent vouches for nothing.
    pub fn admissions(&self, dir: &Path) -> Result<BTreeSet<Admission>, Error> {
        let present = |name: &str| {
            let path = dir.join(name);
            path.is_file().then_some(path)
        };
        let null = || PathBuf::from("/dev/null");
        let mut leaks = Command::new(&self.0);
        leaks
            .args(["leaks", "--terms", "/dev/null", "--words", "/dev/null"])
            .args(["--links", "/dev/null", "--admissions", "--admitted"])
            .arg(present(MANIFEST).unwrap_or_else(null))
            .arg("--findings")
            .arg(present(IGNORE).unwrap_or_else(null))
            .stderr(std::process::Stdio::inherit());
        let mut admissions = process::out(&mut leaks)
            .map_err(|e| Error::Unrun(format!("the matcher read no admissions: {e}")))?
            .lines()
            .map(Admission::parse)
            .collect::<Result<BTreeSet<_>, _>>()?;
        if let Some(rules) = present(RULES) {
            // A change to the secret rules changes what passes, so their bytes are admitted as a
            // line is.
            let blob = git::hash_object(&rules)?;
            admissions.insert(Admission {
                path: RepoPath::parse(RULES)?,
                vouch: Vouch::Rules(blob.to_string()),
            });
        }
        Ok(admissions)
    }
}
