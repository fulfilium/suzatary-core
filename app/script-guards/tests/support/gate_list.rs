//! The merge gate's declarations as `scripts/merge-gate.sh --list` prints them. Both test targets
//! of guards mount this file at their root, beside `scratch.rs`.

use crate::scratch::repo;
use std::collections::BTreeSet;
use std::process::Command;

/// One step of `scripts/merge-gate.sh --list`.
pub struct Step {
    pub half: String,
    /// The steps of one half and lane run one after another; the lanes run side by side.
    pub lane: String,
    pub name: String,
    /// `all`, `all-but:<path>,…`, `only:<path>,…` or `unkept`; a unit step's, `crates` or
    /// `files:<dir>`, then `~` and its reads as one unit.
    pub reads: String,
    pub command: String,
}

impl Step {
    /// The reads the step is kept by as one unit: a unit step's after its `~`.
    #[must_use]
    pub fn whole(&self) -> &str {
        self.reads
            .split_once('~')
            .map_or(self.reads.as_str(), |(_, whole)| whole)
    }

    /// The paths the step declares it never reads, as one unit.
    #[must_use]
    pub fn skips(&self) -> Vec<&str> {
        self.whole()
            .strip_prefix("all-but:")
            .map(|paths| paths.split(',').collect())
            .unwrap_or_default()
    }

    /// The paths an `only:` step reads, beside the gate's own scripts and flake.
    #[must_use]
    pub fn only(&self) -> Option<Vec<&str>> {
        self.whole()
            .strip_prefix("only:")
            .map(|paths| paths.split(',').collect())
    }

    /// Whether a kept step, as one unit, reads `path`.
    #[must_use]
    pub fn reads_path(&self, path: &str) -> bool {
        match self.only() {
            Some(only) => ["scripts", "flake.nix", "flake.lock"]
                .iter()
                .chain(only.iter())
                .any(|p| path == *p || path.starts_with(&format!("{p}/"))),
            None => self.reads != "unkept" && !self.skips().iter().any(|p| path.starts_with(p)),
        }
    }
}

/// The lines `--list` prints.
fn list() -> Vec<String> {
    let out = Command::new(repo().join("scripts").join("merge-gate.sh"))
        .arg("--list")
        .output()
        .expect("merge-gate.sh runs");
    assert!(out.status.success(), "--list fails");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[must_use]
pub fn steps() -> Vec<Step> {
    list()
        .iter()
        .filter(|line| {
            !line.starts_with("together ")
                && !line.starts_with("heavy ")
                && !line.starts_with("cli_only ")
                && !line.starts_with("build ")
        })
        .map(|line| {
            let mut words = line.split_whitespace();
            let half = words.next().expect("a half").to_owned();
            let lane = words.next().expect("a lane").to_owned();
            let name = words.next().expect("a name").to_owned();
            let reads = words.next().expect("its reads").to_owned();
            let command = words.collect::<Vec<_>>().join(" ");
            Step {
                half,
                lane,
                name,
                reads,
                command,
            }
        })
        .collect()
}

/// The groups `--list` declares: steps kept or run together.
#[must_use]
pub fn togethers() -> Vec<Vec<String>> {
    list()
        .iter()
        .filter_map(|line| line.strip_prefix("together "))
        .map(|names| names.split_whitespace().map(str::to_owned).collect())
        .collect()
}

/// The steps `--list` declares heavy, in the order they take the machine: each waits until no
/// other heavy step runs.
#[must_use]
pub fn heavy_order() -> Vec<String> {
    list()
        .iter()
        .filter_map(|line| line.strip_prefix("heavy "))
        .flat_map(str::split_whitespace)
        .map(str::to_owned)
        .collect()
}

/// The heavy steps as a set.
#[must_use]
pub fn heavies() -> BTreeSet<String> {
    heavy_order().into_iter().collect()
}
