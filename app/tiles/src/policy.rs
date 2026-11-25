//! What a host decides about how a workspace grows: the bound on its free tiles and what an open
//! beside the focused tile does with the tiles after it.

use std::num::NonZeroU8;

/// One value for every tree of a lineage: `Tiles::new` takes it, and `act`, `arrange`, `print`
/// and `parse` read it from the tiles. Start from `Policy::default()`, the rule of Miller
/// columns, and change it with `with_most` and `with_beside`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    most: NonZeroU8,
    beside: Beside,
}

/// What an open beside the focused tile does with the free tiles after it in its node. Either
/// way the page stands right after the focused tile, takes the focus and stays before a pinned
/// end; where the workspace holds the policy's `most` free tiles already, the page replaces the
/// focused leaf instead, under both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Beside {
    /// They close, and the page takes the share of the first: each page opens to the right of the
    /// page it was opened from and replaces what stood there.
    Replace,
    /// They stay, after the page, which takes the share it declares (`Page::share`).
    Insert,
}

impl Policy {
    /// The same policy with `most` as its bound on a workspace's free tiles. A `u8` keeps the
    /// address reader's nesting, which the bound limits, shallow for any host.
    #[must_use]
    pub fn with_most(self, most: NonZeroU8) -> Policy {
        Policy { most, ..self }
    }

    #[must_use]
    pub fn with_beside(self, beside: Beside) -> Policy {
        Policy { beside, ..self }
    }

    /// The most free tiles a workspace holds; its pinned end is not counted, so a docked page
    /// never costs a person a page. An open or a `Below` that would make more opens here, so a
    /// bound of one opens every page in place of the focused one.
    #[must_use]
    pub fn most(self) -> NonZeroU8 {
        self.most
    }

    #[must_use]
    pub fn beside(self) -> Beside {
        self.beside
    }

    pub(crate) fn bound(self) -> usize {
        usize::from(self.most.get())
    }
}

impl Default for Policy {
    fn default() -> Policy {
        Policy {
            most: NonZeroU8::MIN.saturating_add(3),
            beside: Beside::Replace,
        }
    }
}
