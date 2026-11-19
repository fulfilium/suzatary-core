//! A version, and the one step from the last release's to the next.

use std::fmt;
use std::str::FromStr;

/// `MAJOR.MINOR.PATCH`, ordered as semver orders them (the derive's field order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// The last completed release's major.
    pub major: u32,
    /// The last completed release's minor.
    pub minor: u32,
    /// The trains since `major.minor` was first tagged.
    pub patch: u32,
}

/// A version's major and minor, ordered as a version is: what a release declares, and what the
/// chain reads (the last completed release's).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Counted {
    /// The major.
    pub major: u32,
    /// The minor.
    pub minor: u32,
}

impl fmt::Display for Counted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// What `MAJOR.MINOR.PATCH` would not parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not MAJOR.MINOR.PATCH")]
pub struct NotAVersion(pub String);

/// The chain reads a `major.minor` below the last release's: the version would go down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "the chain reads {read}, below the last release {last}: a version never goes down, so the \
     release that made {last} must be completed on the chain"
)]
pub struct Behind {
    /// The last release.
    pub last: Version,
    /// What the chain reads now.
    pub read: Counted,
}

impl Version {
    /// Its major and minor, what the chain decides.
    #[must_use]
    pub fn counted(self) -> Counted {
        Counted {
            major: self.major,
            minor: self.minor,
        }
    }

    /// The release the chain makes when it reads `read`: [`Version::next`] after `last`, or the
    /// first release, `major.minor.0`, when nothing was released before.
    ///
    /// # Errors
    /// [`Behind`] when `read` is below `last`'s own `major.minor`.
    pub fn after(last: Option<Version>, read: Counted) -> Result<Version, Behind> {
        match last {
            Some(last) => last.next(read),
            None => Ok(Version {
                major: read.major,
                minor: read.minor,
                patch: 0,
            }),
        }
    }

    /// The release after `self`, when the chain reads `read`: a new `major.minor` starts at
    /// patch 0, the same one is the next patch.
    ///
    /// The answer is always above `self`, which is what makes the tags a chain.
    ///
    /// # Errors
    /// [`Behind`] when `read` is below `self`'s own `major.minor`.
    pub fn next(self, read: Counted) -> Result<Version, Behind> {
        match read.cmp(&self.counted()) {
            std::cmp::Ordering::Greater => Ok(Version {
                major: read.major,
                minor: read.minor,
                patch: 0,
            }),
            std::cmp::Ordering::Equal => Ok(Version {
                patch: self.patch.saturating_add(1),
                ..self
            }),
            std::cmp::Ordering::Less => Err(Behind { last: self, read }),
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for Version {
    type Err = NotAVersion;

    fn from_str(text: &str) -> Result<Version, NotAVersion> {
        let refused = || NotAVersion(text.to_owned());
        let mut parts = text.split('.').map(|part| {
            // No sign, no leading zero, as semver spells a number.
            if part.is_empty()
                || !part.bytes().all(|b| b.is_ascii_digit())
                || (part.len() > 1 && part.starts_with('0'))
            {
                None
            } else {
                part.parse::<u32>().ok()
            }
        });
        match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(Some(major)), Some(Some(minor)), Some(Some(patch)), None) => Ok(Version {
                major,
                minor,
                patch,
            }),
            _ => Err(refused()),
        }
    }
}
