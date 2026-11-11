//! The literal of each name an inbox act stores is its text.

use crate::drive_path::DrivePath;
use crate::repo_ids::{CommitId, RepoName, RepoPath, Rev};

prodrome_core::text_literal!(
    RepoName => |repo| String::from(repo.clone()),
    CommitId => |commit| String::from(commit.clone()),
    RepoPath => |path| String::from(path.clone()),
    Rev => |rev| String::from(rev.clone()),
    DrivePath => |path| String::from(path.clone()),
);
