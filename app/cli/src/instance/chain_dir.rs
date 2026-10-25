//! A chain this deployment serves, and every chain in the file's order.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::bail;
use prodrome_core::event::Actor;
use prodrome_core::policy::Untrusted;
use prodrome_core::sign::PublicKey;
use suzatary_core::chains::ChainPolicy;

use super::file::Chain;
use super::{DataRoot, RESERVED};

/// One chain this deployment serves: its policy and its stores, together so a
/// chain can never be folded under another chain's trust policy.
#[derive(Debug, Clone)]
pub struct ChainDir {
    /// Whose events bind here, and which actors this store knows.
    pub policy: ChainPolicy,
    /// Where this chain's objects live, resolved under the data root.
    pub events: PathBuf,
    /// Whom a command with no `--actor` acts as: on the roster and trusted.
    pub owner: Actor,
}

impl ChainDir {
    /// One `[[chains]]` entry, its untrusted actors checked against its own
    /// roster: a name the roster does not know reads as configured and
    /// behaves as absent.
    fn of(path: &Path, data: &DataRoot, chain: Chain, root: &PublicKey) -> anyhow::Result<Self> {
        let roster: BTreeSet<_> = chain.actors.into_iter().collect();
        if let Some(stranger) = chain.untrusted.iter().find(|actor| !roster.contains(actor)) {
            bail!(
                "{}: chain {:?} calls {:?} untrusted and its roster does not name it",
                path.display(),
                chain.name.as_str(),
                stranger.as_str()
            );
        }
        if !roster.contains(&chain.owner) || chain.untrusted.contains(&chain.owner) {
            bail!(
                "{}: chain {:?} names {:?} its owner, who must be on its roster and trusted",
                path.display(),
                chain.name.as_str(),
                chain.owner.as_str()
            );
        }
        if RESERVED.contains(&chain.name.as_str()) {
            bail!(
                "{}: {:?} is a reserved chain name — the server's own route \
                 segments would shadow it ({})",
                path.display(),
                chain.name.as_str(),
                RESERVED.join(", ")
            );
        }
        Ok(Self {
            policy: ChainPolicy {
                name: chain.name,
                untrusted: Untrusted::of(chain.untrusted),
                roster,
                root: root.clone(),
            },
            events: data.under(&chain.events),
            owner: chain.owner,
        })
    }
}

/// Every chain's directories, as a head and a tail so there is always a first
/// chain for a verb with no `--chain`.
#[derive(Debug, Clone)]
pub struct ChainDirs {
    first: ChainDir,
    rest: Vec<ChainDir>,
}

impl ChainDirs {
    /// The first, what a verb with no `--chain` means.
    #[must_use]
    pub fn first(&self) -> &ChainDir {
        &self.first
    }

    /// Every chain, in the file's order.
    pub fn iter(&self) -> impl Iterator<Item = &ChainDir> {
        std::iter::once(&self.first).chain(&self.rest)
    }

    /// How many. At least one.
    #[must_use]
    pub fn count(&self) -> usize {
        1 + self.rest.len()
    }

    /// Every `[[chains]]` entry, with no two chains sharing a directory, so no chain is read
    /// under another's trust policy.
    pub(super) fn of(
        path: &Path,
        data: &DataRoot,
        chains: Vec<Chain>,
        root: &PublicKey,
    ) -> anyhow::Result<Self> {
        let built = chains
            .into_iter()
            .map(|chain| ChainDir::of(path, data, chain, root))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut seen: BTreeMap<&Path, String> = BTreeMap::new();
        for chain in &built {
            let named = chain.policy.name.to_string();
            if let Some(other) = seen.insert(&chain.events, named.clone()) {
                bail!(
                    "{}: chains {other} and {named} are the same directory ({})",
                    path.display(),
                    chain.events.display()
                );
            }
        }
        let mut walking = built.into_iter();
        let first = walking.next().ok_or_else(|| {
            anyhow::anyhow!("{}: a deployment names at least one chain", path.display())
        })?;
        Ok(Self {
            first,
            rest: walking.collect(),
        })
    }
}

impl<'a> IntoIterator for &'a ChainDirs {
    type Item = &'a ChainDir;
    type IntoIter = std::iter::Chain<std::iter::Once<&'a ChainDir>, std::slice::Iter<'a, ChainDir>>;
    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(&self.first).chain(self.rest.iter())
    }
}
