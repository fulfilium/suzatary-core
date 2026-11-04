//! A coding session's report stays on its branch. The poll reads it there (`Mirrors::seen` diffs
//! the branch against the default branch) and keeps its text in the session's state, so the
//! reviewer deletes it in a commit on the branch before the train lands the branch.

use std::process::Command;

use suzatary_core::coding::REPORTS_DIR;

use super::repo;

/// What `git <args>` printed in this repository, or `None` when git fails.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo())
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The tree adds no file under the reports directory against its merge base with
/// `origin/master`, tracked or untracked.
#[test]
fn reports_no_session_report_lands_on_master() {
    let Some(base) = git(&["merge-base", "HEAD", "origin/master"]) else {
        eprintln!("skipped: this checkout has no origin/master to compare with");
        return;
    };
    let directory = format!("{REPORTS_DIR}/");
    let tracked = git(&[
        "diff",
        "--name-only",
        "--diff-filter=A",
        base.trim(),
        "--",
        &directory,
    ])
    .expect("git diff runs");
    let untracked = git(&[
        "ls-files",
        "--others",
        "--exclude-standard",
        "--",
        &directory,
    ])
    .expect("git ls-files runs");
    let added: Vec<&str> = tracked.lines().chain(untracked.lines()).collect();
    assert!(
        added.is_empty(),
        "a session's report stays on its branch, where the poll reads it; delete it in a commit \
         on the branch before the train lands it: {added:?}"
    );
}
