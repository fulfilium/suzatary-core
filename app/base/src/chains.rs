//! The chains a deployment names, one store per concern, each with its own trust policy and
//! roster.
//!
//! The policy is per chain and the door is not: who may open the app is the deployment's
//! (`suzatary_core::deployment::Deployment`), while whose events bind is a store's, because one
//! chain's untrusted agent is not the other's.
//!
//! Order is meaning here: the first chain is what the bare `/api/…` paths, the briefing, the
//! document and every verb with no `--chain` serve, so [`Chains`] is an ordered non-empty list,
//! not a map.

use std::collections::BTreeSet;

use prodrome_core::event::Actor;
use prodrome_core::policy::Untrusted;
use prodrome_core::sign::{PublicKey, Registrar};

/// A chain's name, one vocabulary for the instance file, a URL segment and a `--chain`.
///
/// A slug because it is a path segment: that refuses `..`, an empty segment and anything a
/// router would have to normalise before matching.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct ChainName(String);

/// A name that is not a slug.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a chain name is 1-32 characters of a-z, 0-9 and -, starting with a letter: {0:?}")]
pub struct NotAName(pub String);

impl ChainName {
    /// Parse a slug.
    ///
    /// # Errors
    /// When it is empty, too long, does not start with a lowercase letter, or
    /// holds anything but lowercase letters, digits and hyphens.
    pub fn new(text: impl Into<String>) -> Result<Self, NotAName> {
        let text = text.into();
        let ok = matches!(text.len(), 1..=32)
            && text.starts_with(|c: char| c.is_ascii_lowercase())
            && text
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        if ok {
            Ok(Self(text))
        } else {
            Err(NotAName(text))
        }
    }

    /// The name as it is spelled everywhere.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ChainName {
    type Error = NotAName;
    fn try_from(text: String) -> Result<Self, NotAName> {
        Self::new(text)
    }
}

impl From<ChainName> for String {
    fn from(name: ChainName) -> Self {
        name.0
    }
}

impl std::fmt::Display for ChainName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One chain's own rules: whose events bind, and which actors it recognises.
///
/// It carries no path: where a chain lives is the binary's business, so the library never
/// holds a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainPolicy {
    /// The name, as a URL segment and a `--chain` spell it.
    pub name: ChainName,
    /// This chain's whole trust policy: actors whose events are recorded, surfaced for
    /// confirmation, and inert to the fold.
    pub untrusted: Untrusted,
    /// Every actor this chain recognises; a name on a write is tested against it.
    pub roster: BTreeSet<Actor>,
    /// The ROOT key whose signatures make a key object count (SPEC §5): every write must be
    /// proven by a key it registered, every device request signed by one, and every deployment
    /// declares one.
    pub root: PublicKey,
}

impl ChainPolicy {
    /// The untrusted actors as the wire spells them, sorted, so the browser folds under this
    /// chain's policy rather than a guess.
    #[must_use]
    pub fn untrusted_names(&self) -> Vec<String> {
        self.untrusted
            .actors()
            .iter()
            .map(|actor| actor.as_str().to_owned())
            .collect()
    }

    /// Whose signatures register a key here: the root alone.
    #[must_use]
    pub fn registrar(&self) -> Registrar {
        Registrar::Roots([self.root.clone()].into_iter().collect())
    }
}

/// Every chain this deployment serves, in the file's order, with the first one distinguished.
///
/// A head and a tail, not a `Vec`, so an empty list cannot be built and [`Chains::first`]
/// cannot fail. Names are unique by construction in [`Chains::of`], so a name picks one chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chains {
    /// The first: what an unnamed chain means.
    first: ChainPolicy,
    /// The rest, in the file's order.
    rest: Vec<ChainPolicy>,
}

/// A list of chains that is not one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotChains {
    /// No chains at all.
    #[error("a deployment names at least one chain")]
    Empty,
    /// Two entries with one name.
    #[error("two chains are named {0:?}; a name is how a route and a verb pick one")]
    Twice(String),
}

impl Chains {
    /// Build the list, refusing an empty one and a repeated name.
    ///
    /// # Errors
    /// [`NotChains::Empty`] and [`NotChains::Twice`].
    pub fn of(policies: Vec<ChainPolicy>) -> Result<Self, NotChains> {
        let mut walking = policies.into_iter();
        let first = walking.next().ok_or(NotChains::Empty)?;
        let rest: Vec<ChainPolicy> = walking.collect();
        let mut seen = BTreeSet::new();
        seen.insert(first.name.clone());
        for policy in &rest {
            if !seen.insert(policy.name.clone()) {
                return Err(NotChains::Twice(policy.name.as_str().to_owned()));
            }
        }
        Ok(Self { first, rest })
    }

    /// The first chain: what the bare paths, the briefing and a verb with no
    /// `--chain` mean.
    #[must_use]
    pub fn first(&self) -> &ChainPolicy {
        &self.first
    }

    /// One chain by name, or nothing.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ChainPolicy> {
        self.iter().find(|policy| policy.name.as_str() == name)
    }

    /// Every chain, in the file's order.
    pub fn iter(&self) -> impl Iterator<Item = &ChainPolicy> {
        std::iter::once(&self.first).chain(&self.rest)
    }

    /// Every name, in the file's order.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.iter().map(|policy| policy.name.as_str()).collect()
    }

    /// How many. At least one.
    #[must_use]
    pub fn count(&self) -> usize {
        1 + self.rest.len()
    }
}

impl<'a> IntoIterator for &'a Chains {
    type Item = &'a ChainPolicy;
    type IntoIter =
        std::iter::Chain<std::iter::Once<&'a ChainPolicy>, std::slice::Iter<'a, ChainPolicy>>;
    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(&self.first).chain(self.rest.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name is a path segment, and the type refuses what a route would otherwise have to.
    #[test]
    fn a_chain_name_is_a_slug() {
        for good in ["life", "suzatary", "a", "x9", "two-words", &"a".repeat(32)] {
            assert!(ChainName::new(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "..",
            "Life",
            "9lives",
            "-lead",
            "has space",
            "has/slash",
            "has_underscore",
            "%2e%2e",
            &"a".repeat(33),
        ] {
            assert!(ChainName::new(bad).is_err(), "{bad:?}");
        }
    }
}
