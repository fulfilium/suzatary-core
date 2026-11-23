//! A tile: a page and the identity its running program is kept by.

/// Fresh for each page opened and never reused within one lineage of `Tiles`, so a host never
/// feeds a program that was closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(u64);

/// Only `Tiles` makes one, from its counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf<P> {
    id: Id,
    page: P,
}

impl Id {
    /// The number a host keys the tile's element by.
    #[must_use]
    pub fn key(self) -> u64 {
        self.0
    }
}

impl<P> Leaf<P> {
    pub(crate) fn new(id: u64, page: P) -> Leaf<P> {
        Leaf { id: Id(id), page }
    }

    pub fn id(&self) -> Id {
        self.id
    }

    pub fn page(&self) -> &P {
        &self.page
    }
}
