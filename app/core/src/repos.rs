//! The repositories a deployment declares, and a patch to one of them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::repositories::RepoFailure;
use suzatary_base::repo_ids::{RepoName, RepoNameError};

/// The most a patch may be: what a person reads on a phone before tapping.
pub(crate) const MAX_PATCH_BYTES: usize = 64 * 1024;

/// The most a public patch and its message may be after words not the owner's: a screenful.
pub(crate) const MAX_SHOWN_BYTES: usize = 4 * 1024;

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

/// The token for `[[secretary.repos]]` alone: their reads, and a tapped patch's push and pull
/// request.
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
    /// That a patch may be pushed here though the repository is public; absent, it takes none.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub public: bool,
}

/// The repositories the secretary may reach, all of one account; any other is refused by name.
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

    /// Whether a patch may be pushed to `repo`: declared, and if public, `public = true`.
    ///
    /// # Errors
    /// [`RepoFailure::Unknown`] or [`RepoFailure::Public`].
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
    Patch => |patch| String::from(patch.clone()),
);
prodrome_core::word_literal!(Visibility: Visibility::ALL => |visibility| visibility.as_str());

#[cfg(test)]
mod tests {
    use super::*;

    /// `rename_all` spells the generated `Visibility`, so serde's words are checked against
    /// `ALL`'s.
    #[test]
    fn a_visibilitys_serde_word_is_its_literals() {
        suzatary_base::words::assert_serde_words(&Visibility::ALL, |visibility| {
            visibility.as_str()
        });
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
