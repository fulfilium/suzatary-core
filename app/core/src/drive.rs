//! What the secretary may name in the owner's Drive.
//!
//! A path comes from a model that reads attacker-controlled mail and ends up as a Drive CLI
//! argument, so [`DrivePath`] refuses anything the CLI could read as an option or a step outside
//! a folder. Folders match segment by segment, so `/my-files/Docs` never allows
//! `/my-files/Docs2`.

mod fenced;
mod memory;
mod port;
mod probe;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use fenced::Fenced;
pub use memory::Memory;
pub use port::{Port, WritePort};
pub use probe::{SOURCE, probe};

/// The longest path accepted, in bytes.
pub const MAX_PATH: usize = 1024;

/// The largest file read, in bytes: a scanned lease is a few megabytes, and a download is
/// bounded before it starts.
pub const MAX_FILE: u64 = 25 * 1024 * 1024;

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
    /// character, or one that would pass [`MAX_PATH`].
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

/// Whether any of `folders` holds `path`; never inside the owner's private folder
/// ([`crate::vault::FOLDER`]), whose files are read only on a tap.
#[must_use]
pub fn allowed(folders: &[DrivePath], path: &DrivePath) -> bool {
    !crate::vault::holds(path) && folders.iter().any(|folder| path.is_under(folder))
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

/// What a listed entry is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// A folder, or one of the top-level sections.
    Folder,
    /// A file, the only thing `drive_read` reads.
    File,
    /// Anything else the CLI names (an album, a photo), by its word.
    Other(String),
}

impl EntryKind {
    /// The word a listing shows.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            EntryKind::Folder => "folder",
            EntryKind::File => "file",
            EntryKind::Other(word) => word,
        }
    }
}

/// One entry of a folder, as `drive_list` answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveEntry {
    /// Where it is.
    pub path: DrivePath,
    /// Folder, file, or what else.
    pub kind: EntryKind,
    /// Its size in bytes, when the CLI said.
    pub size: Option<u64>,
    /// When it last changed, as the CLI printed it, when it said.
    pub modified: Option<String>,
}

/// The media type a file's name says it holds, when it is one a model reads:
/// a PDF, an image, or plain text in one of its common spellings. Anything
/// else, a `.docx` included, is `None`.
#[must_use]
pub fn mime_of(name: &str) -> Option<&'static str> {
    let (_, extension) = name.rsplit_once('.')?;
    Some(match extension.to_ascii_lowercase().as_str() {
        "pdf" => "application/pdf",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "txt" | "text" | "log" | "md" | "markdown" | "typ" | "tex" | "toml" | "yaml" | "yml"
        | "json" | "eml" | "ics" | "vcf" => "text/plain",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "html" | "htm" => "text/html",
        "xml" => "text/xml",
        _ => return None,
    })
}

/// A file's extension as a refusal names it, "" when it has none.
#[must_use]
pub fn extension_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

// The literal of each is its text, as an inbox act stores it.
prodrome_core::text_literal!(
    DrivePath => |path| String::from(path.clone()),
);

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
    fn a_folder_allows_what_is_under_it_segment_by_segment() {
        let folder = path("/my-files/Docs");
        assert!(path("/my-files/Docs").is_under(&folder));
        assert!(path("/my-files/Docs/tax/w2.pdf").is_under(&folder));
        assert!(!path("/my-files/Docs2/w2.pdf").is_under(&folder));
        assert!(!path("/my-files").is_under(&folder));
        assert!(!path("/shared-with-me/Docs/w2.pdf").is_under(&folder));
        assert!(allowed(
            &[path("/trash"), folder.clone()],
            &path("/my-files/Docs/a.txt")
        ));
        assert!(!allowed(&[], &path("/my-files/Docs/a.txt")));
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
