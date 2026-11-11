//! An absolute path in the owner's Drive.
//!
//! A path comes from a model that reads attacker-controlled mail and ends up as a Drive CLI
//! argument, so [`DrivePath`] refuses anything the CLI could read as an option or a step outside
//! a folder. Folders match segment by segment, so `/my-files/Docs` never allows
//! `/my-files/Docs2`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The longest path accepted, in bytes.
pub(crate) const MAX_PATH: usize = 1024;

/// The longest segment accepted, in characters.
const MAX_SEGMENT: usize = 255;

/// Why a path was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "a Drive path is absolute (/my-files/…), without '.', '..', empty segments or control \
     characters, at most {MAX_PATH} bytes; got {0:?}"
)]
pub struct DrivePathError(pub String);

/// An absolute path in the owner's Drive: `/` or `/section/folder/…/name`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DrivePath {
    segments: Vec<String>,
}

impl DrivePath {
    /// The top: the list of sections.
    #[must_use]
    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Whether this is `/`.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// The last segment, `None` for `/`.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    /// The folder this is in, `None` for `/`.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let (_, init) = self.segments.split_last()?;
        Some(Self {
            segments: init.to_vec(),
        })
    }

    /// `name` inside this folder, when `name` is one segment this grammar
    /// takes.
    ///
    /// # Errors
    /// [`DrivePathError`] for a name holding `/`, `.`, `..` or a control
    /// character, or one that would pass `MAX_PATH`.
    pub fn join(&self, name: &str) -> Result<Self, DrivePathError> {
        let mut text = self.to_string();
        if !self.is_root() {
            text.push('/');
        }
        text.push_str(name);
        let joined: Self = text.parse()?;
        if joined.segments.len() == self.segments.len() + 1 {
            Ok(joined)
        } else {
            Err(DrivePathError(text))
        }
    }

    /// Whether this path is `folder` or anywhere under it, its names compared without case.
    #[must_use]
    pub fn is_under_ignoring_case(&self, folder: &DrivePath) -> bool {
        self.segments.len() >= folder.segments.len()
            && self
                .segments
                .iter()
                .zip(&folder.segments)
                .all(|(mine, theirs)| mine.to_lowercase() == theirs.to_lowercase())
    }

    /// Whether this path is `folder` or anywhere under it.
    #[must_use]
    pub fn is_under(&self, folder: &DrivePath) -> bool {
        self.segments.starts_with(&folder.segments)
    }
}

impl std::str::FromStr for DrivePath {
    type Err = DrivePathError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let wrong = || DrivePathError(text.to_owned());
        if text.len() > MAX_PATH || text.chars().any(char::is_control) {
            return Err(wrong());
        }
        let rest = text.strip_prefix('/').ok_or_else(wrong)?;
        if rest.is_empty() {
            return Ok(Self::root());
        }
        // One trailing slash is how a folder is often written; it names the
        // same folder.
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let segments: Vec<String> = rest.split('/').map(str::to_owned).collect();
        let segment_ok = |segment: &String| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment.chars().count() <= MAX_SEGMENT
        };
        if segments.iter().all(segment_ok) {
            Ok(Self { segments })
        } else {
            Err(wrong())
        }
    }
}

impl TryFrom<String> for DrivePath {
    type Error = DrivePathError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<DrivePath> for String {
    fn from(path: DrivePath) -> Self {
        path.to_string()
    }
}

impl fmt::Display for DrivePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.segments.is_empty() {
            return f.write_str("/");
        }
        for segment in &self.segments {
            write!(f, "/{segment}")?;
        }
        Ok(())
    }
}

/// Why a read produced nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DriveFailure {
    /// No such file or folder.
    #[error("not found in Drive: {0}")]
    Missing(String),
    /// A folder where a file was wanted, or a file too large to read.
    #[error("{0}")]
    Refused(String),
    /// The CLI could not be run, timed out, or failed, in its own words.
    #[error("Drive did not answer: {0}")]
    Unreachable(String),
    /// The CLI answered that its session expired or was never begun: it needs its sign-in at a
    /// terminal, in its own words.
    #[error("the Drive needs its sign-in: {0}")]
    SignedOut(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> DrivePath {
        text.parse().expect("a Drive path")
    }

    #[test]
    fn anything_the_cli_could_misread_is_refused() {
        for wrong in [
            "",
            "my-files/a.pdf",
            "-rf",
            "--help",
            "/my-files/../trash",
            "/my-files/./a",
            "/my-files//a",
            "//",
            "/a\nb",
            "/a\u{0}b",
        ] {
            assert!(wrong.parse::<DrivePath>().is_err(), "{wrong:?} must refuse");
        }
        assert!(
            format!("/{}", "a".repeat(MAX_SEGMENT + 1))
                .parse::<DrivePath>()
                .is_err()
        );
        assert!(
            format!("/{}", "a/".repeat(MAX_PATH))
                .parse::<DrivePath>()
                .is_err()
        );
    }

    #[test]
    fn join_takes_one_segment_only() {
        let folder = path("/my-files");
        assert_eq!(
            folder.join("a.pdf").expect("a name").to_string(),
            "/my-files/a.pdf"
        );
        assert_eq!(DrivePath::root().join("my-files").expect("a name"), folder);
        for wrong in ["", "..", "a/b", "."] {
            assert!(folder.join(wrong).is_err(), "{wrong:?} must refuse");
        }
    }

    #[test]
    fn a_path_round_trips_through_json_and_refuses_a_relative_one() {
        let json = serde_json::to_string(&path("/my-files/a b.pdf")).expect("serialises");
        assert_eq!(json, r#""/my-files/a b.pdf""#);
        assert_eq!(
            serde_json::from_str::<DrivePath>(&json).expect("round trip"),
            path("/my-files/a b.pdf")
        );
        assert!(serde_json::from_str::<DrivePath>(r#""../etc/passwd""#).is_err());
    }
}
