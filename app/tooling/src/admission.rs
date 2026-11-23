//! One thing a tree's admission files vouch for.

use crate::Error;
use crate::repo_path::RepoPath;
use std::fmt;

/// A line of the admitted files, a fingerprint of `.gitleaksignore` or the secret rules' bytes,
/// at the path it vouches for: `<path>\t<word>` as `suzatary leaks --admissions` prints it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Admission {
    pub path: RepoPath,
    pub vouch: Vouch,
}

/// What an admission vouches for: `admitted <digest>`, `finding <fingerprint>` or `rules <blob>`.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Vouch {
    /// A person looked at these bytes, and they may be public; the path says where they stand.
    Bytes(Digest),
    Finding(String),
    Rules(String),
}

/// A SHA-256 as `sha256sum` prints it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Digest(String);

impl Digest {
    pub fn parse(text: &str) -> Result<Digest, Error> {
        if text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Digest(text.to_owned()))
        } else {
            Err(Error::Malformed(format!(
                "{text:?} is not a SHA-256 digest"
            )))
        }
    }

    #[must_use]
    pub fn of(bytes: &[u8]) -> Digest {
        use sha2::Digest as _;
        use std::fmt::Write as _;
        Digest(
            sha2::Sha256::digest(bytes)
                .iter()
                .fold(String::with_capacity(64), |mut hex, b| {
                    let _ = write!(hex, "{b:02x}");
                    hex
                }),
        )
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Admission {
    pub fn parse(line: &str) -> Result<Admission, Error> {
        let malformed = || {
            Error::Malformed(format!(
                "{line:?} is not an admission: <path>\\t<admitted|finding|rules> <word>"
            ))
        };
        let (path, word) = line.split_once('\t').ok_or_else(malformed)?;
        let vouch = match word.split_once(' ').ok_or_else(malformed)? {
            ("admitted", digest) => Vouch::Bytes(Digest::parse(digest)?),
            ("finding", fingerprint) => Vouch::Finding(fingerprint.to_owned()),
            ("rules", blob) => Vouch::Rules(blob.to_owned()),
            _ => return Err(malformed()),
        };
        Ok(Admission {
            path: RepoPath::parse(path)?,
            vouch,
        })
    }
}
