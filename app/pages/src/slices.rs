//! A long list drawn a slice per step: a first reading draws none of it (that step decodes it),
//! the next steps a slice each, and what a later reading adds is drawn with it.

pub(crate) const SLICE: usize = 2;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Slices {
    shown: usize,
}

impl Slices {
    /// `before` items were held, `now` are.
    pub(crate) fn read(&mut self, first: bool, before: usize, now: usize) {
        self.shown = if first {
            0
        } else {
            now.min(self.shown + now.saturating_sub(before))
        };
    }

    pub(crate) fn more(&mut self, held: usize) {
        self.shown = held.min(self.shown + SLICE);
    }

    #[must_use]
    pub(crate) const fn whole(self, held: usize) -> bool {
        self.shown >= held
    }

    #[must_use]
    pub(crate) const fn shown(self) -> usize {
        self.shown
    }
}
