//! What the secretary may name in the owner's Drive, by [`DrivePath`].

mod effect;
mod fenced;
mod memory;
mod port;
mod probe;
pub use effect::Drive;

use crate::drive_path::DrivePath;

pub use fenced::Fenced;
pub use memory::Memory;
pub use port::{Port, WritePort};
pub use probe::probe;

/// The largest file read, in bytes: a scanned lease is a few megabytes, and a download is
/// bounded before it starts.
pub const MAX_FILE: u64 = 25 * 1024 * 1024;

/// Whether any of `folders` holds `path`; never inside the owner's private folder
/// ([`crate::vault::FOLDER`]), whose files are read only on a tap.
#[must_use]
pub fn allowed(folders: &[DrivePath], path: &DrivePath) -> bool {
    !crate::vault::holds(path) && folders.iter().any(|folder| path.is_under(folder))
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
}
