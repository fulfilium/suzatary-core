//! The chains a deployment names, each with its own trust policy and roster.
//!
//! Who may open the app is the deployment's choice; whose events bind is each chain's.
//!
//! The first chain is what a path or a verb with no chain serves, so [`Chains`] is ordered.

use std::collections::{BTreeMap, BTreeSet};

use prodrome_dag::sign::Registrar;
use prodrome_data::event::Actor;
use prodrome_data::policy::Untrusted;
use prodrome_data::sign::PublicKey;

/// A chain's name: a slug, so a path segment refuses `..` and anything a router normalises.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(try_from = "String", into = "String")]
pub struct ChainName(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a chain name is 1-32 characters of a-z, 0-9 and -, starting with a letter: {0:?}")]
pub struct NotAName(pub String);

impl ChainName {
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
/// It carries no path: where a chain lives is the binary's business.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainPolicy {
    pub name: ChainName,
    /// This chain's whole trust policy: actors whose events are recorded, surfaced for
    /// confirmation, and inert to the fold.
    pub untrusted: Untrusted,
    /// Every actor this chain recognises; a name on a write is tested against it.
    pub roster: BTreeSet<Actor>,
    /// The ROOT key whose signatures make a key object count (SPEC §5).
    pub root: PublicKey,
    pub naming: Naming,
}

/// The name a deployment's actor (a `[web.act_as]` value, a `--actor`) writes under on a chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Naming {
    /// A private chain's: every actor writes under its own name.
    AsActs,
    /// A public store's (`[[chains]] public_names`): an actor writes under its public name, and
    /// an actor with none writes nothing there. The roster is the public names.
    Published(BTreeMap<Actor, Actor>),
}

/// An actor with no public name, writing to a public store.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "chain {chain} is public, and {actor} has no public name there: an event written to a public \
     store names only a public name ([[chains]] public_names)"
)]
pub struct Unnamed {
    pub chain: String,
    pub actor: String,
}

impl ChainPolicy {
    /// Whether its store keeps key objects: a public one keeps none.
    #[must_use]
    pub fn keeps_keys(&self) -> bool {
        match self.naming {
            Naming::AsActs => true,
            Naming::Published(_) => false,
        }
    }

    /// The name `actor`'s events carry on this chain: its own on a private chain; on a public
    /// one, its own where that is already a public name (on the roster), else its public name.
    pub fn named(&self, actor: &Actor) -> Result<Actor, Unnamed> {
        match &self.naming {
            Naming::AsActs => Ok(actor.clone()),
            Naming::Published(_) if self.roster.contains(actor) => Ok(actor.clone()),
            Naming::Published(names) => names.get(actor).cloned().ok_or_else(|| Unnamed {
                chain: self.name.to_string(),
                actor: actor.as_str().to_owned(),
            }),
        }
    }
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

/// Every chain in the file's order: a head and a tail, so the list is never empty.
/// [`Chains::of`] makes the names unique.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chains {
    /// The first: what an unnamed chain means.
    first: ChainPolicy,
    rest: Vec<ChainPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotChains {
    #[error("a deployment names at least one chain")]
    Empty,
    #[error("two chains are named {0:?}; a name is how a route and a verb pick one")]
    Twice(String),
}

impl Chains {
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

    #[must_use]
    pub fn first(&self) -> &ChainPolicy {
        &self.first
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ChainPolicy> {
        self.iter().find(|policy| policy.name.as_str() == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ChainPolicy> {
        std::iter::once(&self.first).chain(&self.rest)
    }

    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.iter().map(|policy| policy.name.as_str()).collect()
    }

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
