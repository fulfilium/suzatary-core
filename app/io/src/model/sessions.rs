//! The `claude` CLI's session files, as <https://code.claude.com/docs/en/claude-directory> lays
//! them out: forgetting deletes one session's, never the sign-in or another session beside it.

use std::path::{Path, PathBuf};

use suzatary_core::capability::{ModelSessions, SessionId};

/// A longer project directory name is cut to this and followed by a hash of the whole path.
const PROJECT_NAME: usize = 200;

/// Where the CLI keeps sessions: the run's configuration directory, else `CLAUDE_CONFIG_DIR`,
/// else `~/.claude`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliSessions {
    config: Option<PathBuf>,
}

impl CliSessions {
    #[must_use]
    pub fn new(config: Option<PathBuf>) -> Self {
        Self { config }
    }

    fn root(&self) -> Option<PathBuf> {
        self.config
            .clone()
            .or_else(|| std::env::var_os(super::CONFIG_DIR_VAR).map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude")))
    }
}

/// A working directory's project name: each character but an ASCII letter or digit as `-`.
fn project(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn projects(projects: &Path, cwd: &str) -> Result<Vec<PathBuf>, String> {
    let named = project(cwd);
    if named.len() <= PROJECT_NAME {
        return Ok(vec![projects.join(named)]);
    }
    let cut = &named[..PROJECT_NAME];
    Ok(listed(projects)?
        .into_iter()
        .filter(|entry| {
            entry
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(cut))
        })
        .collect())
}

/// A missing directory lists nothing.
fn listed(dir: &Path) -> Result<Vec<PathBuf>, String> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<_, _>>()
            .map_err(|failed| format!("{}: {failed}", dir.display())),
        Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(failed) => Err(format!("{}: {failed}", dir.display())),
    }
}

fn remove(path: &Path) -> Result<(), String> {
    let removed = match std::fs::symlink_metadata(path) {
        Err(missing) if missing.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(failed) => Err(failed),
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
    };
    removed.map_err(|failed| format!("{}: {failed}", path.display()))
}

impl ModelSessions for CliSessions {
    fn forget(&self, id: &SessionId, cwd: &str) -> Result<(), String> {
        let Some(root) = self.root() else {
            return Ok(());
        };
        let id = id.as_str();
        let mut doomed = vec![
            root.join("file-history").join(id),
            root.join("session-env").join(id),
            root.join("debug").join(format!("{id}.txt")),
        ];
        for project in projects(&root.join("projects"), cwd)? {
            doomed.push(project.join(id));
            // `<id>.jsonl` and its sidecars.
            let prefix = format!("{id}.");
            doomed.extend(listed(&project)?.into_iter().filter(|entry| {
                entry
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
            }));
        }
        doomed.iter().try_for_each(|path| remove(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = "6f1c2b0e-8a4d-4c55-9f3e-2b7a1d9e0c41";
    const THEIRS: &str = "0b9e3c7d-1f2a-4e6b-8c5d-7a3f9e2d1c08";

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, "words").expect("written");
    }

    /// Law: forgetting a session deletes its files and keeps the sign-in, the prompt history and
    /// another session's.
    #[test]
    fn forgetting_a_session_deletes_its_files_and_keeps_the_sign_in() {
        for cwd in [
            "/srv/suzatary/code",
            &format!("/srv/{}", "deep/".repeat(60)),
        ] {
            let dir = tempfile::tempdir().expect("scratch");
            let root = dir.path().join("config");
            let named = project(cwd);
            let project = root.join("projects").join(if named.len() > PROJECT_NAME {
                format!("{}-1a2b3c", &named[..PROJECT_NAME])
            } else {
                named
            });
            let session = |id: &str| {
                [
                    project.join(format!("{id}.jsonl")),
                    project.join(format!("{id}.orphaned-1-a.jsonl")),
                    project.join(id).join("tool-results").join("r.txt"),
                    root.join("file-history").join(id).join("f"),
                    root.join("session-env").join(id).join("e"),
                    root.join("debug").join(format!("{id}.txt")),
                ]
            };
            let stays = [root.join(".credentials.json"), root.join("history.jsonl")];
            for path in session(OURS).iter().chain(&session(THEIRS)).chain(&stays) {
                touch(path);
            }
            CliSessions::new(Some(root.clone()))
                .forget(&SessionId::new(OURS).expect("a uuid"), cwd)
                .expect("forgotten");
            for path in session(OURS) {
                assert!(!path.exists(), "{} is left", path.display());
            }
            assert!(!project.join(OURS).exists());
            for path in session(THEIRS).iter().chain(&stays) {
                assert!(path.exists(), "{} is gone", path.display());
            }
        }
    }
}
