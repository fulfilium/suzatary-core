//! What core's rows already hold: every path, and every admission.

use crate::admission::Admission;
use crate::core_rows::Core;
use crate::fresh::Fresh;
use crate::matcher::{ADMISSION_FILES, Matcher};
use crate::public_tree::Written;
use crate::repo_path::RepoPath;
use crate::scratch::Scratch;
use crate::{Error, git, process};
use std::collections::BTreeSet;
use std::process::Stdio;

/// What a push is compared with to find what no person has looked at yet.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Known {
    /// Core is unborn: the operator's first push is the baseline, and nothing is held against it.
    Baseline,
    /// Every path any row of core's master held, and every admission one published; once
    /// published, an admission stands.
    Held {
        paths: BTreeSet<RepoPath>,
        admissions: BTreeSet<Admission>,
    },
}

impl Known {
    pub fn read(core: &Core, matcher: &Matcher) -> Result<Known, Error> {
        let Some(tip) = core.tip() else {
            return Ok(Known::Baseline);
        };
        let tip = tip.to_string();
        let paths = process::out(&mut git::cmd(&[
            "log",
            "-z",
            "--format=",
            "--name-only",
            &tip,
        ]))?
        .split('\0')
        .map(|path| path.trim_start_matches('\n'))
        .filter(|path| !path.is_empty())
        .map(RepoPath::parse)
        .collect::<Result<_, _>>()?;
        let mut admissions = BTreeSet::new();
        let mut log = git::cmd(&["log", "--format=%H", &tip, "--"]);
        log.args(ADMISSION_FILES);
        for commit in process::out(&mut log)?.lines() {
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
            admissions.extend(matcher.admissions(row.path())?);
        }
        Ok(Known::Held { paths, admissions })
    }

    /// What `tree` holds that is not known, each path once; the tree's then join what is known,
    /// so a later tree is compared with this one too.
    pub fn fresh(&mut self, tree: &Written, matcher: &Matcher) -> Result<Vec<Fresh>, Error> {
        let Known::Held { paths, admissions } = self else {
            return Ok(Vec::new());
        };
        let mut fresh: Vec<Fresh> = Vec::new();
        for path in tree.paths()? {
            if !paths.contains(&path) {
                fresh.push(Fresh::Path(path.clone()));
            }
            paths.insert(path);
        }
        for admission in matcher.admissions(&tree.dir())? {
            if !admissions.contains(&admission) {
                fresh.push(Fresh::Admission(admission.path.clone()));
            }
            admissions.insert(admission);
        }
        fresh.sort();
        fresh.dedup();
        Ok(fresh)
    }
}
