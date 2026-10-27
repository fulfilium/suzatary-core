//! What the secretary may name in a repository.
//!
//! Every name comes from a model that reads attacker-controlled mail and ends up as a `git`
//! argument, so each constructor refuses anything `git` could read as an option or as a path
//! outside the repository.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::capability::RepoFailure;

/// Why a name was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RepoNameError {
    /// Not `owner/name`, or a part holds a character GitHub does not allow.
    #[error("a repository is owner/name, letters, digits, '-', '_' and '.', got {0:?}")]
    Repository(String),
    /// Absolute, empty in the middle, `..`, or too long.
    #[error("a path is relative, inside the repository and without '..', got {0:?}")]
    Path(String),
    /// Not a full commit id.
    #[error("a commit is forty lowercase hex digits, got {0:?}")]
    Commit(String),
    /// Not a branch, tag or commit name, or one `git` could read as an expression.
    #[error(
        "a revision is a branch, a tag or a commit: letters, digits, '-', '_', '.' and '/', \
         got {0:?}"
    )]
    Rev(String),
    /// Empty, too large, not a unified diff, or a binary change.
    #[error("{0}")]
    Patch(String),
    /// The declared list names repositories of more than one account.
    #[error(
        "every declared repository is in the owner's own account, but these name {}",
        .0.join(", ")
    )]
    Accounts(Vec<String>),
    /// The declared list names one repository twice.
    #[error("{0} is declared twice")]
    Twice(String),
}

fn segment_ok(part: &str) -> bool {
    !part.is_empty()
        && part.len() <= 100
        && !part.starts_with('.')
        && !part.starts_with('-')
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// `owner/name`, as GitHub spells one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoName {
    owner: String,
    name: String,
}

impl RepoName {
    /// The owner, the account the token belongs to.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The repository's own name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl TryFrom<String> for RepoName {
    type Error = RepoNameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<RepoName> for String {
    fn from(repo: RepoName) -> Self {
        repo.to_string()
    }
}

impl std::str::FromStr for RepoName {
    type Err = RepoNameError;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (owner, name) = text
            .split_once('/')
            .ok_or_else(|| RepoNameError::Repository(text.to_owned()))?;
        if !segment_ok(owner) || !segment_ok(name) || name.contains('/') {
            return Err(RepoNameError::Repository(text.to_owned()));
        }
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }
}

impl fmt::Display for RepoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// A path inside a repository, relative; `""` is the root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RepoPath(String);

impl RepoPath {
    /// The path as `git` reads it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RepoPath {
    type Error = RepoNameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        let trimmed = text.trim_end_matches('/');
        let refused = trimmed.len() > 400
            || trimmed.starts_with('/')
            || trimmed.starts_with('-')
            || trimmed.contains('\0')
            || trimmed.contains(':')
            || (!trimmed.is_empty()
                && trimmed
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == ".."));
        if refused {
            return Err(RepoNameError::Path(text));
        }
        Ok(Self(trimmed.to_owned()))
    }
}

impl From<RepoPath> for String {
    fn from(path: RepoPath) -> Self {
        path.0
    }
}

/// A full commit id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommitId(String);

impl CommitId {
    /// Forty hex digits.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The first twelve, for a card.
    #[must_use]
    pub fn short(&self) -> &str {
        &self.0[..12]
    }
}

impl TryFrom<String> for CommitId {
    type Error = RepoNameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.len() == 40 && text.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) {
            Ok(Self(text))
        } else {
            Err(RepoNameError::Commit(text))
        }
    }
}

impl From<CommitId> for String {
    fn from(commit: CommitId) -> Self {
        commit.0
    }
}

/// A revision as the secretary names one: a branch, a tag or a commit id. Nothing `git` reads
/// as an expression (`^`, `~`, `:`, `@{`, `..`) or as an option can be one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Rev(String);

impl Rev {
    /// The name as `git rev-parse` reads it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Rev {
    type Error = RepoNameError;
    fn try_from(text: String) -> Result<Self, Self::Error> {
        let ok = !text.is_empty()
            && text.len() <= 200
            && text.split('/').all(|part| {
                !part.is_empty()
                    && !part.starts_with(['.', '-'])
                    && !part.contains("..")
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            });
        if ok {
            Ok(Self(text))
        } else {
            Err(RepoNameError::Rev(text))
        }
    }
}

impl From<Rev> for String {
    fn from(rev: Rev) -> Self {
        rev.0
    }
}

impl From<CommitId> for Rev {
    fn from(commit: CommitId) -> Self {
        Self(commit.0)
    }
}

/// The most a patch may be: what a person reads on a phone before tapping.
pub const MAX_PATCH_BYTES: usize = 64 * 1024;

/// A unified diff of text files, bounded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Patch(String);

impl Patch {
    /// The diff, exactly as the card shows it and `git apply` reads it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Every file the diff touches, from its `+++`/`---` lines, in order and
    /// once each.
    #[must_use]
    pub fn files(&self) -> Vec<String> {
        let mut files: Vec<String> = Vec::new();
        for line in self.0.lines() {
            let named = line
                .strip_prefix("+++ b/")
                .or_else(|| line.strip_prefix("--- a/"));
            if let Some(name) = named {
                let name = name.trim_end().to_owned();
                if !files.contains(&name) {
                    files.push(name);
                }
            }
        }
        files
    }
}

impl TryFrom<String> for Patch {
    type Error = RepoNameError;
    fn try_from(mut text: String) -> Result<Self, Self::Error> {
        if text.trim().is_empty() {
            return Err(RepoNameError::Patch("the patch is empty".to_owned()));
        }
        if text.len() > MAX_PATCH_BYTES {
            return Err(RepoNameError::Patch(format!(
                "the patch is {} bytes; at most {MAX_PATCH_BYTES} is one a person can read before tapping",
                text.len()
            )));
        }
        if !text.lines().any(|line| line.starts_with("+++ ")) {
            return Err(RepoNameError::Patch(
                "a patch is a unified diff, with ---/+++ file lines and @@ hunks".to_owned(),
            ));
        }
        if text.contains("GIT binary patch") || text.lines().any(|l| l.starts_with("Binary files "))
        {
            return Err(RepoNameError::Patch(
                "a patch changes text files only; a binary file cannot be shown on a card"
                    .to_owned(),
            ));
        }
        if !text.ends_with('\n') {
            text.push('\n');
        }
        Ok(Self(text))
    }
}

impl From<Patch> for String {
    fn from(patch: Patch) -> Self {
        patch.0
    }
}

/// The token the secretary reaches `[[secretary.repos]]` with: their reads, a tapped patch's
/// push and its pull request. Scoped to those repositories alone, never the data repository's.
pub const TOKEN: &str = "SUZATARY_REPOS_TOKEN";

/// Who can read a repository once something is pushed to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Anyone.
    Public,
    /// Only who the owner has let in.
    Private,
}

impl Visibility {
    /// Every visibility.
    pub const ALL: [Visibility; 2] = [Visibility::Public, Visibility::Private];

    /// Its word on the wire and in an inbox act.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Private => "private",
        }
    }

    /// What a card says about pushing here.
    #[must_use]
    pub fn sentence(self) -> &'static str {
        match self {
            Visibility::Public => "public: anyone can read this once it is pushed",
            Visibility::Private => "private",
        }
    }
}

/// Its word on the wire.
impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One repository the token can reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoRow {
    /// `owner/name`.
    pub repo: RepoName,
    /// Who can read it.
    pub visibility: Visibility,
    /// The branch a push lands on unless the instance says otherwise.
    pub default_branch: String,
    /// GitHub's one line about it, "" when it has none.
    pub description: String,
}

/// What the instance file says about one repository.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RepoTarget {
    /// A shell command run in the patched tree before anything is committed;
    /// a nonzero exit refuses the push with its output. `None` runs nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// That a patch may be pushed here though the repository is public, said in so many words:
    /// a pushed branch is readable by anyone, and what the secretary writes into one it wrote
    /// after reading mail. Absent, a public repository takes no patch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub public: bool,
}

/// The repositories this deployment lets the secretary reach: a closed list from the instance
/// file, all in one account. Anything else is refused by name before `git` or GitHub is asked.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Declared(BTreeMap<RepoName, RepoTarget>);

impl Declared {
    /// The list, each repository once and all of one owner.
    ///
    /// # Errors
    /// [`RepoNameError::Twice`] or [`RepoNameError::Accounts`].
    pub fn new(
        entries: impl IntoIterator<Item = (RepoName, RepoTarget)>,
    ) -> Result<Self, RepoNameError> {
        let mut declared = BTreeMap::new();
        for (repo, target) in entries {
            if declared.contains_key(&repo) {
                return Err(RepoNameError::Twice(repo.to_string()));
            }
            declared.insert(repo, target);
        }
        Self::one_account(declared)
    }

    /// The list with `repo` reachable too, with no check unless it already had one.
    ///
    /// # Errors
    /// [`RepoNameError::Accounts`] when `repo` is another account's.
    pub fn with(self, repo: RepoName) -> Result<Self, RepoNameError> {
        let mut declared = self.0;
        declared.entry(repo).or_default();
        Self::one_account(declared)
    }

    fn one_account(declared: BTreeMap<RepoName, RepoTarget>) -> Result<Self, RepoNameError> {
        let owners: BTreeSet<&str> = declared.keys().map(RepoName::owner).collect();
        if owners.len() > 1 {
            return Err(RepoNameError::Accounts(
                owners.into_iter().map(str::to_owned).collect(),
            ));
        }
        Ok(Self(declared))
    }

    /// What the instance file says about `repo`, or `None`: not declared, so not reachable.
    #[must_use]
    pub fn target(&self, repo: &RepoName) -> Option<&RepoTarget> {
        self.0.get(repo)
    }

    /// Whether a patch may be pushed to `repo`, read as `visibility`: a private one that is
    /// declared, or a public one whose entry says `public = true`.
    ///
    /// # Errors
    /// [`RepoFailure::Unknown`] for one not declared, [`RepoFailure::Public`] for a public one
    /// whose entry does not say it may be pushed to.
    pub fn pushable(&self, repo: &RepoName, visibility: Visibility) -> Result<(), RepoFailure> {
        let target = self
            .target(repo)
            .ok_or_else(|| RepoFailure::Unknown(repo.to_string()))?;
        match visibility {
            Visibility::Public if !target.public => Err(RepoFailure::Public(repo.to_string())),
            Visibility::Public | Visibility::Private => Ok(()),
        }
    }

    /// Every declared repository with its target, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&RepoName, &RepoTarget)> {
        self.0.iter()
    }
}

// The literal of each is its text, as an inbox act stores it.
prodrome_core::text_literal!(
    RepoName => |repo| String::from(repo.clone()),
    CommitId => |commit| String::from(commit.clone()),
    Patch => |patch| String::from(patch.clone()),
    RepoPath => |path| String::from(path.clone()),
    Rev => |rev| String::from(rev.clone()),
);
prodrome_core::word_literal!(Visibility: Visibility::ALL => |visibility| visibility.as_str());

#[cfg(test)]
mod tests {
    use super::*;

    /// `rename_all` spells the generated `Visibility`, so serde's words are checked against
    /// `ALL`'s.
    #[test]
    fn a_visibilitys_serde_word_is_its_literals() {
        crate::words::assert_serde_words(&Visibility::ALL, |visibility| visibility.as_str());
    }

    fn named(text: &str) -> RepoName {
        text.parse().expect("a name")
    }

    /// A private repository a deployment declares takes a patch; a public one only where its
    /// entry says `public = true`; an undeclared one never.
    #[test]
    fn a_public_repository_is_pushable_only_where_its_entry_says_so() {
        let flagged = RepoTarget {
            public: true,
            ..RepoTarget::default()
        };
        let declared = Declared::new([
            (named("ada/notes"), RepoTarget::default()),
            (named("ada/site"), flagged),
        ])
        .expect("a list");
        assert_eq!(
            declared.pushable(&named("ada/notes"), Visibility::Private),
            Ok(())
        );
        assert_eq!(
            declared.pushable(&named("ada/notes"), Visibility::Public),
            Err(RepoFailure::Public("ada/notes".to_owned()))
        );
        assert_eq!(
            declared.pushable(&named("ada/site"), Visibility::Public),
            Ok(())
        );
        assert_eq!(
            declared.pushable(&named("ada/other"), Visibility::Private),
            Err(RepoFailure::Unknown("ada/other".to_owned()))
        );
    }

    #[test]
    fn the_declared_list_is_closed_and_of_one_account() {
        let declared = Declared::new([
            (named("bmabsout/cv"), RepoTarget::default()),
            (named("bmabsout/site"), RepoTarget::default()),
        ])
        .expect("one account");
        assert!(declared.target(&named("bmabsout/cv")).is_some());
        assert!(declared.target(&named("bmabsout/other")).is_none());
        assert!(matches!(
            declared.clone().with(named("someorg/secrets")),
            Err(RepoNameError::Accounts(owners)) if owners == ["bmabsout", "someorg"]
        ));
        assert_eq!(
            declared
                .clone()
                .with(named("bmabsout/cv"))
                .expect("idempotent"),
            declared
        );
        assert!(matches!(
            Declared::new([
                (named("bmabsout/cv"), RepoTarget::default()),
                (named("bmabsout/cv"), RepoTarget::default()),
            ]),
            Err(RepoNameError::Twice(_))
        ));
    }

    #[test]
    fn a_repository_is_owner_and_name_and_nothing_git_reads_as_an_option() {
        let repo: RepoName = "bmabsout/resume_typst".parse().expect("parses");
        assert_eq!(repo.owner(), "bmabsout");
        assert_eq!(repo.to_string(), "bmabsout/resume_typst");
        for bad in [
            "bmabsout", "/x", "a/b/c", "-x/y", "a/..", "a/.git", "a b/c", "a/",
        ] {
            assert!(bad.parse::<RepoName>().is_err(), "{bad}");
        }
    }

    #[test]
    fn a_path_stays_inside_the_repository() {
        assert_eq!(
            RepoPath::try_from("cv.typ".to_owned())
                .expect("ok")
                .as_str(),
            "cv.typ"
        );
        assert_eq!(
            RepoPath::try_from(String::new()).expect("root").as_str(),
            ""
        );
        assert_eq!(
            RepoPath::try_from("docs/".to_owned())
                .expect("dir")
                .as_str(),
            "docs"
        );
        for bad in [
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "-n",
            "HEAD:x",
            "./a",
        ] {
            assert!(RepoPath::try_from(bad.to_owned()).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_revision_is_a_name_never_an_expression() {
        for good in ["main", "v1.2", "feature/cv-2026", &"a".repeat(40)] {
            assert!(Rev::try_from(good.to_owned()).is_ok(), "{good}");
        }
        for bad in [
            "", "-n", "main^", "main~1", "HEAD@{1}", "a..b", "a:b", "/main", "main/", "a//b",
            ".hidden", "a b",
        ] {
            assert!(Rev::try_from(bad.to_owned()).is_err(), "{bad}");
        }
        let commit = CommitId::try_from("b".repeat(40)).expect("a commit");
        assert_eq!(Rev::from(commit.clone()).as_str(), commit.as_str());
    }

    #[test]
    fn a_commit_is_a_full_id() {
        assert!(CommitId::try_from("a".repeat(40)).is_ok());
        assert!(CommitId::try_from("HEAD".to_owned()).is_err());
        assert!(CommitId::try_from("A".repeat(40)).is_err());
    }

    #[test]
    fn a_patch_is_bounded_text_and_names_its_files() {
        let diff = "--- a/cv.typ\n+++ b/cv.typ\n@@ -1 +1 @@\n-old\n+new";
        let patch = Patch::try_from(diff.to_owned()).expect("a diff");
        assert!(patch.as_str().ends_with('\n'));
        assert_eq!(patch.files(), vec!["cv.typ"]);
        assert!(Patch::try_from("just words".to_owned()).is_err());
        assert!(Patch::try_from(format!("{diff}\nGIT binary patch\n")).is_err());
        assert!(Patch::try_from(format!("{diff}\n{}", "+x\n".repeat(MAX_PATCH_BYTES))).is_err());
    }
}
