use suzatary_base::chains::{ChainName, ChainPolicy};
use suzatary_record::record::TodoEvent;
use suzatary_store::store::{Prodrome, StoreError};

// Test support, never production.
#[cfg(any(test, feature = "nothing"))]
#[path = "../../tests/support/keeping.rs"]
mod stores;

/// The chains that keep keys, at least one; the first keeps a trust card's endorsement.
#[derive(Debug, Clone)]
pub struct Keyed<T> {
    first: T,
    rest: Vec<T>,
}

/// A deployment's chains as keys see them; built only by [`Keeping::of`].
#[derive(Debug, Clone)]
pub enum Keeping<T> {
    /// Every chain is public.
    Keyless,
    Keyed(Keyed<T>),
}

#[derive(Clone)]
pub struct Keeper<'a> {
    pub name: ChainName,
    pub store: &'a dyn Prodrome<TodoEvent>,
}

/// A keyed chain that did not read: the box's failure, and no answer about any key.
#[derive(Debug, thiserror::Error)]
#[error("the chain {chain} did not read: {error}")]
pub struct Unread {
    pub chain: ChainName,
    pub error: StoreError,
}

impl<T> Keyed<T> {
    #[must_use]
    pub fn first(&self) -> &T {
        &self.first
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.first).chain(&self.rest)
    }

    #[must_use]
    pub fn map<'s, U>(&'s self, mut f: impl FnMut(&'s T) -> U) -> Keyed<U> {
        Keyed {
            first: f(&self.first),
            rest: self.rest.iter().map(f).collect(),
        }
    }

    pub fn try_map<'s, U, X>(
        &'s self,
        mut f: impl FnMut(&'s T) -> Result<U, X>,
    ) -> Result<Keyed<U>, X> {
        Ok(Keyed {
            first: f(&self.first)?,
            rest: self.rest.iter().map(f).collect::<Result<_, _>>()?,
        })
    }
}

impl<T> Keeping<T> {
    /// Those of `chains` whose `policy` keeps keys.
    pub fn of(chains: impl IntoIterator<Item = T>, policy: impl Fn(&T) -> &ChainPolicy) -> Self {
        let mut keyed = chains
            .into_iter()
            .filter(|chain| policy(chain).keeps_keys());
        match keyed.next() {
            None => Keeping::Keyless,
            Some(first) => Keeping::Keyed(Keyed {
                first,
                rest: keyed.collect(),
            }),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        let keyed = match self {
            Keeping::Keyless => None,
            Keeping::Keyed(keyed) => Some(keyed.iter()),
        };
        keyed.into_iter().flatten()
    }

    #[must_use]
    pub fn map<'s, U>(&'s self, f: impl FnMut(&'s T) -> U) -> Keeping<U> {
        match self {
            Keeping::Keyless => Keeping::Keyless,
            Keeping::Keyed(keyed) => Keeping::Keyed(keyed.map(f)),
        }
    }
}
