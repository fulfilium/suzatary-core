//! What this binary runs as: the package it was installed from, its release, read at run time
//! from the file beside it ([`BESIDE`]) so a release cut rebuilds nothing, and its build
//! (`SUZATARY_BUILD`, a source hash the flake sets). A cargo build has none of them and says
//! `dev`.

use std::path::{Path, PathBuf};

use suzatary_release::release::{Released, ReleasedError};

use suzatary_server::serve::{Build, Stamp};

/// Where the release file is installed, relative to the package's `<out>`.
pub const BESIDE: &str = "share/suzatary/release.toml";

/// Where the package installs gitleaks, which door 1 runs, relative to its `<out>`.
pub const GITLEAKS: &str = "libexec/suzatary/gitleaks";

/// Where the package installs gitleaks's rules, relative to its `<out>`.
pub const SECRET_RULES: &str = "share/suzatary/secret-rules.toml";

/// Where the package links the Typst package path a drawing imports the design system from,
/// relative to its `<out>`; a cargo build reads `TYPST_PACKAGE_PATH`, which the flake's shells
/// set to the same path.
pub const TYPST_PACKAGES: &str = "share/suzatary/typst-packages";

/// The package an executable was installed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Package {
    /// No `share/suzatary` above the executable: a cargo build, or a binary copied out alone.
    Dev,
    /// `<out>`, whose `bin/suzatary` the executable is.
    Installed(PathBuf),
}

impl Package {
    /// The package of the executable at `exe`.
    #[must_use]
    pub fn of(exe: &Path) -> Package {
        exe.parent()
            .and_then(Path::parent)
            .filter(|out| out.join("share/suzatary").is_dir())
            .map_or(Package::Dev, |out| Package::Installed(out.to_owned()))
    }

    /// The Typst package path a drawing reads: the package's own, else `TYPST_PACKAGE_PATH`.
    ///
    /// # Errors
    /// A cargo build run where `TYPST_PACKAGE_PATH` is unset.
    pub fn typst_packages(&self) -> anyhow::Result<PathBuf> {
        match self {
            Package::Installed(out) => Ok(out.join(TYPST_PACKAGES)),
            Package::Dev => std::env::var_os("TYPST_PACKAGE_PATH")
                .map(PathBuf::from)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "a cargo build reads the design system from TYPST_PACKAGE_PATH, which is \
                         unset: enter nix develop .#triage"
                    )
                }),
        }
    }

    /// The running executable's package, looked up once.
    #[must_use]
    pub fn running() -> &'static Package {
        static PACKAGE: std::sync::OnceLock<Package> = std::sync::OnceLock::new();
        PACKAGE
            .get_or_init(|| std::env::current_exe().map_or(Package::Dev, |exe| Package::of(&exe)))
    }
}

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
        let Package::Installed(out) = Package::of(exe) else {
            return Ok(Release::Dev);
        };
        let path = out.join(BESIDE);
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

/// What `GET /api/about` names of this process: its release, read once, and `build`.
#[must_use]
pub fn stamped(build: Build) -> Stamp {
    Stamp {
        release: release()
            .map(|release| match release {
                Release::Released(released) => Some(released),
                Release::Dev => None,
            })
            .map_err(ToString::to_string),
        build,
    }
}
