//! The inert Drive, repositories, weather and web: test support, never production.

use suzatary_base::capability::Nothing;
use suzatary_base::drive_path::{DriveFailure, DrivePath};
use suzatary_base::repo_ids::{CommitId, RepoName, RepoPath, Rev};
use suzatary_held::file_read::FileRead;

use crate::drive::{Drive, DriveEntry, Port};
use crate::repos::{Patch, RepoRow, Visibility};
use crate::repositories::{InBranch, RepoFailure, Repositories};
use crate::repository::{Repository, RepositoryError};
use crate::weather::{Weather, WeatherError};
use crate::web::{AllowRule, Web, WebAnswer, WebFailure, WebRequest};

impl Repository for Nothing {
    fn diff_summary(&self) -> Result<String, RepositoryError> {
        Ok(String::new())
    }
}

impl Weather for Nothing {
    fn today(&self) -> Result<String, WeatherError> {
        Ok("clear all day, no umbrella".to_owned())
    }
}

impl Web for Nothing {
    fn allow_rules(&self) -> Vec<AllowRule> {
        Vec::new()
    }
    fn perform(&self, request: &WebRequest) -> Result<WebAnswer, WebFailure> {
        Err(WebFailure::Unreachable(request.sentence()))
    }
}

impl Repositories for Nothing {
    fn repos(&self) -> Result<Vec<RepoRow>, RepoFailure> {
        Ok(Vec::new())
    }
    fn resolve(&self, repo: &RepoName, _at: Option<&Rev>) -> Result<CommitId, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn tree(
        &self,
        repo: &RepoName,
        _at: &CommitId,
        _dir: &RepoPath,
    ) -> Result<Vec<String>, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn blob(
        &self,
        repo: &RepoName,
        _at: &CommitId,
        _path: &RepoPath,
    ) -> Result<Vec<u8>, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn grep(
        &self,
        repo: &RepoName,
        _at: &CommitId,
        _needle: &str,
    ) -> Result<Vec<String>, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn applies(
        &self,
        repo: &RepoName,
        _base: &CommitId,
        _patch: &Patch,
    ) -> Result<(), RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn merged(&self, repo: &RepoName, _patch: &Patch) -> Result<Option<InBranch>, RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
    fn pushable(&self, repo: &RepoName, _visibility: Visibility) -> Result<(), RepoFailure> {
        Err(RepoFailure::Unknown(repo.to_string()))
    }
}

impl Drive for Nothing {
    fn allowed_folders(&self) -> Vec<DrivePath> {
        Vec::new()
    }
    fn port(&self) -> &dyn Port {
        self
    }
    fn read(&self, file: &DrivePath) -> Result<FileRead, DriveFailure> {
        Err(DriveFailure::Missing(file.to_string()))
    }
}

/// No Drive: nothing is in it, and nothing can be put there.
impl Port for Nothing {
    fn list(&self, folder: &DrivePath) -> Result<Vec<DriveEntry>, DriveFailure> {
        Err(DriveFailure::Missing(folder.to_string()))
    }
    fn download(&self, file: &DrivePath) -> Result<Vec<u8>, DriveFailure> {
        Err(DriveFailure::Missing(file.to_string()))
    }
}
