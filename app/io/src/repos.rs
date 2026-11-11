//! The owner's repositories, as bare mirrors read through `git` subprocesses.
//!
//! Only declared repositories the token lists are reachable; reading a mirror never executes a
//! file in it. The token never touches argv or a file: it reaches `git` through `GIT_CONFIG_*`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use suzatary_core::capability::Secret;
use suzatary_core::coding::{BRANCH_PREFIX, Branch, REPORTS_DIR, ReportFile, Seen};
use suzatary_core::coding_report::{FileStat, Lines};
use suzatary_core::repo_ids::{CommitId, RepoName, RepoPath, Rev};
use suzatary_core::repos::{Declared, Patch, RepoRow, RepoTarget, TOKEN, Visibility};
use suzatary_core::repositories::{InBranch, MAX_FILE_BYTES, RepoFailure, Repositories};

use crate::http::{Http, Req, Ureq};
use crate::run::{Cmd, Ran, RunError, run, succeed};
use crate::scratch::Scratch;
use crate::secrets::Resolves;

/// How long a listing or a fetch stays fresh.
const FRESH: Duration = Duration::from_mins(2);

const REMOTE: Duration = Duration::from_mins(5);

const MAX_LINES: usize = 400;

/// Where the repositories come from.
pub enum Source {
    /// GitHub, with the token.
    GitHub,
    /// Bare repositories at `<base>/<owner>/<name>.git`, public on `main`, with no token.
    Local { base: PathBuf },
}

pub struct Mirrors {
    root: PathBuf,
    source: Source,
    declared: Declared,
    secrets: Box<dyn Resolves + Send + Sync>,
    http: Ureq,
    listed: Mutex<Option<(Instant, Vec<RepoRow>)>>,
    fetched: Mutex<HashMap<RepoName, Instant>>,
}

impl Mirrors {
    #[must_use]
    pub fn new(
        root: impl Into<PathBuf>,
        source: Source,
        declared: Declared,
        secrets: Box<dyn Resolves + Send + Sync>,
    ) -> Self {
        Self {
            root: root.into(),
            source,
            declared,
            secrets,
            http: Ureq::new(),
            listed: Mutex::new(None),
            fetched: Mutex::new(HashMap::new()),
        }
    }

    fn token(&self) -> Result<Secret, RepoFailure> {
        self.secrets
            .get(TOKEN, "the secretary reading the owner's repositories")
            .map_err(|locked| RepoFailure::Unavailable(locked.to_string()))?
            .ok_or_else(|| {
                RepoFailure::Unavailable(format!("{TOKEN} does not resolve on this box"))
            })
    }

    /// Refuses an undeclared `repo` before anything is asked.
    fn row(&self, repo: &RepoName) -> Result<RepoRow, RepoFailure> {
        if self.declared.target(repo).is_none() {
            return Err(RepoFailure::Unknown(repo.to_string()));
        }
        self.repos()?
            .into_iter()
            .find(|row| &row.repo == repo)
            .ok_or_else(|| RepoFailure::Unknown(repo.to_string()))
    }

    /// `git` with the token in its environment, never in argv.
    fn git(token: Option<&Secret>) -> Cmd {
        let command = Cmd::new("git")
            .pass(crate::run::NETWORK)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        let Some(token) = token else {
            return command;
        };
        let basic = base64::engine::general_purpose::STANDARD
            .encode(format!("x-access-token:{}", token.expose()));
        command
            .env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("AUTHORIZATION: basic {basic}"),
            )
            .env("GIT_CONFIG_KEY_1", "credential.helper")
            .env("GIT_CONFIG_VALUE_1", "")
    }

    fn dir(&self, repo: &RepoName) -> PathBuf {
        self.root
            .join(repo.owner())
            .join(format!("{}.git", repo.name()))
    }

    fn mirror(&self, repo: &RepoName) -> Result<PathBuf, RepoFailure> {
        self.row(repo)?;
        let dir = self.dir(repo);
        let fresh = self
            .fetched
            .lock()
            .map_err(|_| RepoFailure::Unavailable("the fetch table is poisoned".to_owned()))?
            .get(repo)
            .is_some_and(|at| at.elapsed() < FRESH);
        if fresh && dir.is_dir() {
            return Ok(dir);
        }
        let (token, url) = self.remote(repo)?;
        if !dir.is_dir() {
            std::fs::create_dir_all(&dir).map_err(|e| RepoFailure::Unavailable(e.to_string()))?;
            Self::local(&dir, &["init", "--bare", "--quiet"])?;
        }
        // Never `--mirror`: `refs/pull/*` holds commits from anyone's forks.
        succeed(
            &Self::git(token.as_ref())
                .arg("--git-dir")
                .arg(&dir)
                .args([
                    "fetch",
                    "--prune",
                    "--quiet",
                    &url,
                    "+refs/heads/*:refs/heads/*",
                    "+refs/tags/*:refs/tags/*",
                ])
                .timeout(REMOTE),
        )?;
        let default = format!("refs/heads/{}", self.row(repo)?.default_branch);
        Self::local(&dir, &["symbolic-ref", "HEAD", &default])?;
        self.fetched
            .lock()
            .map_err(|_| RepoFailure::Unavailable("the fetch table is poisoned".to_owned()))?
            .insert(repo.clone(), Instant::now());
        Ok(dir)
    }

    /// No token: every read is local.
    fn local(dir: &Path, args: &[&str]) -> Result<Ran, RunError> {
        succeed(&Self::git(None).arg("--git-dir").arg(dir).args(args))
    }

    /// The exit status is the answer.
    fn asked(dir: &Path, args: &[&str]) -> Result<Ran, RunError> {
        run(&Self::git(None).arg("--git-dir").arg(dir).args(args))
    }

    /// Applies the patch in a scratch index, `how` it says, touching no ref.
    fn staged(
        &self,
        dir: &Path,
        base: &CommitId,
        patch: &Patch,
        how: Apply,
    ) -> Result<Index, RepoFailure> {
        let index = Index(self.scratch()?);
        let indexed = || {
            Self::git(None)
                .env("GIT_INDEX_FILE", index.path())
                .arg("--git-dir")
                .arg(dir)
        };
        succeed(&indexed().args(["read-tree", base.as_str()]))
            .map_err(refused_as_missing(|| format!("commit {}", base.short())))?;
        let applied = run(&indexed()
            .args(["apply", "--cached", "--whitespace=nowarn"])
            .args(match how {
                Apply::Exact => &[][..],
                Apply::ThreeWay => &["--3way"][..],
                Apply::Reverse => &["--reverse"][..],
            })
            .arg("-")
            .stdin(patch.as_str()))
        .map_err(|e| RepoFailure::Unavailable(e.to_string()))?;
        if !applied.ok() {
            return Err(RepoFailure::Conflict(applied.err().trim().to_owned()));
        }
        Ok(index)
    }

    /// The tree an index holds.
    fn tree_of(dir: &Path, index: &Index) -> Result<String, RepoFailure> {
        Ok(succeed(
            &Self::git(None)
                .env("GIT_INDEX_FILE", index.path())
                .arg("--git-dir")
                .arg(dir)
                .arg("write-tree"),
        )?
        .out()
        .trim()
        .to_owned())
    }
}

/// How a patch is applied to a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Apply {
    /// Its context must match the commit exactly, as a publish needs.
    Exact,
    /// Merged with what changed since, so a change the commit already holds applies as nothing.
    /// It needs the blobs the patch's `index` lines name; without them it applies exactly.
    ThreeWay,
    /// Undone: it applies exactly when the commit holds the patch's result.
    Reverse,
}

impl Mirrors {
    /// Every `claude/` branch of `repo`; one that does not parse is skipped.
    pub fn seen(&self, repo: &RepoName) -> Result<Vec<Seen>, RepoFailure> {
        let default = format!("refs/heads/{}", self.row(repo)?.default_branch);
        let dir = self.mirror(repo)?;
        let refs = Self::local(
            &dir,
            &[
                "for-each-ref",
                "--format=%(refname:strip=2) %(objectname)",
                &format!("refs/heads/{BRANCH_PREFIX}"),
            ],
        )?;
        let mut seen = Vec::new();
        for line in lines(&refs) {
            let Some((name, tip)) = line.split_once(' ') else {
                continue;
            };
            let (Ok(branch), Ok(tip)) =
                (name.parse::<Branch>(), CommitId::try_from(tip.to_owned()))
            else {
                continue;
            };
            let merged = Self::asked(
                &dir,
                &["merge-base", "--is-ancestor", tip.as_str(), &default],
            )
            .map_err(|e| RepoFailure::Unavailable(e.to_string()))?
            .ok();
            let range = if merged {
                format!(
                    "{}...{}",
                    Self::before_merge(&dir, &tip, &default)?,
                    tip.as_str()
                )
            } else {
                format!("{default}...{}", tip.as_str())
            };
            let reports = lines(&Self::local(
                &dir,
                &[
                    "diff",
                    "--name-only",
                    &range,
                    "--",
                    &format!("{REPORTS_DIR}/"),
                ],
            )?)
            .into_iter()
            .filter(|path| {
                Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            })
            .filter_map(|path| {
                let text = Self::local(
                    &dir,
                    &["cat-file", "blob", &format!("{}:{path}", tip.as_str())],
                )
                .map_err(RepoFailure::from)
                .ok()?;
                Some(ReportFile::read(RepoPath::try_from(path).ok()?, text.out()))
            })
            .collect();
            let stat = lines(&Self::local(&dir, &["diff", "--numstat", &range])?)
                .iter()
                .filter_map(|line| file_stat(line))
                .collect();
            seen.push(Seen {
                branch,
                tip,
                merged,
                reports,
                stat,
            });
        }
        Ok(seen)
    }
}

impl Mirrors {
    /// The default branch before it took `tip`; `<that>...<tip>` is the branch's own commits.
    fn before_merge(dir: &Path, tip: &CommitId, default: &str) -> Result<String, RepoFailure> {
        let line = Self::local(
            dir,
            &[
                "rev-list",
                "--first-parent",
                "--ancestry-path",
                "--reverse",
                &format!("{}..{default}", tip.as_str()),
            ],
        )?;
        let oldest = lines(&line).into_iter().next();
        let parents = |commit: &str| -> Result<Vec<String>, RepoFailure> {
            let printed = Self::local(dir, &["rev-list", "--parents", "-n", "1", commit])?;
            Ok(printed
                .out()
                .split_whitespace()
                .skip(1)
                .map(str::to_owned)
                .collect())
        };
        let took = match oldest {
            Some(child) if parents(&child)?.first().map(String::as_str) != Some(tip.as_str()) => {
                child
            }
            Some(_) | None => tip.as_str().to_owned(),
        };
        Ok(format!("{took}^1"))
    }
}

fn file_stat(line: &str) -> Option<FileStat> {
    let mut parts = line.splitn(3, '\t');
    let (added, removed, path) = (parts.next()?, parts.next()?, parts.next()?);
    let lines = match (added.parse(), removed.parse()) {
        (Ok(added), Ok(removed)) => Lines::Text { added, removed },
        _ => Lines::Binary,
    };
    Some(FileStat {
        path: RepoPath::try_from(path.to_owned()).ok()?,
        lines,
    })
}

/// A finished publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub commit: CommitId,
    pub branch: String,
    pub pull: PullRequest,
}

/// Where a publish stopped; publishing again finishes it, doing nothing twice.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Stopped {
    /// Before any push, for a cause a publish again meets; a branch is never forced.
    #[error("{0}")]
    Refused(String),
    /// Before any push, for a cause that can change.
    #[error("{0}")]
    Failed(String),
    /// After the push, which may have landed: a pull request may be open at `link`.
    #[error("{failure}")]
    Pushed {
        branch: String,
        commit: CommitId,
        failure: RepoFailure,
        /// The repository's pull requests from `branch`.
        link: String,
    },
}

/// A failure before any push, by whether a publish again meets it.
impl From<RepoFailure> for Stopped {
    fn from(failure: RepoFailure) -> Self {
        match failure {
            RepoFailure::Conflict(_)
            | RepoFailure::Public(_)
            | RepoFailure::Missing(_)
            | RepoFailure::TooLarge { .. } => Stopped::Refused(failure.to_string()),
            RepoFailure::Unknown(_) | RepoFailure::Unavailable(_) => {
                Stopped::Failed(failure.to_string())
            }
        }
    }
}

enum Commit {
    Pushed(CommitId),
    Made(CommitId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    /// Its page.
    pub url: String,
}

/// The commit author: the box's git identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    /// `user.name`.
    pub name: String,
    /// `user.email`.
    pub email: String,
}

impl Author {
    #[must_use]
    pub fn of_this_box() -> Option<Self> {
        let get = |key: &str| {
            run(&Cmd::new("git").args(["config", "--get", key]))
                .ok()
                .filter(Ran::ok)
                .map(|ran| ran.out().trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        Some(Self {
            name: get("user.name")?,
            email: get("user.email")?,
        })
    }
}

const CHECK_TIMEOUT: Duration = Duration::from_mins(10);

impl Mirrors {
    /// What the instance file declares about `repo`.
    #[must_use]
    pub fn target(&self, repo: &RepoName) -> Option<RepoTarget> {
        self.declared.target(repo).cloned()
    }

    /// Publish `patch` as a pull request on a new `branch`, never forced; nothing lands on the
    /// default branch.
    pub fn publish(
        &self,
        repo: &RepoName,
        base: &CommitId,
        patch: &Patch,
        message: &str,
        author: &Author,
        branch: &str,
    ) -> Result<Published, Stopped> {
        let commit = match self.commit(repo, base, patch, message, author, branch)? {
            Commit::Pushed(id) => id,
            Commit::Made(id) => {
                self.push(repo, &id, branch)
                    .map_err(|failure| self.pushed(repo, branch, &id, failure))?;
                id
            }
        };
        let pull = self
            .pull(repo, branch, &commit, message)
            .map_err(|failure| self.pushed(repo, branch, &commit, failure))?;
        Ok(Published {
            commit,
            branch: branch.to_owned(),
            pull,
        })
    }

    fn pushed(
        &self,
        repo: &RepoName,
        branch: &str,
        commit: &CommitId,
        failure: RepoFailure,
    ) -> Stopped {
        let link = match &self.source {
            Source::GitHub => {
                format!("https://github.com/{repo}/pulls?q=is%3Apr+head%3A{branch}")
            }
            Source::Local { .. } => self
                .remote(repo)
                .map_or_else(|_| String::new(), |(_, url)| format!("{url}#{branch}")),
        };
        Stopped::Pushed {
            branch: branch.to_owned(),
            commit: commit.clone(),
            failure,
            link,
        }
    }

    /// Reuses the commit `branch` holds when it is this change, whose check already passed.
    fn commit(
        &self,
        repo: &RepoName,
        base: &CommitId,
        patch: &Patch,
        message: &str,
        author: &Author,
        branch: &str,
    ) -> Result<Commit, Stopped> {
        let (dir, tree, check) = self.staged_tree(repo, base, patch).map_err(Stopped::from)?;
        match Self::held(&dir, branch).map_err(Stopped::from)? {
            Some((id, held_tree, parent)) if held_tree == tree && parent == base.as_str() => {
                Ok(Commit::Pushed(id))
            }
            Some(_) => Err(Stopped::Refused(format!(
                "the branch {branch} already holds another commit, which is never forced"
            ))),
            None => {
                if let Some(check) = &check {
                    self.checked_tree(&dir, &tree, check)?;
                }
                let made = succeed(
                    &Self::git(None)
                        .env("GIT_AUTHOR_NAME", &author.name)
                        .env("GIT_AUTHOR_EMAIL", &author.email)
                        .env("GIT_COMMITTER_NAME", &author.name)
                        .env("GIT_COMMITTER_EMAIL", &author.email)
                        .arg("--git-dir")
                        .arg(&dir)
                        .args(["commit-tree", &tree, "-p", base.as_str(), "-F", "-"])
                        .stdin(message),
                )
                .map_err(|e| Stopped::from(RepoFailure::from(e)))?;
                CommitId::try_from(made.out().trim().to_owned())
                    .map(Commit::Made)
                    .map_err(|e| Stopped::Failed(e.to_string()))
            }
        }
    }

    fn staged_tree(
        &self,
        repo: &RepoName,
        base: &CommitId,
        patch: &Patch,
    ) -> Result<(PathBuf, String, Option<String>), RepoFailure> {
        // Asked again: the instance file or the repository may have changed.
        self.declared.pushable(repo, self.row(repo)?.visibility)?;
        let check = self
            .target(repo)
            .ok_or_else(|| RepoFailure::Unknown(repo.to_string()))?
            .check;
        self.fetched
            .lock()
            .map_err(|_| RepoFailure::Unavailable("the fetch table is poisoned".to_owned()))?
            .remove(repo);
        let dir = self.mirror(repo)?;
        let index = self.staged(&dir, base, patch, Apply::Exact)?;
        let tree = Self::tree_of(&dir, &index)?;
        Ok((dir, tree, check))
    }

    /// Valid right after a fetch.
    fn held(dir: &Path, branch: &str) -> Result<Option<(CommitId, String, String)>, RepoFailure> {
        let held = Self::asked(
            dir,
            &[
                "show",
                "-s",
                "--format=%H %T %P",
                &format!("refs/heads/{branch}"),
            ],
        )?;
        if !held.ok() {
            return Ok(None);
        }
        let out = held.out();
        let mut fields = out.split_whitespace();
        let id = CommitId::try_from(fields.next().unwrap_or_default().to_owned())
            .map_err(|e| RepoFailure::Unavailable(e.to_string()))?;
        let tree = fields.next().unwrap_or_default().to_owned();
        let parents = fields.collect::<Vec<_>>().join(" ");
        Ok(Some((id, tree, parents)))
    }

    fn push(&self, repo: &RepoName, commit: &CommitId, branch: &str) -> Result<(), RepoFailure> {
        let (token, url) = self.remote(repo)?;
        succeed(
            &Self::git(token.as_ref())
                .arg("--git-dir")
                .arg(self.dir(repo))
                .args([
                    "push",
                    "--quiet",
                    &url,
                    &format!("{}:refs/heads/{branch}", commit.as_str()),
                ])
                .timeout(REMOTE),
        )
        .map_err(|failed| RepoFailure::Unavailable(format!("the push was refused: {failed}")))?;
        self.fetched
            .lock()
            .map_err(|_| RepoFailure::Unavailable("the fetch table is poisoned".to_owned()))?
            .remove(repo);
        Ok(())
    }

    fn pull(
        &self,
        repo: &RepoName,
        branch: &str,
        commit: &CommitId,
        message: &str,
    ) -> Result<PullRequest, RepoFailure> {
        match self.open_pull_of(repo, branch, commit)? {
            Some(pull) => Ok(pull),
            None => self.open_pull(repo, branch, message),
        }
    }

    fn open_pull_of(
        &self,
        repo: &RepoName,
        branch: &str,
        commit: &CommitId,
    ) -> Result<Option<PullRequest>, RepoFailure> {
        match &self.source {
            Source::GitHub => {
                let token = self.token()?;
                let resp = self
                    .http
                    .send(
                        Req::get(format!(
                            "https://api.github.com/repos/{repo}/pulls?state=open&head={}:{branch}",
                            repo.owner()
                        ))
                        .header("Authorization", format!("Bearer {}", token.expose()))
                        .header("Accept", "application/vnd.github+json")
                        .header("User-Agent", "suzatary-secretary")
                        .timeout(Duration::from_secs(20)),
                    )
                    .map_err(|e| RepoFailure::Unavailable(format!("GitHub: {e}")))?;
                if resp.status != 200 {
                    return Err(RepoFailure::Unavailable(format!(
                        "GitHub answered {} to the pull request listing",
                        resp.status
                    )));
                }
                let answer: serde_json::Value = serde_json::from_slice(&resp.body)
                    .map_err(|e| RepoFailure::Unavailable(format!("GitHub: {e}")))?;
                answer
                    .as_array()
                    .and_then(|pulls| pulls.first())
                    .map(pull_of)
                    .transpose()
            }
            Source::Local { .. } => {
                let (_, url) = self.remote(repo)?;
                let refs = Self::local(
                    Path::new(&url),
                    &[
                        "for-each-ref",
                        "--format=%(objectname) %(refname)",
                        "refs/pull/",
                    ],
                )?;
                Ok(lines(&refs).iter().find_map(|line| {
                    let (id, name) = line.split_once(' ')?;
                    let number = name.strip_prefix("refs/pull/")?.strip_suffix("/head")?;
                    if id != commit.as_str() {
                        return None;
                    }
                    Some(PullRequest {
                        number: number.parse().ok()?,
                        url: format!("{url}#pull/{number}"),
                    })
                }))
            }
        }
    }

    fn open_pull(
        &self,
        repo: &RepoName,
        branch: &str,
        message: &str,
    ) -> Result<PullRequest, RepoFailure> {
        let into = self.row(repo)?.default_branch;
        let (title, body) = message.split_once('\n').unwrap_or((message, ""));
        match &self.source {
            Source::GitHub => {
                let token = self.token()?;
                let resp = self
                    .http
                    .send(
                        Req::json(
                            format!("https://api.github.com/repos/{repo}/pulls"),
                            &serde_json::json!({
                                "title": title.trim(),
                                "head": branch,
                                "base": into,
                                "body": body.trim(),
                            }),
                        )
                        .header("Authorization", format!("Bearer {}", token.expose()))
                        .header("Accept", "application/vnd.github+json")
                        .header("User-Agent", "suzatary-secretary")
                        .timeout(Duration::from_secs(20)),
                    )
                    .map_err(|e| RepoFailure::Unavailable(format!("GitHub: {e}")))?;
                if resp.status != 201 {
                    return Err(RepoFailure::Unavailable(format!(
                        "GitHub answered {} to the pull request",
                        resp.status
                    )));
                }
                let answer: serde_json::Value = serde_json::from_slice(&resp.body)
                    .map_err(|e| RepoFailure::Unavailable(format!("GitHub: {e}")))?;
                pull_of(&answer)
            }
            Source::Local { .. } => {
                let (_, url) = self.remote(repo)?;
                let remote = Path::new(&url);
                let opened = lines(&Self::local(
                    remote,
                    &["for-each-ref", "--format=%(refname)", "refs/pull/"],
                )?)
                .len();
                let number = opened as u64 + 1;
                Self::local(
                    remote,
                    &[
                        "update-ref",
                        &format!("refs/pull/{number}/head"),
                        &format!("refs/heads/{branch}"),
                    ],
                )?;
                Ok(PullRequest {
                    number,
                    url: format!("{url}#pull/{number}"),
                })
            }
        }
    }

    fn checked_tree(&self, dir: &Path, tree: &str, check: &str) -> Result<(), Stopped> {
        let unpack = || -> Result<Scratch, RepoFailure> {
            let scratch = self.scratch()?;
            let archive = Self::local(dir, &["archive", "--format=tar", tree])?;
            succeed(
                &Cmd::new("tar")
                    .arg("-x")
                    .arg("-C")
                    .arg(scratch.path())
                    .stdin(archive.stdout),
            )?;
            Ok(scratch)
        };
        let scratch = unpack().map_err(Stopped::from)?;
        // Both streams to stderr, so a failure quotes the check whole. This runs agent-proposed
        // code and is NOT a sandbox: HOME and XDG_RUNTIME_DIR still lead to the user's files,
        // gpg-agent and the secret store.
        let tail = match succeed(
            &Cmd::new("sh")
                .args(["-c", &format!("exec 1>&2\n{check}")])
                .cwd(scratch.path())
                .timeout(CHECK_TIMEOUT),
        ) {
            Ok(_) => return Ok(()),
            Err(RunError::Exited { stderr, .. }) => stderr.as_str().to_owned(),
            Err(RunError::TimedOut { after, .. }) => format!("timed out after {after:?}"),
            Err(failed) => {
                return Err(Stopped::Failed(format!(
                    "the check could not run: {failed}"
                )));
            }
        };
        Err(Stopped::Failed(format!(
            "the check `{check}` failed:\n{tail}"
        )))
    }

    /// Keeps an unpacked tree on the mirrors' filesystem.
    fn scratch(&self) -> Result<Scratch, RepoFailure> {
        Scratch::under(&self.root, ".scratch-").map_err(|e| RepoFailure::Unavailable(e.to_string()))
    }

    fn remote(&self, repo: &RepoName) -> Result<(Option<Secret>, String), RepoFailure> {
        Ok(match &self.source {
            Source::GitHub => (
                Some(self.token()?),
                format!("https://github.com/{repo}.git"),
            ),
            Source::Local { base, .. } => (
                None,
                base.join(repo.owner())
                    .join(format!("{}.git", repo.name()))
                    .display()
                    .to_string(),
            ),
        })
    }
}

struct Index(Scratch);

impl Index {
    fn path(&self) -> PathBuf {
        self.0.path().join("index")
    }
}

fn lines(ran: &Ran) -> Vec<String> {
    ran.out()
        .lines()
        .take(MAX_LINES)
        .map(str::to_owned)
        .collect()
}

impl From<RunError> for RepoFailure {
    fn from(failed: RunError) -> Self {
        RepoFailure::Unavailable(failed.to_string())
    }
}

/// A non-zero exit is Missing; failing to run is Unavailable.
fn refused_as_missing(what: impl FnOnce() -> String) -> impl FnOnce(RunError) -> RepoFailure {
    move |failed| match failed {
        RunError::Exited { .. } => RepoFailure::Missing(what()),
        failed => RepoFailure::from(failed),
    }
}

fn pull_of(answer: &serde_json::Value) -> Result<PullRequest, RepoFailure> {
    match (
        answer.get("number").and_then(serde_json::Value::as_u64),
        answer.get("html_url").and_then(serde_json::Value::as_str),
    ) {
        (Some(number), Some(url)) => Ok(PullRequest {
            number,
            url: url.to_owned(),
        }),
        _ => Err(RepoFailure::Unavailable(
            "GitHub's pull request names no number and page".to_owned(),
        )),
    }
}

fn row_of(value: &serde_json::Value) -> Option<RepoRow> {
    let repo: RepoName = value.get("full_name")?.as_str()?.parse().ok()?;
    let private = value.get("private")?.as_bool()?;
    Some(RepoRow {
        repo,
        visibility: if private {
            Visibility::Private
        } else {
            Visibility::Public
        },
        default_branch: value
            .get("default_branch")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("main")
            .to_owned(),
        description: value
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

impl Repositories for Mirrors {
    fn repos(&self) -> Result<Vec<RepoRow>, RepoFailure> {
        let poisoned = || RepoFailure::Unavailable("the listing lock is poisoned".to_owned());
        if let Some((at, rows)) = self.listed.lock().map_err(|_| poisoned())?.as_ref()
            && at.elapsed() < FRESH
        {
            return Ok(rows.clone());
        }
        if let Source::Local { base } = &self.source {
            return Ok(self
                .declared
                .iter()
                .map(|(repo, _)| repo)
                .filter(|repo| {
                    base.join(repo.owner())
                        .join(format!("{}.git", repo.name()))
                        .is_dir()
                })
                .map(|repo| RepoRow {
                    repo: repo.clone(),
                    visibility: Visibility::Public,
                    default_branch: "main".to_owned(),
                    description: String::new(),
                })
                .collect());
        }
        let token = self.token()?;
        let mut rows = Vec::new();
        for page in 1..=10 {
            let resp = self
                .http
                .send(
                    Req::get(format!(
                        "https://api.github.com/user/repos?per_page=100&affiliation=owner&page={page}"
                    ))
                    .header("Authorization", format!("Bearer {}", token.expose()))
                    .header("Accept", "application/vnd.github+json")
                    .header("User-Agent", "suzatary-secretary")
                    .timeout(Duration::from_secs(20)),
                )
                .map_err(|e| RepoFailure::Unavailable(format!("GitHub: {e}")))?;
            if resp.status != 200 {
                return Err(RepoFailure::Unavailable(format!(
                    "GitHub answered {} to the repository listing",
                    resp.status
                )));
            }
            let body: serde_json::Value = serde_json::from_slice(&resp.body)
                .map_err(|e| RepoFailure::Unavailable(format!("GitHub: {e}")))?;
            let items = body.as_array().map_or(&[][..], Vec::as_slice);
            let last = items.len() < 100;
            rows.extend(
                items
                    .iter()
                    .filter_map(row_of)
                    .filter(|row| self.declared.target(&row.repo).is_some()),
            );
            if last {
                break;
            }
        }
        rows.sort_by(|a, b| a.repo.cmp(&b.repo));
        *self.listed.lock().map_err(|_| poisoned())? = Some((Instant::now(), rows.clone()));
        Ok(rows)
    }

    fn resolve(&self, repo: &RepoName, at: Option<&Rev>) -> Result<CommitId, RepoFailure> {
        let name = match at {
            Some(rev) => rev.as_str().to_owned(),
            None => format!("refs/heads/{}", self.row(repo)?.default_branch),
        };
        let dir = self.mirror(repo)?;
        let output = Self::local(
            &dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &format!("{name}^{{commit}}"),
            ],
        )
        .map_err(refused_as_missing(|| format!("{name} in {repo}")))?;
        let commit = CommitId::try_from(output.out().trim().to_owned())
            .map_err(|e| RepoFailure::Unavailable(e.to_string()))?;
        // Only the default branch resolves, so nothing read is a fork's commit.
        let default = format!("refs/heads/{}", self.row(repo)?.default_branch);
        let on_default = Self::asked(
            &dir,
            &["merge-base", "--is-ancestor", commit.as_str(), &default],
        )
        .map_err(|e| RepoFailure::Unavailable(e.to_string()))?
        .ok();
        if on_default {
            Ok(commit)
        } else {
            Err(RepoFailure::Missing(format!(
                "{name} in {repo} is not on its default branch"
            )))
        }
    }

    fn tree(
        &self,
        repo: &RepoName,
        at: &CommitId,
        dir: &RepoPath,
    ) -> Result<Vec<String>, RepoFailure> {
        let mirror = self.mirror(repo)?;
        let mut args = vec!["ls-tree", at.as_str()];
        let under = format!("{}/", dir.as_str());
        if !dir.as_str().is_empty() {
            args.push("--");
            args.push(&under);
        }
        let output = Self::local(&mirror, &args).map_err(refused_as_missing(|| {
            format!("{} at {}", dir.as_str(), at.short())
        }))?;
        Ok(lines(&output)
            .into_iter()
            .filter_map(|line| {
                let (meta, path) = line.split_once('\t')?;
                Some(if meta.contains(" tree ") {
                    format!("{path}/")
                } else {
                    path.to_owned()
                })
            })
            .collect())
    }

    fn blob(
        &self,
        repo: &RepoName,
        at: &CommitId,
        path: &RepoPath,
    ) -> Result<Vec<u8>, RepoFailure> {
        let mirror = self.mirror(repo)?;
        let spec = format!("{}:{}", at.as_str(), path.as_str());
        let what = || format!("{} at {}", path.as_str(), at.short());
        let size: u64 = Self::local(&mirror, &["cat-file", "-s", &spec])
            .map_err(refused_as_missing(what))?
            .out()
            .trim()
            .parse()
            .map_err(|_| RepoFailure::Missing(what()))?;
        if size > MAX_FILE_BYTES {
            return Err(RepoFailure::TooLarge {
                path: path.as_str().to_owned(),
                size,
            });
        }
        Ok(Self::local(&mirror, &["cat-file", "blob", &spec])
            .map_err(refused_as_missing(what))?
            .stdout)
    }

    fn grep(
        &self,
        repo: &RepoName,
        at: &CommitId,
        needle: &str,
    ) -> Result<Vec<String>, RepoFailure> {
        let mirror = self.mirror(repo)?;
        let output = Self::asked(
            &mirror,
            &[
                "grep",
                "-n",
                "-I",
                "-F",
                "-i",
                "-e",
                needle,
                at.as_str(),
                "--",
            ],
        )
        .map_err(|e| RepoFailure::Unavailable(e.to_string()))?;
        // Exit 1 is no match.
        if !output.ok() && output.status.code() != Some(1) {
            return Err(RepoFailure::Unavailable(output.err().trim().to_owned()));
        }
        let prefix = format!("{}:", at.as_str());
        Ok(lines(&output)
            .into_iter()
            .map(|line| {
                line.strip_prefix(&prefix)
                    .map_or(line.clone(), str::to_owned)
            })
            .collect())
    }

    fn applies(&self, repo: &RepoName, base: &CommitId, patch: &Patch) -> Result<(), RepoFailure> {
        let mirror = self.mirror(repo)?;
        self.staged(&mirror, base, patch, Apply::Exact).map(|_| ())
    }

    fn merged(&self, repo: &RepoName, patch: &Patch) -> Result<Option<InBranch>, RepoFailure> {
        let tip = self.resolve(repo, None)?;
        let branch = self.row(repo)?.default_branch;
        let dir = self.mirror(repo)?;
        let undone = match self.staged(&dir, &tip, patch, Apply::Reverse) {
            Ok(_) => true,
            Err(RepoFailure::Conflict(_)) => false,
            Err(failure) => return Err(failure),
        };
        let holds = undone
            || match self.staged(&dir, &tip, patch, Apply::ThreeWay) {
                Ok(index) => {
                    let tree =
                        Self::local(&dir, &["rev-parse", &format!("{}^{{tree}}", tip.as_str())])?;
                    Self::tree_of(&dir, &index)? == tree.out().trim()
                }
                Err(RepoFailure::Conflict(_)) => false,
                Err(failure) => return Err(failure),
            };
        Ok(holds.then_some(InBranch { branch, tip }))
    }

    fn pushable(&self, repo: &RepoName, visibility: Visibility) -> Result<(), RepoFailure> {
        self.declared.pushable(repo, visibility)
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    /// A local source needs no token: asking for one fails the test.
    struct NoToken;

    impl Resolves for NoToken {
        fn get(
            &self,
            name: &str,
            _reason: &str,
        ) -> Result<Option<Secret>, crate::secrets::SecretError> {
            panic!("a local source asked for {name}");
        }
    }

    fn run(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            status.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    fn declared(repo: &RepoName) -> Declared {
        checked_by(repo, None)
    }

    fn checked_by(repo: &RepoName, check: Option<&str>) -> Declared {
        Declared::new([(
            repo.clone(),
            RepoTarget {
                check: check.map(str::to_owned),
                // A local remote lists as public.
                public: true,
            },
        )])
        .expect("one repository")
    }

    fn fixture(tag: &str) -> (tempfile::TempDir, Mirrors, RepoName) {
        checked_fixture(tag, None)
    }

    fn checked_fixture(tag: &str, check: Option<&str>) -> (tempfile::TempDir, Mirrors, RepoName) {
        let temp = tempfile::tempdir().expect("a temp dir");
        let work = temp.path().join("work");
        std::fs::create_dir_all(work.join("docs")).expect("work dir");
        std::fs::write(work.join("cv.typ"), "= Ada\nResearch engineer\n").expect("cv");
        std::fs::write(work.join("docs/notes.md"), "notes\n").expect("notes");
        run(&work, &["init", "-q", "-b", "main"]);
        run(&work, &["add", "."]);
        run(&work, &["commit", "-q", "-m", "first"]);
        let base = temp.path().join("remotes");
        std::fs::create_dir_all(base.join("bmabsout")).expect("remotes");
        run(
            temp.path(),
            &[
                "clone",
                "-q",
                "--bare",
                "work",
                &format!("remotes/bmabsout/{tag}.git"),
            ],
        );
        let repo: RepoName = format!("bmabsout/{tag}").parse().expect("a name");
        let mirrors = Mirrors::new(
            temp.path().join("mirrors"),
            Source::Local { base },
            checked_by(&repo, check),
            Box::new(NoToken),
        );
        (temp, mirrors, repo)
    }

    #[test]
    fn only_the_default_branchs_history_resolves() {
        let (temp, mirrors, repo) = fixture("forks");
        let work = temp.path().join("work");
        let remote = temp.path().join("remotes/bmabsout/forks.git");
        let remote = remote.to_str().expect("a utf-8 path");
        run(&work, &["checkout", "-q", "-b", "outsider"]);
        std::fs::write(work.join("cv.typ"), "= Mallory\n").expect("a fork's cv");
        run(&work, &["commit", "-qam", "a fork's change"]);
        run(&work, &["push", "-q", remote, "outsider:refs/pull/1/head"]);
        run(
            &work,
            &["push", "-q", remote, "outsider:refs/heads/unmerged"],
        );
        let fork = String::from_utf8(
            Command::new("git")
                .current_dir(&work)
                .args(["rev-parse", "HEAD"])
                .output()
                .expect("git runs")
                .stdout,
        )
        .expect("utf-8")
        .trim()
        .to_owned();
        for name in ["pull/1/head", "refs/pull/1/head", "unmerged", fork.as_str()] {
            let rev = Rev::try_from(name.to_owned()).expect("a revision");
            assert!(
                matches!(
                    mirrors.resolve(&repo, Some(&rev)),
                    Err(RepoFailure::Missing(_))
                ),
                "{name}"
            );
        }
        let mirror = temp.path().join("mirrors/bmabsout/forks.git");
        let refs = Command::new("git")
            .arg("--git-dir")
            .arg(&mirror)
            .args(["for-each-ref", "--format=%(refname)"])
            .output()
            .expect("git runs");
        let refs = String::from_utf8_lossy(&refs.stdout);
        assert!(
            !refs.contains("refs/pull/"),
            "a pull request's ref is never fetched: {refs}"
        );
        assert!(mirrors.resolve(&repo, None).is_ok());
    }

    #[test]
    fn a_patch_is_checked_against_its_base_and_touches_nothing() {
        let (_temp, mirrors, repo) = fixture("cv2");
        let head = mirrors.resolve(&repo, None).expect("a head");
        let good = Patch::try_from(
            "--- a/cv.typ\n+++ b/cv.typ\n@@ -1,2 +1,2 @@\n = Ada\n-Research engineer\n+Principal investigator\n"
                .to_owned(),
        )
        .expect("a patch");
        mirrors.applies(&repo, &head, &good).expect("applies");
        assert_eq!(
            mirrors.resolve(&repo, None).expect("head"),
            head,
            "checking moved nothing"
        );
        let stale = Patch::try_from(
            "--- a/cv.typ\n+++ b/cv.typ\n@@ -1,2 +1,2 @@\n = Ada\n-Not what it says\n+x\n"
                .to_owned(),
        )
        .expect("a patch");
        assert!(matches!(
            mirrors.applies(&repo, &head, &stale),
            Err(RepoFailure::Conflict(_))
        ));
    }

    #[test]
    fn a_claude_branch_is_seen_with_its_report_and_then_merged() {
        let (temp, mirrors, repo) = fixture("coding");
        let work = temp.path().join("work");
        let remote = temp.path().join("remotes/bmabsout/coding.git");
        let remote = remote.to_str().expect("a utf-8 path");
        run(&work, &["checkout", "-q", "-b", "claude/retitle"]);
        std::fs::create_dir_all(work.join("docs/reports")).expect("reports dir");
        std::fs::write(
            work.join("docs/reports/retitle.md"),
            "Session: https://claude.ai/code/session_01fake\nGates: green\n",
        )
        .expect("report");
        std::fs::write(work.join("cv.typ"), "= Ada\nPrincipal investigator\n").expect("cv");
        run(&work, &["add", "."]);
        run(&work, &["commit", "-q", "-m", "retitle"]);
        run(&work, &["push", "-q", remote, "claude/retitle"]);
        run(
            &work,
            &["push", "-q", remote, "main:refs/heads/not-a-session"],
        );

        let seen = mirrors.seen(&repo).expect("seen");
        assert_eq!(seen.len(), 1, "only claude/ branches: {seen:?}");
        let branch = &seen[0];
        assert_eq!(branch.branch.as_str(), "claude/retitle");
        assert!(!branch.merged);
        assert_eq!(branch.reports.len(), 1);
        assert_eq!(branch.reports[0].path().as_str(), "docs/reports/retitle.md");
        assert!(branch.reports[0].text().contains("session_01fake"));
        let files: Vec<(&str, Lines)> = branch
            .stat
            .iter()
            .map(|file| (file.path.as_str(), file.lines))
            .collect();
        assert_eq!(
            files,
            [
                (
                    "cv.typ",
                    Lines::Text {
                        added: 1,
                        removed: 1
                    }
                ),
                (
                    "docs/reports/retitle.md",
                    Lines::Text {
                        added: 2,
                        removed: 0
                    }
                ),
            ]
        );

        run(&work, &["checkout", "-q", "main"]);
        run(&work, &["merge", "-q", "--ff-only", "claude/retitle"]);
        run(&work, &["push", "-q", remote, "main"]);
        let refetched = Mirrors::new(
            temp.path().join("mirrors"),
            Source::Local {
                base: temp.path().join("remotes"),
            },
            declared(&repo),
            Box::new(NoToken),
        );
        let seen = refetched.seen(&repo).expect("seen again");
        assert!(seen[0].merged, "the tip is on main now");
    }

    #[test]
    fn a_branch_merged_before_the_first_poll_is_reported_then_merged() {
        use suzatary_core::coding::{Session, Step, next};
        let (temp, mirrors, repo) = fixture("merged-first");
        let work = temp.path().join("work");
        let remote = temp.path().join("remotes/bmabsout/merged-first.git");
        let remote = remote.to_str().expect("a utf-8 path");
        run(&work, &["checkout", "-q", "-b", "claude/retitle"]);
        std::fs::create_dir_all(work.join("docs/reports")).expect("reports dir");
        std::fs::write(
            work.join("docs/reports/retitle.md"),
            "Session: https://claude.ai/code/session_01fake\nGates: green\n",
        )
        .expect("report");
        run(&work, &["add", "."]);
        run(&work, &["commit", "-q", "-m", "report"]);
        std::fs::write(work.join("cv.typ"), "= Ada\nPrincipal investigator\n").expect("cv");
        run(&work, &["commit", "-qam", "retitle"]);
        run(&work, &["checkout", "-q", "main"]);
        std::fs::write(work.join("docs/notes.md"), "notes, later\n").expect("notes");
        run(&work, &["commit", "-qam", "meanwhile"]);
        run(
            &work,
            &["merge", "-q", "--no-ff", "-m", "merge", "claude/retitle"],
        );
        std::fs::write(work.join("docs/notes.md"), "notes, after\n").expect("notes");
        run(&work, &["commit", "-qam", "after"]);
        run(&work, &["push", "-q", remote, "main", "claude/retitle"]);

        let seen = mirrors.seen(&repo).expect("seen");
        assert_eq!(seen.len(), 1, "{seen:?}");
        let branch = &seen[0];
        assert!(branch.merged, "the tip is on main");
        assert_eq!(branch.reports.len(), 1, "{branch:?}");
        let files: Vec<&str> = branch.stat.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            files,
            ["cv.typ", "docs/reports/retitle.md"],
            "its own diff alone"
        );

        let fired = Session::Fired {
            id: "session_01fake".parse().expect("an id"),
            url: "https://claude.ai/code/session_01fake"
                .parse()
                .expect("a url"),
            at: "2026-11-04T09:00:00Z".to_owned(),
        };
        let Some(step @ Step::Report { .. }) = next(&fired, &seen) else {
            panic!("the first poll reports it: {:?}", next(&fired, &seen));
        };
        let reported = fired.step(step).expect("a fired session reports");
        assert!(
            matches!(next(&reported, &seen), Some(Step::Merge { .. })),
            "the next poll reads it merged"
        );
    }

    #[test]
    fn a_repository_outside_the_declared_list_is_refused_before_git_runs() {
        let (temp, mirrors, _repo) = fixture("cv3");
        let present: RepoName = "bmabsout/present".parse().expect("a name");
        run(
            temp.path(),
            &[
                "clone",
                "-q",
                "--bare",
                "work",
                "remotes/bmabsout/present.git",
            ],
        );
        assert!(
            matches!(
                mirrors.resolve(&present, None),
                Err(RepoFailure::Unknown(_))
            ),
            "undeclared"
        );
        let other: RepoName = "someorg/secrets".parse().expect("a name");
        assert!(matches!(
            mirrors.resolve(&other, None),
            Err(RepoFailure::Unknown(_))
        ));
        assert!(!temp.path().join("mirrors/bmabsout/present.git").exists());
    }

    /// `NoToken` fails the test if the token is asked for.
    #[test]
    fn an_undeclared_repository_is_refused_before_any_network() {
        let temp = tempfile::tempdir().expect("a temp dir");
        let declared_one: RepoName = "bmabsout/cv".parse().expect("a name");
        let mirrors = Mirrors::new(
            temp.path().join("mirrors"),
            Source::GitHub,
            declared(&declared_one),
            Box::new(NoToken),
        );
        for name in ["bmabsout/other", "someorg/secrets"] {
            let repo: RepoName = name.parse().expect("a name");
            assert!(
                matches!(mirrors.resolve(&repo, None), Err(RepoFailure::Unknown(_))),
                "{name}"
            );
        }
    }

    fn author() -> Author {
        Author {
            name: "Ada".to_owned(),
            email: "b@example.test".to_owned(),
        }
    }

    fn retitle() -> Patch {
        Patch::try_from(
            "--- a/cv.typ\n+++ b/cv.typ\n@@ -1,2 +1,2 @@\n = Ada\n-Research engineer\n+Principal investigator\n"
                .to_owned(),
        )
        .expect("a patch")
    }

    fn on_remote(temp: &tempfile::TempDir, tag: &str, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("--git-dir")
            .arg(temp.path().join(format!("remotes/bmabsout/{tag}.git")))
            .args(args)
            .output()
            .expect("git runs");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[test]
    fn a_publish_opens_a_pull_request_and_leaves_the_default_branch() {
        let (temp, mirrors, repo) = checked_fixture("pub", Some("grep -q Principal cv.typ"));
        let base = mirrors.resolve(&repo, None).expect("a head");
        let published = mirrors
            .publish(
                &repo,
                &base,
                &retitle(),
                "Retitle the CV\n\nHe is a PI now.\n",
                &author(),
                "secretary/2026-10-05-1-0",
            )
            .expect("published");
        assert_eq!(published.branch, "secretary/2026-10-05-1-0");
        assert_eq!(published.pull.number, 1);
        assert!(
            on_remote(&temp, "pub", &["show", "secretary/2026-10-05-1-0:cv.typ"])
                .contains("Principal investigator")
        );
        assert_eq!(
            on_remote(&temp, "pub", &["rev-parse", "refs/pull/1/head"]),
            published.commit.as_str(),
            "the request names the pushed commit"
        );
        assert_eq!(
            mirrors.resolve(&repo, None).expect("head"),
            base,
            "the default branch is a person's to merge into"
        );
        let second = mirrors
            .publish(
                &repo,
                &base,
                &retitle(),
                "Again",
                &author(),
                "secretary/other",
            )
            .expect("a second");
        assert_eq!(second.pull.number, 2);
        let again = mirrors
            .publish(
                &repo,
                &base,
                &retitle(),
                "Retitle the CV\n\nHe is a PI now.\n",
                &author(),
                "secretary/2026-10-05-1-0",
            )
            .expect("published again");
        assert_eq!(
            again, published,
            "a publish again answers what it published"
        );
    }

    #[test]
    fn a_pushed_branch_without_a_pull_request_is_finished_by_a_retry() {
        let (temp, mirrors, repo) = fixture("nopull");
        // A stale lock on the request's ref: the branch pushes, the request cannot be made.
        let pulls = temp.path().join("remotes/bmabsout/nopull.git/refs/pull/1");
        std::fs::create_dir_all(&pulls).expect("the pull refs");
        std::fs::write(pulls.join("head.lock"), "").expect("a stale lock");
        let base = mirrors.resolve(&repo, None).expect("a head");
        let publish = || {
            mirrors.publish(
                &repo,
                &base,
                &retitle(),
                "Retitle",
                &author(),
                "secretary/x",
            )
        };
        let Err(Stopped::Pushed { commit, .. }) = publish() else {
            panic!("the branch is pushed and no request is opened");
        };
        assert_eq!(
            on_remote(&temp, "nopull", &["rev-parse", "secretary/x"]),
            commit.as_str()
        );
        std::fs::remove_file(pulls.join("head.lock")).expect("the lock goes");
        let published = publish().expect("finished");
        assert_eq!(
            published.commit, commit,
            "the commit pushed, not a second one"
        );
        assert_eq!(
            on_remote(&temp, "nopull", &["rev-parse", "refs/pull/1/head"]),
            commit.as_str()
        );
    }

    /// A change is in the default branch once applying it to the tip changes nothing, however
    /// many commits came after it; another change to the same lines is not that change.
    #[test]
    fn a_change_is_merged_once_the_default_branch_holds_it() {
        let (temp, mirrors, repo) = fixture("merged");
        assert_eq!(mirrors.merged(&repo, &retitle()).expect("read"), None);
        let work = temp.path().join("work");
        let remote = temp.path().join("remotes/bmabsout/merged.git");
        std::fs::write(work.join("cv.typ"), "= Ada\nPrincipal investigator\n").expect("cv");
        run(
            &work,
            &["commit", "-qam", "retitle, by another pull request"],
        );
        std::fs::write(work.join("docs/notes.md"), "notes, later\n").expect("notes");
        run(&work, &["commit", "-qam", "later"]);
        run(
            &work,
            &["push", "-q", remote.to_str().expect("a utf-8 path"), "main"],
        );
        let refetched = Mirrors::new(
            temp.path().join("mirrors"),
            Source::Local {
                base: temp.path().join("remotes"),
            },
            declared(&repo),
            Box::new(NoToken),
        );
        let held = refetched
            .merged(&repo, &retitle())
            .expect("read")
            .expect("merged");
        assert_eq!(held.branch, "main");
        assert_eq!(held.tip, refetched.resolve(&repo, None).expect("the tip"));
        let other = Patch::try_from(
            "--- a/cv.typ\n+++ b/cv.typ\n@@ -1,2 +1,2 @@\n = Ada\n-Research engineer\n+Professor\n"
                .to_owned(),
        )
        .expect("a patch");
        assert_eq!(refetched.merged(&repo, &other).expect("read"), None);
    }

    #[test]
    fn a_failing_check_pushes_nothing() {
        let (temp, mirrors, repo) = checked_fixture("chk", Some("echo broken; exit 3"));
        let base = mirrors.resolve(&repo, None).expect("a head");
        let refused = mirrors.publish(&repo, &base, &retitle(), "x", &author(), "secretary/x");
        assert!(
            matches!(&refused, Err(Stopped::Failed(said)) if said.contains("broken")),
            "{refused:?}"
        );
        assert_eq!(
            on_remote(&temp, "chk", &["for-each-ref"]).lines().count(),
            1,
            "only main"
        );
    }

    #[test]
    fn a_check_leaves_nothing_behind() {
        for (tag, check) in [("kept", "true"), ("gone", "exit 3")] {
            let (temp, mirrors, repo) = checked_fixture(tag, Some(check));
            let base = mirrors.resolve(&repo, None).expect("a head");
            let _ = mirrors.publish(&repo, &base, &retitle(), "x", &author(), "secretary/x");
            let left: Vec<String> = std::fs::read_dir(temp.path().join("mirrors"))
                .expect("the mirrors' root")
                .map(|entry| {
                    entry
                        .expect("an entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            assert_eq!(left, ["bmabsout"], "`{check}` left {left:?}");
        }
    }

    #[test]
    fn a_branch_that_holds_something_else_is_refused_not_forced() {
        let (temp, mirrors, repo) = fixture("moved");
        let base = mirrors.resolve(&repo, None).expect("a head");
        let first = mirrors
            .publish(
                &repo,
                &base,
                &retitle(),
                "first",
                &author(),
                "secretary/one",
            )
            .expect("first");
        let other = Patch::try_from(
            "--- a/docs/notes.md\n+++ b/docs/notes.md\n@@ -1 +1 @@\n-notes\n+more notes\n"
                .to_owned(),
        )
        .expect("a patch");
        let stale = mirrors.publish(&repo, &base, &other, "second", &author(), "secretary/one");
        assert!(matches!(stale, Err(Stopped::Refused(_))), "{stale:?}");
        assert_eq!(
            on_remote(&temp, "moved", &["rev-parse", "secretary/one"]),
            first.commit.as_str()
        );
    }
}
