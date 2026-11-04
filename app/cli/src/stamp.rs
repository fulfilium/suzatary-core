//! What this binary runs as: its release, read at run time from the file beside it ([`BESIDE`])
//! so a release cut rebuilds nothing, and its build (`SUZATARY_BUILD`, a source hash the flake
//! sets). A cargo build has neither and says `dev`.

use std::path::{Path, PathBuf};

use suzatary_core::release::{Released, ReleasedError};

/// Where the release file is installed, relative to the directory above the executable's own
/// (`<out>/bin/suzatary` reads `<out>/share/suzatary/release.toml`).
pub const BESIDE: &str = "share/suzatary/release.toml";

/// The release a binary runs as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Release {
    /// No release file beside the executable: a cargo build, or a binary copied out alone.
    Dev,
    /// The release the file beside the executable holds.
    Released(Released),
}

/// Why the release beside an executable could not be read.
#[derive(Debug)]
pub enum Unread {
    /// The running executable's own path.
    Exe(std::io::Error),
    /// The release file is there and could not be read.
    File(PathBuf, std::io::Error),
    /// The release file does not hold a release.
    Released(ReleasedError),
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unread::Exe(e) => write!(f, "this executable's path: {e}"),
            Unread::File(path, e) => write!(f, "{}: {e}", path.display()),
            Unread::Released(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Unread {}

impl Release {
    /// The release installed beside the executable at `exe` ([`BESIDE`]).
    ///
    /// # Errors
    /// A release file that cannot be read or does not hold a release.
    pub fn beside(exe: &Path) -> Result<Release, Unread> {
        let Some(prefix) = exe.parent().and_then(Path::parent) else {
            return Ok(Release::Dev);
        };
        let path = prefix.join(BESIDE);
        match std::fs::read_to_string(&path) {
            Ok(text) => Released::parse(&text)
                .map(Release::Released)
                .map_err(Unread::Released),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Release::Dev),
            Err(e) => Err(Unread::File(path, e)),
        }
    }
}

/// The release this binary runs as, read once from beside the running executable.
///
/// # Errors
/// The executable's path or its release file could not be read, or the file holds no release.
pub fn release() -> Result<&'static Release, &'static Unread> {
    static RELEASE: std::sync::OnceLock<Result<Release, Unread>> = std::sync::OnceLock::new();
    RELEASE
        .get_or_init(|| {
            std::env::current_exe()
                .map_err(Unread::Exe)
                .and_then(|exe| Release::beside(&exe))
        })
        .as_ref()
}

/// The build this binary was stamped with, or `dev`.
#[must_use]
pub fn build() -> &'static str {
    option_env!("SUZATARY_BUILD").unwrap_or("dev")
}

/// The version string: the release's, `dev` with no release file, or what is wrong with the file.
#[must_use]
pub fn version() -> &'static str {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(|| match release() {
        Ok(Release::Released(released)) => released.version.to_string(),
        Ok(Release::Dev) => "dev".to_owned(),
        Err(unread) => format!("unreadable ({unread})"),
    })
}
