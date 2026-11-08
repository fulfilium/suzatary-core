//! Where the data repository is pushed, and whether it was.

use crate::health::Failure;
use crate::health::{Health, Instant, Outcome};

/// Where the data repository is pushed; a failed read is `Unread`, never `Nowhere`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pushed {
    /// To its `origin` on a host.
    To(Remote),
    /// To its `origin` in a folder, as written.
    Folder(String),
    /// No `origin`, or no git repository.
    Nowhere,
    /// The configuration could not be read, and why.
    Unread(String),
}

impl Pushed {
    /// Where `origin` is; an unparsed URL is never repeated, as it may hold a credential.
    #[must_use]
    pub fn of_git_config(config: &str) -> Self {
        let mut origin = false;
        let url = config.lines().map(str::trim).find_map(|line| {
            if line.starts_with('[') {
                origin = line == "[remote \"origin\"]";
                return None;
            }
            let (key, value) = line.split_once('=')?;
            (origin && key.trim() == "url").then(|| value.trim())
        });
        match url {
            None | Some("") => Self::Nowhere,
            Some(url) => match url.strip_prefix("file://") {
                Some(path) => Self::Folder(path.to_owned()),
                None if !url.contains("://") && !scp_like(url) => Self::Folder(url.to_owned()),
                None => Remote::parse(url).map_or_else(
                    || Self::Unread("an origin that names no host and path".to_owned()),
                    Self::To,
                ),
            },
        }
    }

    /// Whether the push token is needed: for GitHub, or when unread.
    #[must_use]
    pub(crate) fn needs_push_token(&self) -> bool {
        match self {
            Pushed::To(remote) => remote.host().eq_ignore_ascii_case("github.com"),
            Pushed::Unread(_) => true,
            Pushed::Folder(_) | Pushed::Nowhere => false,
        }
    }

    /// The directory a `.git` file's `gitdir:` line points at.
    #[must_use]
    pub fn gitdir(dot_git: &str) -> Option<&str> {
        dot_git
            .lines()
            .find_map(|line| line.trim().strip_prefix("gitdir:"))
            .map(str::trim)
            .filter(|dir| !dir.is_empty())
    }
}

/// scp-like `host:path`: a colon before any slash.
fn scp_like(url: &str) -> bool {
    url.split_once(':')
        .is_some_and(|(host, _)| !host.is_empty() && !host.contains('/'))
}

/// A git remote as `host/path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote(String);

impl Remote {
    /// The host and path a URL names, credentials, port and `.git` dropped.
    #[must_use]
    pub fn parse(url: &str) -> Option<Self> {
        let url = url.trim();
        let (host, path) = match url.split_once("://") {
            Some(("file", _)) => return None,
            Some((_, rest)) => rest.split_once('/')?,
            None => url
                .split_once(':')
                .filter(|(host, _)| !host.contains('/'))?,
        };
        let host = host.rsplit('@').next()?.split(':').next()?;
        let path = path.trim_matches('/').trim_end_matches(".git");
        (!host.is_empty() && !path.is_empty()).then(|| Self(format!("{host}/{path}")))
    }

    /// The host and path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The host alone.
    #[must_use]
    pub fn host(&self) -> &str {
        self.0
            .split_once('/')
            .map_or(self.0.as_str(), |(host, _)| host)
    }
}

/// A ref's last move, by its reflog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    /// The commit.
    pub commit: String,
    /// When it moved there, UTC.
    pub at: Instant,
}

impl Tip {
    /// The reflog's last move; `None` if no line parses.
    #[must_use]
    pub fn of_reflog(log: &str) -> Option<Self> {
        let line = log.lines().rev().find(|line| !line.trim().is_empty())?;
        let (moved, _message) = line.split_once('\t').unwrap_or((line, ""));
        let mut fields = moved.split(' ');
        let (_old, commit) = (fields.next()?, fields.next()?);
        let seconds: i64 = moved.rsplit(' ').nth(1)?.parse().ok()?;
        Some(Self {
            commit: commit.to_owned(),
            at: chrono::DateTime::from_timestamp(seconds, 0)?.naive_utc(),
        })
    }
}

/// The pushes' health: success at the upstream's last move, failure since if the head differs.
/// `None` without an upstream.
#[must_use]
pub fn pushes(head: &Tip, upstream: Option<&Tip>) -> Option<Health> {
    let upstream = upstream?;
    let pushed = Health::of(upstream.at, Outcome::Succeeded, "");
    Some(if head.commit == upstream.commit {
        pushed
    } else {
        pushed.join(&Health::of(
            head.at,
            Outcome::Failed(Failure::Unclassified),
            "its latest commit is not on its remote yet",
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every spelling of one remote reads the same, and a credential in the URL is never kept.
    #[test]
    fn a_remote_reads_as_host_and_path_and_drops_its_credential() {
        for url in [
            "https://forge.example.test/ada/data.git",
            "https://ada:token-not-shown@forge.example.test/ada/data",
            "ssh://git@forge.example.test:22/ada/data.git",
            "git@forge.example.test:ada/data.git",
        ] {
            assert_eq!(
                Remote::parse(url).map(|remote| remote.0),
                Some("forge.example.test/ada/data".to_owned()),
                "{url}"
            );
        }
        assert_eq!(Remote::parse("/srv/git/data"), None);
        assert_eq!(Remote::parse("../data"), None);
        assert_eq!(Remote::parse("file:///srv/git/data.git"), None);
    }

    #[test]
    fn the_origin_is_read_from_a_git_config() {
        let origin = |url: &str| {
            Pushed::of_git_config(&format!(
                "[core]\n\turl = nothing\n[remote \"backup\"]\n\turl = https://example.test/b\n\
                 [remote \"origin\"]\n\turl = {url}\n"
            ))
        };
        assert_eq!(
            origin("git@example.test:ada/data.git"),
            Pushed::To(Remote("example.test/ada/data".to_owned()))
        );
        assert_eq!(
            origin("/srv/git/data.git"),
            Pushed::Folder("/srv/git/data.git".to_owned())
        );
        assert_eq!(
            origin("file:///srv/git/data.git"),
            Pushed::Folder("/srv/git/data.git".to_owned())
        );
        assert_eq!(origin("../data"), Pushed::Folder("../data".to_owned()));
        // A URL with a credential and no path is refused whole, and never repeated.
        for refused in ["https://user:tok@host", "git@host:", "ssh://host"] {
            let read = origin(refused);
            assert_eq!(
                read,
                Pushed::Unread("an origin that names no host and path".to_owned()),
                "{refused}"
            );
            assert!(!format!("{read:?}").contains("tok"));
        }
        assert_eq!(Pushed::of_git_config("[core]\n"), Pushed::Nowhere);
    }
}
