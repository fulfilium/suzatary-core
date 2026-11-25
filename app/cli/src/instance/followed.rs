//! A store this deployment follows, read under its own policy and never written here: it is no
//! [`super::ChainDir`], so nothing that walks [`super::ChainDirs`] (adoption, the writers, the
//! release plan, the public push) reaches it, and its one handle is a [`Reading`], with no append.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::bail;
use prodrome_data::event::Actor;
use prodrome_data::literal::Datetime;
use prodrome_data::policy::Untrusted;
use serde::{Deserialize, Serialize};
use suzatary_base::chains::ChainName;
use suzatary_io_access::run::{Cmd, run, succeed};
use suzatary_io_chains::reading::Reading;

/// One `[[follows]]` entry as the file spells it.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Follow {
    pub(crate) name: ChainName,
    /// A git remote. One with no `:` is a path, and a relative path is resolved against the
    /// instance file's directory.
    pub(crate) remote: Remote,
    pub(crate) branch: Branch,
    /// Where the store is inside the branch's tree; the tree itself where absent.
    #[serde(default, skip_serializing_if = "Within::is_tree")]
    pub(crate) path: Within,
    #[serde(default, with = "super::file::texts")]
    pub(crate) untrusted: Vec<Actor>,
}

/// A remote git is handed as one argument: not empty, no leading `-`, no space or control.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct Remote(String);

impl TryFrom<String> for Remote {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        if text.is_empty()
            || text.starts_with('-')
            || text.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(format!(
                "[[follows]] remote {text:?} is not one word that does not start with -"
            ));
        }
        Ok(Self(text))
    }
}

impl From<Remote> for String {
    fn from(remote: Remote) -> Self {
        remote.0
    }
}

/// A branch's name, never a refspec: git's rules for a ref name, so `:`, `+`, `*`, `^`, `~`,
/// `?`, `[`, `\`, `..` and `@{` have no place in it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct Branch(String);

impl TryFrom<String> for Branch {
    type Error = String;
    fn try_from(text: String) -> Result<Self, String> {
        let refused = text.is_empty()
            || text.starts_with(['-', '/', '.'])
            || text.ends_with(['/', '.'])
            || text.strip_suffix(".lock").is_some()
            || text.contains("..")
            || text.contains("@{")
            || text.contains("//")
            || text
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || ":+*^~?[\\".contains(c));
        if refused {
            return Err(format!(
                "[[follows]] branch {text:?} is not a branch's name"
            ));
        }
        Ok(Self(text))
    }
}

impl From<Branch> for String {
    fn from(branch: Branch) -> Self {
        branch.0
    }
}

/// A relative path inside the branch's tree, every part a name: no `..`, no root.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "PathBuf", into = "PathBuf")]
pub(crate) struct Within(PathBuf);

impl Within {
    fn is_tree(&self) -> bool {
        self.0.as_os_str().is_empty()
    }
}

impl TryFrom<PathBuf> for Within {
    type Error = String;
    fn try_from(path: PathBuf) -> Result<Self, String> {
        if path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            Ok(Self(path))
        } else {
            Err(format!(
                "[[follows]] path {} is not a relative path of names",
                path.display()
            ))
        }
    }
}

impl From<Within> for PathBuf {
    fn from(within: Within) -> Self {
        within.0
    }
}

#[derive(Debug, Clone)]
pub struct Followed {
    /// The name the one list and `--chain` spell, unique beside every chain's.
    pub name: ChainName,
    /// Whose events it does not believe.
    pub untrusted: Untrusted,
    /// The remote as git is handed it, a relative path resolved.
    remote: String,
    branch: Branch,
    within: Within,
    /// The copy of the branch, in the cache tier: rebuilt by a fetch, never pushed.
    clone: PathBuf,
}

/// What the copy holds as of the last fetch that moved or confirmed it, and a refused fetch
/// since; anything the upstream gained since the confirmed fetch is missing from every reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Copied {
    pub tree: Tree,
    pub since: Since,
}

/// The copy's tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum Tree {
    /// No fetch has confirmed it: the store is absent, and a reference into it reads `∅`.
    Never,
    /// A fetch confirmed it at `commit`.
    Confirmed { commit: String, at: String },
    /// A fetch confirmed it at `commit`, and a later one began to change it and stopped: it is
    /// read as absent until a fetch confirms it again.
    Unsettled { commit: String, at: String },
}

/// What became of the fetches since the tree's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum Since {
    Nothing,
    Refused { at: String, why: String },
}

impl Copied {
    /// Whether a confirmed copy is there to read.
    #[must_use]
    pub fn held(&self) -> bool {
        match self.tree {
            Tree::Confirmed { .. } => true,
            Tree::Never | Tree::Unsettled { .. } => false,
        }
    }
}

impl Followed {
    /// Every `[[follows]]` entry of the instance file at `path`, its copy under `cache`, no name
    /// shared with `chains`.
    pub(super) fn of(
        path: &Path,
        cache: &Path,
        follows: Vec<Follow>,
        chains: &super::ChainDirs,
    ) -> anyhow::Result<Vec<Followed>> {
        let mut seen: BTreeSet<ChainName> = chains
            .iter()
            .map(|chain| chain.policy.name.clone())
            .collect();
        follows
            .into_iter()
            .map(|follow| {
                if super::RESERVED.contains(&follow.name.as_str())
                    || !seen.insert(follow.name.clone())
                {
                    bail!(
                        "{}: [[follows]] {:?} is a chain's name or a reserved one; each store of \
                         the one list has its own",
                        path.display(),
                        follow.name.as_str()
                    );
                }
                let remote = if follow.remote.0.contains(':') {
                    follow.remote.0
                } else {
                    super::Instance::data_beside(path, Path::new(&follow.remote.0))
                        .to_string_lossy()
                        .into_owned()
                };
                Ok(Followed {
                    clone: cache.join("follows").join(follow.name.as_str()),
                    name: follow.name,
                    untrusted: Untrusted::of(follow.untrusted),
                    remote,
                    branch: follow.branch,
                    within: follow.path,
                })
            })
            .collect()
    }

    /// The copy as it stands, under its own policy, with no way to append; a store directory, a
    /// directory on its way down or `objects` that is not a real directory, or an entry of
    /// `objects` that is not a regular file, is refused: a link would read another place as it.
    pub fn reading(&self) -> Result<Reading, String> {
        let store = self.clone.join(&self.within.0);
        let mut walked = self.clone.clone();
        for part in self
            .within
            .0
            .iter()
            .chain([std::ffi::OsStr::new("objects")])
        {
            walked.push(part);
            let real = std::fs::symlink_metadata(&walked).is_ok_and(|meta| meta.is_dir());
            if !real {
                return Err(format!(
                    "{} is not a directory inside the copy",
                    walked.display()
                ));
            }
        }
        let entries =
            std::fs::read_dir(&walked).map_err(|why| format!("{}: {why}", walked.display()))?;
        for entry in entries {
            let entry = entry.map_err(|why| format!("{}: {why}", walked.display()))?;
            let regular = std::fs::symlink_metadata(entry.path()).is_ok_and(|meta| meta.is_file());
            if !regular {
                return Err(format!(
                    "{} is not a regular file inside the copy",
                    entry.path().display()
                ));
            }
        }
        Ok(suzatary_io_chains::reading::chain(
            store,
            self.untrusted.clone(),
        ))
    }

    /// The record of the last fetch that moved or confirmed the copy: its instant and commit.
    fn confirmed(&self) -> PathBuf {
        self.record("confirmed")
    }

    /// The confirmed record, kept aside while a fetch changes the tree.
    fn unsettled(&self) -> PathBuf {
        self.record("unsettled")
    }

    /// The record of a refused fetch since: its instant, then git's words.
    fn refused(&self) -> PathBuf {
        self.record("refused")
    }

    fn record(&self, what: &str) -> PathBuf {
        self.clone.with_file_name(format!("{}.{what}", self.name))
    }

    /// What the copy holds, from the fetch's own records; reads nothing else and writes nothing.
    #[must_use]
    pub fn copy(&self) -> Copied {
        let read = |path: PathBuf| std::fs::read_to_string(path).ok();
        let fetched = |path: PathBuf| {
            read(path).and_then(|text| {
                let (at, commit) = text.trim().split_once(' ')?;
                Some((at.to_owned(), commit.to_owned()))
            })
        };
        let tree = match (fetched(self.confirmed()), fetched(self.unsettled())) {
            (Some((at, commit)), _) => Tree::Confirmed { commit, at },
            (None, Some((at, commit))) => Tree::Unsettled { commit, at },
            (None, None) => Tree::Never,
        };
        let since = read(self.refused()).map_or(Since::Nothing, |text| {
            let (at, why) = text.split_once('\n').unwrap_or((text.as_str(), ""));
            Since::Refused {
                at: at.to_owned(),
                why: why.trim().to_owned(),
            }
        });
        Copied { tree, since }
    }

    /// Bring the copy to the upstream head, fast-forward only, recording the outcome at `now`: a
    /// store only gains objects, so a rewritten history is refused and the copy stays, and a fetch
    /// that fails after it began to change the tree leaves the copy unconfirmed, read as no store
    /// until a fetch confirms it again, and the next fetch puts the tree back first.
    pub fn fetch(&self, now: Datetime) -> anyhow::Result<()> {
        let at = prodrome_data::instant::iso(prodrome_data::instant::instant_of(now));
        let _held = suzatary_os::durable::hold(&self.record("lock"))?;
        match self.fetched() {
            Ok(commit) => {
                std::fs::write(self.confirmed(), format!("{at} {commit}\n"))?;
                gone(&self.unsettled())?;
                gone(&self.refused())
            }
            Err(why) => {
                std::fs::write(self.refused(), format!("{at}\n{why:#}\n"))?;
                Err(why)
            }
        }
    }

    /// The fetch itself, answering the commit the copy stands at, its tree that commit's.
    fn fetched(&self) -> anyhow::Result<String> {
        if !self.clone.join(".git").is_dir() {
            std::fs::create_dir_all(&self.clone)?;
            succeed(&git(&self.clone).args(["init", "--quiet", "--template="]))?;
        }
        succeed(
            &git(&self.clone)
                .args(["fetch", "--quiet", "--no-tags", "--"])
                .arg(&self.remote)
                .arg(&self.branch.0),
        )?;
        let head = run(&git(&self.clone).args(["rev-parse", "--verify", "--quiet", "HEAD"]))?;
        if head.ok() {
            let ancestor =
                run(&git(&self.clone).args(["merge-base", "--is-ancestor", "HEAD", "FETCH_HEAD"]))?;
            if !ancestor.ok() {
                bail!(
                    "the upstream's branch does not descend from the copy: its history was rewritten"
                );
            }
        }
        // From here the tree changes: the copy is unconfirmed until it is the commit's.
        match std::fs::rename(self.confirmed(), self.unsettled()) {
            Err(absent) if absent.kind() == std::io::ErrorKind::NotFound => {}
            moved => moved?,
        }
        // Only a fetch writes the copy, under the lock this one holds: a lock left in it is
        // a killed fetch's.
        gone(&self.clone.join(".git").join("index.lock"))?;
        succeed(&git(&self.clone).args(["reset", "--hard", "--quiet", "FETCH_HEAD"]))?;
        let fetched = succeed(&git(&self.clone).args(["rev-parse", "--verify", "FETCH_HEAD"]))?;
        let head = succeed(&git(&self.clone).args(["rev-parse", "--verify", "HEAD"]))?;
        let dirty =
            succeed(&git(&self.clone).args(["status", "--porcelain", "--untracked-files=no"]))?;
        if head.out() != fetched.out() || !dirty.out().trim().is_empty() {
            bail!("the copy's tree is not the fetched commit's");
        }
        Ok(head.out().trim().to_owned())
    }
}

fn gone(path: &Path) -> anyhow::Result<()> {
    match std::fs::remove_file(path) {
        Err(absent) if absent.kind() == std::io::ErrorKind::NotFound => Ok(()),
        removed => removed.map_err(anyhow::Error::from),
    }
}

/// Git in `dir` for a store another repository keeps, reading none of the user's or system's
/// git configuration (ssh still reads the user's keys and known hosts; a followed https store
/// is public): no hook runs, a link checks out as a plain file, every object received is
/// checked, only the https, ssh and file transports are allowed, nothing prompts, and a call
/// that hangs is killed.
fn git(dir: &Path) -> Cmd {
    Cmd::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.symlinks=false",
            "-c",
            "fetch.fsckObjects=true",
            "-c",
            "transfer.fsckObjects=true",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.https.allow=always",
            "-c",
            "protocol.ssh.allow=always",
            "-c",
            "protocol.file.allow=always",
        ])
        .arg("-C")
        .arg(dir)
        .pass(&[
            "HTTPS_PROXY",
            "https_proxy",
            "NO_PROXY",
            "no_proxy",
            "SSL_CERT_FILE",
            "GIT_SSL_CAINFO",
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .timeout(Duration::from_mins(5))
}
