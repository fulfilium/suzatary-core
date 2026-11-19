//! What one release is: `app/release.toml`, its version and the ids of the todos it completed,
//! its data written by the release that cuts it and read by the binary beside its executable,
//! never by a crate's source, so a cut moves no compiled input. The file is public, so it holds
//! ids alone: a title is the chain's, looked up where it is shown.

use prodrome_core::event::TodoId;
use serde::{Deserialize, Serialize};

use super::{NotAVersion, Version};

/// One release as `app/release.toml` holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Released {
    /// Its version.
    pub version: Version,
    /// The todos it completed, each once, in the order the cut named them.
    pub completes: Vec<TodoId>,
}

/// The todos a version completed, each with the words a reader knows it by: what a cut lists
/// first in its changelog section, which stays in this repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Completes(Vec<Completed>);

/// One completed todo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completed {
    /// The todo.
    pub todo: TodoId,
    /// Its action line as plain text, one line.
    pub title: String,
}

/// Why a text is not a release file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleasedError {
    /// Not the file's shape.
    #[error("release.toml: {0}")]
    Unread(String),
    /// Its version is not one.
    #[error("release.toml: {0}")]
    NotAVersion(#[from] NotAVersion),
    /// Not a todo id.
    #[error("release.toml: {0:?} is not a todo id")]
    NotATodo(String),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    completes: Vec<String>,
}

impl Released {
    /// `version`, completing `todos`, each kept once in the order given.
    #[must_use]
    pub fn new(version: Version, todos: impl IntoIterator<Item = TodoId>) -> Released {
        let mut completes: Vec<TodoId> = Vec::new();
        for todo in todos {
            if !completes.contains(&todo) {
                completes.push(todo);
            }
        }
        Released { version, completes }
    }

    /// The release `text` holds.
    ///
    /// # Errors
    /// Not the file's shape, a version that is not one, or a todo that is not an id.
    pub fn parse(text: &str) -> Result<Released, ReleasedError> {
        let file: File =
            toml::from_str(text).map_err(|e| ReleasedError::Unread(e.message().to_owned()))?;
        let version = file.version.parse()?;
        let todos = file
            .completes
            .into_iter()
            .map(|id| TodoId::new(id.clone()).map_err(|_| ReleasedError::NotATodo(id)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Released::new(version, todos))
    }

    /// The file's text as a cut writes it over `existing`, the file it replaces: the leading
    /// comment lines of `existing` as they are, then this release's data lines, which
    /// [`Released::parse`] reads back as `self`. A cut rewrites only the data, so a binary of
    /// any revision cuts a tree without moving the words its header says.
    #[must_use]
    pub fn print_over(&self, existing: &str) -> String {
        let header: String = existing
            .split_inclusive('\n')
            .take_while(|line| line.starts_with('#'))
            .collect();
        let file = File {
            version: self.version.to_string(),
            completes: self
                .completes
                .iter()
                .map(|todo| todo.as_str().to_owned())
                .collect(),
        };
        // A string and a list of strings always serialise.
        let data = toml::to_string(&file).unwrap_or_default();
        if header.is_empty() {
            data
        } else {
            format!("{header}\n{data}")
        }
    }
}

impl Completes {
    /// `todos`, each kept once, in the order given; a title is cut to its first line.
    #[must_use]
    pub fn new(todos: impl IntoIterator<Item = Completed>) -> Completes {
        let mut kept: Vec<Completed> = Vec::new();
        for Completed { todo, title } in todos {
            if !kept.iter().any(|k| k.todo == todo) {
                let title = title.lines().next().unwrap_or("").trim().to_owned();
                kept.push(Completed { todo, title });
            }
        }
        Completes(kept)
    }

    /// The changelog's lines for these, first in the version's section: one per todo, its
    /// id in the roadmap store and its title.
    #[must_use]
    pub fn changelog(&self) -> Vec<String> {
        self.0
            .iter()
            .map(|c| format!("- Completes `{}`: {}", c.todo.as_str(), c.title))
            .collect()
    }

    /// Each completed todo, in order.
    #[must_use]
    pub fn todos(&self) -> &[Completed] {
        &self.0
    }
}
